use std::convert::TryFrom;
use std::io::{BufReader, BufWriter, Read};
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};
use std::net::SocketAddr;
use std::sync::Arc;

use ascii::{AsciiChar, AsciiStr, AsciiString};

use crate::common::{HttpVersion, Method};
use crate::response::Standard::{
    BadRequest400, ExpectationFailed417, HttpVersionNotSupported505,
    RequestHeaderFieldsTooLarge431, RequestTimeout408, UriTooLong414,
};
use crate::util::registration::ArcRegistrationU16;
use crate::util::{
    RefinedTcpStream, SequentialReader, SequentialReaderBuilder, SequentialWriterBuilder,
};
use crate::Request;
#[cfg(feature = "socket2")]
use crate::SocketConfig;
use crate::{limits, Header};
use crate::{log, response};
use crate::{request, ConnectionHeader};
use crate::{ConnectionValue, LimitsConfig};

const CR: u8 = b'\r';
const NL: u8 = b'\n';
const HEADER_READER_BUF_MAX_IDX: usize = limits::HEADER_READER_BUF_SIZE - 1;

/// A `ClientConnection` is an object that will store a socket to a client
/// and return Request objects.
pub(crate) struct ClientConnection {
    /// store registration to count all open ClientConnection
    _client_counter: ArcRegistrationU16,

    /// set to true if we know that the previous request is the last one
    is_connection_close: bool,

    /// configuration of limits
    limits: Arc<LimitsConfig>,

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

/// Checks line length limit in `read_next_line()`
macro_rules! check_line_limit {
    ($self:ident, $n:expr, $limit:expr) => {
        if $n >= $limit {
            log::warn!(
                "connection [{}] header line limit reached",
                $self
                    .remote_addr
                    .as_ref()
                    .ok()
                    .map_or(String::default(), |a| {
                        a.map_or(String::default(), |a| a.to_string())
                    })
            );
            return Err(ReadError::HttpProtocol(
                HttpVersion::Version1_0,
                RequestHeaderFieldsTooLarge431,
            ));
        }
    };
}

impl ClientConnection {
    /// Creates a new `ClientConnection` that takes ownership of the `TcpStream`.
    pub(crate) fn new(
        write_socket: RefinedTcpStream,
        mut read_socket: RefinedTcpStream,
        client_counter: ArcRegistrationU16,
        limits: &Arc<LimitsConfig>,
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
            limits: Arc::clone(limits),
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
    /// at the first byte of the new line.
    ///
    /// The overall header limit is 8K.
    /// The limit per header line is 2K.
    fn read_next_line(&mut self) -> Result<AsciiString, ReadError> {
        let mut buf = Vec::new();
        let mut bytes = [0u8; limits::HEADER_READER_BUF_SIZE];
        let mut limit = 0;
        let mut w = 0_usize;

        let reader = self.next_header_source.by_ref();

        loop {
            let byte_result = reader.read(&mut bytes[w..=w]);

            match byte_result {
                Ok(0) => break,
                Err(err) => {
                    return Err(ReadError::ReadIoError(err));
                }
                _ => {}
            }

            if bytes[w] == NL {
                if w > 0 && bytes[w - 1] == CR {
                    let n = w - 1;
                    limit += n;
                    check_line_limit!(self, limit, self.limits.header_line_len);
                    buf.extend_from_slice(&bytes[0..n]);
                } else {
                    limit += w;
                    check_line_limit!(self, limit, self.limits.header_line_len);
                    buf.extend_from_slice(&bytes[0..w]);
                    log::debug!("missing 2-byte compliant <CR><NL>");
                }
                break;
            }

            if w < HEADER_READER_BUF_MAX_IDX {
                w += 1;
            } else {
                limit += w + 1;
                check_line_limit!(self, limit, self.limits.header_line_len);
                buf.extend_from_slice(&bytes[0..=w]);
                w = 0;
            }
        }

        AsciiString::from_ascii(buf).map_err(|_| {
            log::debug!("header no ascii");
            ReadError::ReadIoError(IoError::new(IoErrorKind::InvalidInput, "header no ascii"))
        })
    }

    /// Reads a request from the stream.
    /// Blocks until the header has been read or `read_timeout` occurred.
    ///
    /// The overall header limit is 8K.
    /// The limit per header line is 2K.
    fn read_request(&mut self) -> Result<Request, ReadError> {
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

                if line.is_empty() {
                    return Err(ReadError::ReadIoError(IoError::new(
                        IoErrorKind::TimedOut,
                        "no header",
                    )));
                }

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

        log::debug!("source-next");

        // follow-up for next potential request
        let mut next_header_source = self.source.next().unwrap();

        // log::debug!(
        //     "next_header_source timeout: {:?} self.next_header_source timeout: {:?}",
        //     crate::stream_traits::ReadTimeout::read_timeout(&next_header_source),
        //     crate::stream_traits::ReadTimeout::read_timeout(&self.next_header_source)
        // );

        std::mem::swap(&mut self.next_header_source, &mut next_header_source);
        let source_data = next_header_source; // move to make clear for current swap

        log::debug!("source-swaped");
        // log::debug!(
        //     "source_data timeout: {:?} self.next_header_source timeout: {:?}",
        //     source_data.read_timeout(),
        //     self.next_header_source.read_timeout()
        // );
        // let _ = data_source.set_read_timeout(Some(std::time::Duration::from_secs(300)));
        // log::debug!(
        //     "source_data timeout: {:?} self.next_header_source timeout: {:?}",
        //     source_data.read_timeout(),
        //     self.next_header_source.read_timeout()
        // );
        // let _ = self
        //     .next_header_source
        //     .set_read_timeout(Some(std::time::Duration::from_secs(400)));
        // log::debug!(
        //     "source_data timeout: {:?} self.next_header_source timeout: {:?}",
        //     source_data.read_timeout(),
        //     self.next_header_source.read_timeout()
        // );

        // building the next request
        let request = Request::create(
            self.limits.content_buffer_size,
            headers,
            method,
            path.to_string(),
            self.secure,
            version,
            *self.remote_addr.as_ref().unwrap(),
            source_data,
            writer,
        )
        .map_err(|err| {
            log::warn!("request: {err}");
            ReadError::from((err, version))
        })?;

        // return the request
        Ok(request)
    }

    fn request_error_handler(&mut self, err: ReadError) -> ReadError {
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

            ReadError::ExpectationFailed(ver) => {
                // TODO: should be recoverable, but needs handling in case of body
                send_error_std_response(self, ExpectationFailed417, Some(ver), true);
            }

            ReadError::ReadIoError(ref inner_err) if inner_err.kind() == IoErrorKind::TimedOut => {
                // windows socket uses `TimedOut` on socket timeout
                send_error_std_response(self, RequestTimeout408, None, false);
                // converting to `WouldBlock` for consistency
                // closing the connection
                let _ = inner_err;
                log::debug!("timed out: {inner_err}");
                return ReadError::WouldBlock;
            }

            ReadError::ReadIoError(ref inner_err)
                if inner_err.kind() == IoErrorKind::WouldBlock =>
            {
                // unix socket uses `WouldBlock` on socket timeout
                send_error_std_response(self, RequestTimeout408, None, false);
                // closing the connection
                let _ = inner_err;
                log::debug!("would block: {inner_err}");
                return ReadError::WouldBlock;
            }

            ReadError::ReadIoError(ref inner_err) => {
                let _ = inner_err;
                log::debug!("close cause: {inner_err} kind: {}", inner_err.kind());
                // closing the connection
            }

            ReadError::WouldBlock => {}
        };

        err
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

        let rq_result = self
            .read_request()
            .map_err(|err| self.request_error_handler(err));

        // check if request available
        let rq = if let Ok(rq) = rq_result {
            rq
        } else {
            self.is_connection_close = true;
            // return with ReadError
            return Some(rq_result);
        };

        // updating the status of the connection
        let connection_header = rq.headers().iter().find_map(|h| {
            if h.field.equiv("Connection") {
                ConnectionHeader::try_from(&h.value).ok()
            } else {
                None
            }
        });

        let mut rq = rq;

        // handle Connection header - see also `[Request::respond]`
        if let Some(connection_headers) = connection_header {
            let connection_header = connection_headers.iter().next();
            match connection_header {
                Some(ConnectionValue::Close) => set_connection_close(self, &mut rq, true),
                Some(ConnectionValue::Upgrade) => {
                    if !connection_headers.contains(&ConnectionValue::KeepAlive) {
                        set_connection_close(self, &mut rq, false);
                    }
                }
                // HTTP/1.0 does a upgrade to 1.1 with keep-alive set
                Some(ConnectionValue::KeepAlive)
                    if rq.http_version() == HttpVersion::Version1_0 =>
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
        } else if rq.http_version() <= HttpVersion::Version1_0 {
            // <= HTTP/1.0 is always close
            set_connection_close(self, &mut rq, true);
        } else {
            #[cfg(feature = "socket2")]
            if !self.socket_config.tcp_keep_alive {
                set_connection_close(self, &mut rq, true);
            }
        }

        let rq = rq;

        // returning the request
        Some(Ok(rq))
    }
}

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

    let _ = response.raw_print_ref(writer, version, &[], do_not_send_body, None);
}

#[inline]
fn set_connection_close(c: &mut ClientConnection, rq: &mut Request, set_close_header: bool) {
    c.is_connection_close = true;
    if set_close_header {
        rq.set_connection_header(Some(ConnectionValue::Close));
    }
}

/// Error that can happen when reading a request.
#[derive(Debug)]
pub(crate) enum ReadError {
    /// the client sent an unrecognized `Expect` header
    ExpectationFailed(HttpVersion),
    HttpProtocol(HttpVersion, response::Standard),
    ReadIoError(IoError),
    WouldBlock,
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
                    response.as_utf8_str().unwrap() // StandardResponse has data set
                )
            }
            Self::ReadIoError(err) => err.fmt(f),
            Self::WouldBlock => f.write_str("would block"),
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
