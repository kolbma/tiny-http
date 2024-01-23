use std::convert::TryFrom;
use std::io::{BufReader, BufWriter, Read};
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};
use std::net::SocketAddr;
#[cfg(feature = "socket2")]
use std::sync::Arc;

use ascii::{AsciiChar, AsciiStr, AsciiString};

use crate::common::{HttpVersion, Method};
use crate::log;
use crate::request;
use crate::request::RequestCreateError;
use crate::util::ArcRegistration;
use crate::util::RefinedTcpStream;
use crate::util::{SequentialReader, SequentialReaderBuilder, SequentialWriterBuilder};
use crate::ConnectionHeader;
use crate::Header;
use crate::Request;
use crate::Response;
#[cfg(feature = "socket2")]
use crate::SocketConfig;
use crate::StatusCode;

/// A `ClientConnection` is an object that will store a socket to a client
/// and return Request objects.
pub(crate) struct ClientConnection {
    /// store registration to count all open ClientConnection
    _client_counter: ArcRegistration,

    /// set to true if we know that the previous request is the last one
    is_connection_close: bool,

    /// Reader to read the next header from
    next_header_source: SequentialReader<BufReader<RefinedTcpStream>>,

    /// address of the client
    remote_addr: IoResult<Option<SocketAddr>>,

    /// true if the connection goes through SSL
    secure: bool,

    /// sequence of Writers to the stream, to avoid writing response #2 before
    ///  response #1
    sink: SequentialWriterBuilder<BufWriter<RefinedTcpStream>>,

    /// config for tcp sockets
    #[cfg(feature = "socket2")]
    socket_config: Arc<SocketConfig>,

    /// sequence of Readers to the stream, so that the data is not read in
    ///  the wrong order
    source: SequentialReaderBuilder<BufReader<RefinedTcpStream>>,
}

impl ClientConnection {
    /// Creates a new `ClientConnection` that takes ownership of the `TcpStream`.
    pub(crate) fn new(
        write_socket: RefinedTcpStream,
        mut read_socket: RefinedTcpStream,
        client_counter: ArcRegistration,
        #[cfg(feature = "socket2")] socket_config: &Arc<SocketConfig>,
    ) -> Self {
        let remote_addr = read_socket.peer_addr();
        let secure = read_socket.secure();

        let mut source = SequentialReaderBuilder::new(BufReader::with_capacity(1024, read_socket));
        let first_header = source.next().unwrap();

        log::info!(
            "connection [{}] count [{}]",
            remote_addr.as_ref().ok().map_or(String::default(), |a| {
                a.map_or(String::default(), |a| a.to_string())
            }),
            client_counter.value()
        );

        ClientConnection {
            _client_counter: client_counter,
            is_connection_close: false,
            next_header_source: first_header,
            remote_addr,
            secure,
            sink: SequentialWriterBuilder::new(BufWriter::with_capacity(1024, write_socket)),
            #[cfg(feature = "socket2")]
            socket_config: Arc::clone(socket_config),
            source,
        }
    }

    /// true if the connection is HTTPS
    pub(crate) fn secure(&self) -> bool {
        self.secure
    }

    /// Reads the next line from `self.next_header_source`.
    ///
    /// Reads until `CRLF` is reached. The next read will start
    ///  at the first byte of the new line.
    ///
    /// The overall header limit is 8K.
    /// The limit per header line is 2K.
    fn read_next_line(&mut self) -> Result<AsciiString, ReadError> {
        let mut buf = Vec::new();
        let mut prev_byte = 0u8;

        loop {
            let byte = self.next_header_source.by_ref().bytes().next();

            let byte = if let Some(byte) = byte {
                byte?
            } else {
                log::debug!("unexpected connection abort");
                return Err(IoError::new(
                    IoErrorKind::ConnectionAborted,
                    "unexpected connection abort",
                )
                .into());
            };

            if byte == b'\n' && prev_byte == b'\r' {
                let _ = buf.pop(); // removing the '\r'
                return AsciiString::from_ascii(buf).map_err(|_| {
                    log::debug!("header no ascii");
                    IoError::new(IoErrorKind::InvalidInput, "header no ascii").into()
                });
            }
            prev_byte = byte;

            if buf.len() >= 2048 {
                return Err(ReadError::HttpProtocol(HttpVersion::Version1_0, 431.into()));
            }

            buf.push(byte);
        }
    }

    /// Reads a request from the stream.
    /// Blocks until the header has been read.
    ///
    /// The overall header limit is 8K.
    /// The limit per header line is 2K.
    fn read(&mut self) -> Result<Request, ReadError> {
        let (method, path, version, headers) = {
            let mut header_limit_rest = 8_192_usize;

            // reading the request line
            let (method, path, version) = {
                let line = self.read_next_line().map_err(|err| {
                    match err {
                        ReadError::HttpProtocol(v, status) if status == 431 => {
                            // match to 414 URI Too Long for request line
                            ReadError::HttpProtocol(v, 414.into())
                        }
                        _ => err,
                    }
                })?;

                header_limit_rest = header_limit_rest.checked_sub(line.len()).ok_or_else(|| {
                    // Request Header Fields Too Large
                    ReadError::HttpProtocol(HttpVersion::Version1_0, 431.into())
                })?;

                parse_request_line(line.trim())?
            };

            // getting all headers
            let headers = {
                let mut headers = Vec::new();
                loop {
                    let line = self.read_next_line()?;

                    header_limit_rest =
                        header_limit_rest.checked_sub(line.len()).ok_or_else(|| {
                            // Request Header Fields Too Large
                            ReadError::HttpProtocol(version, 431.into())
                        })?;

                    let line = line.trim();

                    if line.is_empty() {
                        break;
                    }

                    headers.push(match Header::try_from(line) {
                        Ok(h) => h,
                        _ => return Err(ReadError::WrongHeader(version)),
                    });
                }

                headers
            };

            (method, path, version, headers)
        };

        log::debug!("{method} {path} {}", version.header());

        // building the writer for the request
        let writer = self.sink.next().unwrap();

        // follow-up for next potential request
        let mut data_source = self.source.next().unwrap();
        std::mem::swap(&mut self.next_header_source, &mut data_source);

        // building the next reader
        let request = crate::request::new_request(
            self.secure,
            method,
            path.to_string(),
            version,
            headers,
            *self.remote_addr.as_ref().unwrap(),
            data_source,
            writer,
        )
        .map_err(|err| {
            log::warn!("request: {err}");
            ReadError::from((err, version))
        })?;

        // return the request
        Ok(request)
    }
}

impl Iterator for ClientConnection {
    type Item = Result<Request, ReadError>;

    /// Blocks until the next Request is available.
    /// Returns None when no new Requests will come from the client.
    fn next(&mut self) -> Option<Result<Request, ReadError>> {
        // the client sent a "connection: close" header in this previous request
        //  or is using HTTP 1.0, meaning that no new request will come
        if self.is_connection_close {
            log::debug!("connection close");
            return None;
        }

        loop {
            let rq = self.read().map_err(|err| {
                match err {
                    ReadError::WrongRequestLine => {
                        send_error_response(self, 400.into(), None, false);
                        // we don't know where the next request would start,
                        // so we have to close
                    }

                    ReadError::WrongHeader(ver) => {
                        send_error_response(self, 400.into(), Some(ver), false);
                        // we don't know where the next request would start,
                        // so we have to close
                    }

                    ReadError::HttpProtocol(ver, status) => {
                        send_error_response(self, status, Some(ver), false);
                        // we don't know where the next request would start,
                        // so we have to close
                    }

                    ReadError::ReadIoError(ref inner_err)
                        if inner_err.kind() == IoErrorKind::TimedOut =>
                    {
                        // request timeout
                        // closing the connection
                        // return send_error_response(self, 408.into(), None, false);
                        let _ = inner_err;
                        log::debug!("close cause: {inner_err}");
                    }

                    ReadError::ExpectationFailed(ver) => {
                        // TODO: should be recoverable, but needs handling in case of body
                        send_error_response(self, 417.into(), Some(ver), true);
                    }

                    ReadError::ReadIoError(ref inner_err) => {
                        let _ = inner_err.to_string();
                        log::debug!("close cause: {inner_err}");
                        // closing the connection
                    }
                };

                err
            });

            // check if request available
            let rq = if let Ok(rq) = rq {
                rq
            } else {
                self.is_connection_close = true;
                // return with ReadError
                return Some(rq);
            };

            // checking HTTP version <= HTTP/1.1
            if rq.http_version() > HttpVersion::Version1_1 {
                let writer = self.sink.next().unwrap();
                let response = Response::from_string(
                    "This server only supports HTTP versions 1.0 and 1.1".to_owned(),
                )
                .with_status_code(StatusCode(505));
                let _ = response.raw_print(writer, HttpVersion::Version1_0, &[], false, None);
                continue;
            }

            // updating the status of the connection
            let connection_header = rq
                .headers()
                .iter()
                .find(|h| h.field.equiv("Connection"))
                .map(|h| h.value.as_str());

            let lowercase = connection_header.map(str::to_ascii_lowercase);

            let mut rq = rq;

            match lowercase {
                Some(ref val) if val.contains("close") => set_connection_close(self, &mut rq, true),
                Some(ref val) if val.contains("upgrade") => {
                    set_connection_close(self, &mut rq, false);
                }
                // <= HTTP/1.0 is always close
                _ if rq.http_version() <= HttpVersion::Version1_0 => {
                    set_connection_close(self, &mut rq, true);
                }
                #[cfg(feature = "socket2")]
                _ => {
                    if !self.socket_config.tcp_keep_alive {
                        set_connection_close(self, &mut rq, true);
                    }
                }
                #[cfg(not(feature = "socket2"))]
                _ => {}
            };

            let rq = rq;

            // returning the request
            return Some(Ok(rq));
        }
    }
}

#[inline]
fn send_error_response(
    client_connection: &mut ClientConnection,
    status: StatusCode,
    version: Option<HttpVersion>,
    do_not_send_body: bool,
) {
    let version = version.unwrap_or(HttpVersion::Version1_0);

    let writer = client_connection.sink.next().unwrap();
    let msg = status.default_reason_phrase();
    let response = Response::empty(status).with_data(msg.as_bytes(), Some(msg.len()));

    log::info!(
        "send error response [{}] ({})",
        client_connection
            .remote_addr
            .as_ref()
            .ok()
            .map_or(String::default(), |a| {
                a.map_or(String::default(), |a| a.to_string())
            }),
        response.status_code()
    );

    let _ = response.raw_print(writer, version, &[], do_not_send_body, None);
}

#[inline]
fn set_connection_close(c: &mut ClientConnection, rq: &mut Request, is_close: bool) {
    c.is_connection_close = is_close;
    if is_close {
        rq.set_connection_header(Some(ConnectionHeader::Close));
    }
}

/// Error that can happen when reading a request.
#[derive(Debug)]
pub(crate) enum ReadError {
    HttpProtocol(HttpVersion, StatusCode),
    WrongRequestLine,
    WrongHeader(HttpVersion),
    /// the client sent an unrecognized `Expect` header
    ExpectationFailed(HttpVersion),
    ReadIoError(IoError),
}

impl std::error::Error for ReadError {}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HttpProtocol(v, status) => write!(
                f,
                "{} {} {}",
                v.header(),
                status.0,
                status.default_reason_phrase()
            ),
            Self::WrongRequestLine => f.write_str("no request"),
            Self::WrongHeader(v) => write!(f, "{} unsupported header", v.header()),
            Self::ExpectationFailed(v) => {
                write!(f, "{} unrecognized Expect", v.header())
            }
            Self::ReadIoError(err) => err.fmt(f),
        }
    }
}

impl From<IoError> for ReadError {
    fn from(err: IoError) -> Self {
        Self::ReadIoError(err)
    }
}

impl From<ReadError> for IoError {
    fn from(err: ReadError) -> Self {
        match err {
            ReadError::ReadIoError(err) => err,
            _ => IoError::new(IoErrorKind::InvalidInput, "request invalid"),
        }
    }
}

impl From<(RequestCreateError, HttpVersion)> for ReadError {
    fn from((err, version): (RequestCreateError, HttpVersion)) -> Self {
        match err {
            request::RequestCreateError::ContentLength => {
                ReadError::HttpProtocol(version, 400.into())
            }
            request::RequestCreateError::CreationIoError(err) => ReadError::ReadIoError(err),
            request::RequestCreateError::ExpectationFailed => ReadError::ExpectationFailed(version),
        }
    }
}

/// Parses the request line of the request.
/// eg. GET / HTTP/1.1
fn parse_request_line(line: &AsciiStr) -> Result<(Method, AsciiString, HttpVersion), ReadError> {
    let mut parts = line.split(AsciiChar::Space);

    let method = parts.next().map(Method::from);
    let path = parts.next().map(ToOwned::to_owned);
    let version = parts.next().and_then(|w| HttpVersion::try_from(w).ok());

    method
        .and_then(|method| Some((method, path?, version?)))
        .ok_or(ReadError::WrongRequestLine)
}

#[cfg(test)]
mod test {
    use ascii::AsAsciiStr;

    use crate::HttpVersion;

    #[test]
    fn test_parse_request_line() {
        let (method, path, ver) =
            super::parse_request_line("GET /hello HTTP/1.1".as_ascii_str().unwrap()).unwrap();

        assert!(method == crate::Method::Get);
        assert!(path == "/hello");
        assert!(ver == HttpVersion::Version1_1);

        assert!(super::parse_request_line("GET /hello".as_ascii_str().unwrap()).is_err());
        assert!(super::parse_request_line("qsd qsd qsd".as_ascii_str().unwrap()).is_err());

        let (method, _, _) =
            super::parse_request_line("loGET /hello HTTP/1.1".as_ascii_str().unwrap()).unwrap();
        assert_eq!(
            method,
            crate::Method::NonStandard(Some("loGET".parse().unwrap()))
        );
    }
}
