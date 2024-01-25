use std::convert::TryFrom;
use std::io::{BufReader, BufWriter, Read};
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};
use std::net::SocketAddr;
#[cfg(feature = "socket2")]
use std::sync::Arc;

use ascii::{AsciiChar, AsciiStr, AsciiString};

use crate::common::{HttpVersion, Method};
use crate::request;
use crate::response::Standard::{
    BadRequest400, ExpectationFailed417, HttpVersionNotSupported505,
    RequestHeaderFieldsTooLarge431, UriTooLong414,
};
use crate::util::{
    ArcRegistration, RefinedTcpStream, SequentialReader, SequentialReaderBuilder,
    SequentialWriterBuilder,
};
use crate::ConnectionHeader;
use crate::Header;
use crate::Request;
#[cfg(feature = "socket2")]
use crate::SocketConfig;
use crate::{log, response};

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
                return Err(ReadError::HttpProtocol(
                    HttpVersion::Version1_0,
                    RequestHeaderFieldsTooLarge431,
                ));
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
                        ReadError::HttpProtocol(v, RequestHeaderFieldsTooLarge431) => {
                            // match to 414 URI Too Long for request line
                            ReadError::HttpProtocol(v, UriTooLong414)
                        }
                        _ => err,
                    }
                })?;

                header_limit_rest = header_limit_rest.checked_sub(line.len()).ok_or(
                    // Request Header Fields Too Large
                    ReadError::HttpProtocol(
                        HttpVersion::Version1_0,
                        RequestHeaderFieldsTooLarge431,
                    ),
                )?;

                parse_request_line(line.trim())?
            };

            // getting all headers
            let headers = {
                let mut headers = Vec::new();
                loop {
                    let line = self.read_next_line()?;

                    header_limit_rest = header_limit_rest.checked_sub(line.len()).ok_or(
                        // Request Header Fields Too Large
                        ReadError::HttpProtocol(version, RequestHeaderFieldsTooLarge431),
                    )?;

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

        let rq = self.read().map_err(|err| {
            match err {
                ReadError::WrongRequestLine => {
                    send_error_std_response(self, BadRequest400, None, false);
                    // we don't know where the next request would start,
                    // so we have to close
                }

                ReadError::WrongHeader(ver) => {
                    send_error_std_response(self, BadRequest400, Some(ver), false);
                    // we don't know where the next request would start,
                    // so we have to close
                }

                ReadError::HttpProtocol(ver, status) => {
                    send_error_std_response(self, status, Some(ver), false);
                    // we don't know where the next request would start,
                    // so we have to close
                }

                ReadError::WrongVersion(_ver) => {
                    send_error_std_response(self, HttpVersionNotSupported505, None, false);
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
                    send_error_std_response(self, ExpectationFailed417, Some(ver), true);
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

        // updating the status of the connection
        let connection_header = rq
            .headers()
            .iter()
            .find_map(|h| {
                if h.field.equiv("Connection") {
                    if let Ok(connection_header) = ConnectionHeader::try_from(h) {
                        Some(Some(connection_header))
                    } else {
                        // return after first connection header, also not parseable
                        Some(None)
                    }
                } else {
                    None
                }
            })
            .flatten();

        let mut rq = rq;

        // handle Connection header - see also `[Request::respond]`
        match connection_header {
            Some(ConnectionHeader::Close) => set_connection_close(self, &mut rq, true),
            Some(ConnectionHeader::Upgrade) => {
                set_connection_close(self, &mut rq, false);
            }
            // HTTP/1.0 does a upgrade to 1.1 with keep-alive set
            Some(ConnectionHeader::KeepAlive) if rq.http_version() == HttpVersion::Version1_0 =>
            {
                #[cfg(feature = "socket2")]
                if !self.socket_config.tcp_keep_alive {
                    set_connection_close(self, &mut rq, true);
                }
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
        Some(Ok(rq))
    }
}

// TODO: remove commented code after benchmarking
// #[inline]
// fn send_error_response(
//     client_connection: &mut ClientConnection,
//     status: impl Into<StatusCode>,
//     version: Option<HttpVersion>,
//     do_not_send_body: bool,
// ) {
//     let version = version.unwrap_or(HttpVersion::Version1_0);

//     let writer = client_connection.sink.next().unwrap();
//     let response = Response::from(status);

//     log::info!(
//         "send error response [{}] ({})",
//         client_connection
//             .remote_addr
//             .as_ref()
//             .ok()
//             .map_or(String::default(), |a| {
//                 a.map_or(String::default(), |a| a.to_string())
//             }),
//         response.status_code()
//     );

//     let _ = response.raw_print(writer, version, &[], do_not_send_body, None);
// }

#[inline]
fn send_error_std_response(
    client_connection: &mut ClientConnection,
    status: response::Standard,
    version: Option<HttpVersion>,
    do_not_send_body: bool,
) {
    let version = version.unwrap_or(HttpVersion::Version1_0);

    let writer = client_connection.sink.next().unwrap();
    let response = <&response::StandardResponse>::from(&status);

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

    let _ = response.raw_print2(writer, version, &[], do_not_send_body, None);
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
    /// the client sent an unrecognized `Expect` header
    ExpectationFailed(HttpVersion),
    HttpProtocol(HttpVersion, response::Standard),
    ReadIoError(IoError),
    WrongHeader(HttpVersion),
    WrongRequestLine,
    WrongVersion(Option<HttpVersion>),
}

impl std::error::Error for ReadError {}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExpectationFailed(v) => {
                write!(f, "{} unrecognized Expect", v.header())
            }
            Self::HttpProtocol(v, status) => {
                let response = <&response::StandardResponse>::from(status);
                write!(
                    f,
                    "{} {} {}",
                    v.header(),
                    response.status_code(),
                    response.reason_phrase()
                )
            }
            Self::ReadIoError(err) => err.fmt(f),
            Self::WrongHeader(v) => write!(f, "{} unsupported header", v.header()),
            Self::WrongRequestLine => f.write_str("no request"),
            Self::WrongVersion(v) => write!(
                f,
                "{} unsupported version",
                v.map(|v| v.header()).unwrap_or_default()
            ),
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

impl From<(request::CreateError, HttpVersion)> for ReadError {
    fn from((err, version): (request::CreateError, HttpVersion)) -> Self {
        match err {
            request::CreateError::ContentLength => ReadError::HttpProtocol(version, BadRequest400),
            request::CreateError::IoError(err) => ReadError::ReadIoError(err),
            request::CreateError::Expect => ReadError::ExpectationFailed(version),
        }
    }
}

/// Parses the request line of the request.
/// eg. GET / HTTP/1.1
/// At the moment supporting 0.9, 1.0, 1.1
fn parse_request_line(line: &AsciiStr) -> Result<(Method, AsciiString, HttpVersion), ReadError> {
    let mut parts = line.split(AsciiChar::Space);

    let method = parts.next().map(Method::from);
    let path = parts.next().map(ToOwned::to_owned);
    let version = parts
        .next()
        .map(|w| {
            if let Ok(ver) = HttpVersion::try_from(w) {
                return if ver <= HttpVersion::Version1_1 {
                    // only up to 1.1 supported
                    Ok(ver)
                } else {
                    Err(ReadError::WrongVersion(Some(ver)))
                };
            }
            Err(ReadError::WrongVersion(None))
        })
        .ok_or(ReadError::WrongRequestLine)??;

    method
        .and_then(|method| Some((method, path?, version)))
        .ok_or(ReadError::WrongRequestLine)
}

#[cfg(test)]
mod test {

    use ascii::AsAsciiStr;

    use crate::HttpVersion;

    #[test]
    fn parse_request_line_test() {
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
