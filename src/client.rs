use std::convert::TryFrom;
use std::io::{BufReader, BufWriter, Read};
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};
use std::net::SocketAddr;
use std::sync::Arc;

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
    fn read_next_line(&mut self, buf: &mut Vec<u8>) -> Result<(), ReadError> {
        let mut bytes = [0u8; limits::HEADER_READER_BUF_SIZE];
        let mut limit = 0;
        let mut space_only = true;
        let mut w = 0_usize;

        buf.clear();
        let reader = self.next_header_source.by_ref();

        loop {
            let byte_result = reader.read(&mut bytes[w..=w]);

            match byte_result {
                Ok(0) => {
                    if space_only && w == 0 {
                        space_only = false;
                    }
                    break;
                }
                Err(err) => {
                    return Err(ReadError::ReadIoError(err));
                }
                _ => {}
            }

            let b = bytes[w];

            #[allow(clippy::manual_range_contains)]
            if b == NL {
                if w > 0 && bytes[w - 1] == CR {
                    let n = w - 1;
                    limit += n;
                    check_line_limit!(self, limit, self.limits.header_line_len);
                    buf.extend_from_slice(&bytes[0..n]);
                } else {
                    if w == 0 {
                        // got <NL> in a fresh bytes buffer
                        space_only = false;
                    }
                    limit += w;
                    check_line_limit!(self, limit, self.limits.header_line_len);
                    buf.extend_from_slice(&bytes[0..w]);
                    log::debug!("missing 2-byte compliant <CR><NL>");
                }

                break;
            } else if (b != CR && b < 32 && b != 9) || b == 127 {
                // abort early when byte range of client violates spec
                return Err(ReadError::RfcViolation);
            } else if space_only && b != 32 && b != 9 {
                space_only = false;
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

        if space_only {
            // spec doesn't allow lines with only spaces
            return Err(ReadError::RfcViolation);
        }

        Ok(())
    }

    /// Reads a request from the stream.
    /// Blocks until the header has been read or `read_timeout` occurred.
    ///
    /// The overall header limit is 8K.
    /// The limit per header line is 2K.
    fn read_request(&mut self) -> Result<Request, ReadError> {
        let (method, path, version, headers) = {
            let mut header_limit_rest = self.limits.header_max_size;
            let mut line_buf = Vec::new();

            // reading the request line
            let (method, path, version) = {
                self.read_next_line(&mut line_buf).map_err(|err| {
                    match err {
                        ReadError::HttpProtocol(v, RequestHeaderFieldsTooLarge431) => {
                            // match to 414 URI Too Long for request line
                            ReadError::HttpProtocol(v, UriTooLong414)
                        }
                        _ => err,
                    }
                })?;

                let line_len = line_buf.len();

                if line_len == 0 {
                    return Err(ReadError::ReadIoError(IoError::new(
                        IoErrorKind::TimedOut,
                        "no header",
                    )));
                }

                header_limit_rest = header_limit_rest.checked_sub(line_len).ok_or(
                    // Request Header Fields Too Large
                    ReadError::HttpProtocol(
                        HttpVersion::Version1_0,
                        RequestHeaderFieldsTooLarge431,
                    ),
                )?;

                parse_request_line(&line_buf)?
            };

            let path = std::str::from_utf8(path).unwrap().to_owned();

            // getting all headers
            let headers = {
                let mut headers = Vec::new();
                loop {
                    self.read_next_line(&mut line_buf)?;

                    let line_len = line_buf.len();

                    if line_len == 0 {
                        break; // header end
                    }

                    header_limit_rest = header_limit_rest.checked_sub(line_len).ok_or(
                        // Request Header Fields Too Large
                        ReadError::HttpProtocol(version, RequestHeaderFieldsTooLarge431),
                    )?;

                    headers.push(match Header::try_from(line_buf.as_slice()) {
                        Ok(h) => h,
                        _ => return Err(ReadError::Header(version)),
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
        let mut next_header_source = self.source.next().unwrap();

        std::mem::swap(&mut self.next_header_source, &mut next_header_source);
        let source_data = next_header_source; // move to make clear for current swap

        // building the next request
        let request = Request::create(
            self.limits.content_buffer_size,
            headers,
            method,
            path,
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

            ReadError::RequestLine | ReadError::RfcViolation => {
                send_error_std_response(self, BadRequest400, None, false);
                // we don't know where the next request would start,
                // so we have to close
            }

            ReadError::Header(ver) => {
                send_error_std_response(self, BadRequest400, Some(ver), false);
                // we don't know where the next request would start,
                // so we have to close
            }

            ReadError::HttpProtocol(ver, status) => {
                send_error_std_response(self, status, Some(ver), false);
                // we don't know where the next request would start,
                // so we have to close
            }

            ReadError::HttpVersion(_ver) => {
                send_error_std_response(self, HttpVersionNotSupported505, None, false);
            }

            ReadError::ExpectationFailed(ver) => {
                // TODO: should be recoverable, but needs handling in case of body
                send_error_std_response(self, ExpectationFailed417, Some(ver), true);
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

        let rq = self
            .read_request()
            .map_err(|err| {
                self.is_connection_close = true;
                self.request_error_handler(err)
            })
            .ok()?;

        // updating the status of the connection
        let connection_header = rq
            .header_first(b"Connection")
            .and_then(|h| ConnectionHeader::try_from(&h.value).ok());

        let mut rq = rq;

        // handle Connection header - see also [`request::Request::respond`]
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

    let _ = response.raw_print_ref(writer, version, None, do_not_send_body, None);
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
    Header(HttpVersion),
    HttpProtocol(HttpVersion, response::Standard),
    HttpVersion(Option<HttpVersion>),
    ReadIoError(IoError),
    RequestLine,
    RfcViolation,
    WouldBlock,
}

impl std::error::Error for ReadError {}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExpectationFailed(v) => {
                f.write_str("unrecognized expect: ")?;
                f.write_str(v.header())
            }
            Self::Header(v) => {
                f.write_str("unsupported header: ")?;
                f.write_str(v.header())
            }
            Self::HttpProtocol(v, status) => {
                let response = <&response::StandardResponse>::from(status);
                f.write_str(v.header())?;
                f.write_str(&response.status_code().to_string())?;
                f.write_str(response.as_utf8_str().unwrap()) // StandardResponse has valid utf-8 data set
            }
            Self::HttpVersion(v) => {
                f.write_str("unsupported version: ")?;
                f.write_str(v.map(|v| v.header()).unwrap_or_default())
            }
            Self::ReadIoError(err) => err.fmt(f),
            Self::RequestLine => f.write_str("no request"),
            Self::RfcViolation => f.write_str("http rfc violation"),
            Self::WouldBlock => f.write_str("would block"),
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
#[inline]
fn parse_request_line(line: &[u8]) -> Result<(Method, &[u8], HttpVersion), ReadError> {
    let mut method_pos = (0, 0);
    let mut path_pos = (0, 0);
    let mut version_pos = (0, 0);

    let mut is_next = false;
    let mut pos = 0;

    #[allow(clippy::explicit_counter_loop, clippy::manual_range_contains)]
    // it's faster than iterator
    for &b in line {
        if b == 32 {
            if is_next {
                // more spaces than allowed
                return Err(ReadError::RequestLine);
            }
            is_next = true;
            if method_pos.1 == 0 {
                method_pos.1 = pos;
            } else if path_pos.1 == 0 {
                path_pos.1 = pos;
            } else if version_pos.1 == 0 {
                // should be at the end of line
                return Err(ReadError::RequestLine);
            }
        } else if !((b >= 63 && b <= 126) || (b >= 36 && b <= 59) || b == 61 || b == 33) {
            return Err(ReadError::RequestLine);
        } else if is_next {
            is_next = false;
            if path_pos.0 == 0 {
                path_pos.0 = pos;
            } else if version_pos.0 == 0 {
                version_pos.0 = pos;
                break; // start of last part
            }
        }

        pos += 1;
    }

    if method_pos.1 == 0 || path_pos.1 == 0 {
        return Err(ReadError::RequestLine);
    }

    let method = Method::from(&line[method_pos.0..method_pos.1]);
    let path = &line[path_pos.0..path_pos.1];
    let version = if let Ok(ver) = HttpVersion::try_from(&line[version_pos.0..]) {
        if ver <= HttpVersion::Version1_1 {
            // only up to 1.1 supported
            ver
        } else {
            return Err(ReadError::HttpVersion(Some(ver)));
        }
    } else {
        return Err(ReadError::HttpVersion(None));
    };

    Ok((method, path, version))
}

#[cfg(test)]
mod test {
    use crate::HttpVersion;

    #[test]
    fn parse_request_line_test() {
        let (method, path, ver) = super::parse_request_line(&b"GET /hello HTTP/1.1"[..]).unwrap();

        assert!(method == crate::Method::Get);
        assert!(path == b"/hello");
        assert!(ver == HttpVersion::Version1_1);

        assert!(super::parse_request_line(&b"GET /hello"[..]).is_err());
        assert!(super::parse_request_line(&b"qsd qsd qsd"[..]).is_err());
        assert!(super::parse_request_line(&b" GET /hello HTTP/1.1"[..]).is_err());
        assert!(super::parse_request_line(&b"GET /hello HTTP/1.1 "[..]).is_err());
        assert!(super::parse_request_line(&b"GET   /hello HTTP/1.1"[..]).is_err());
        assert!(super::parse_request_line(&b"GET /hello   HTTP/1.1"[..]).is_err());

        assert!(super::parse_request_line(&b"GET /favicon.ico HTTP/1.1"[..]).is_ok());
        assert!(super::parse_request_line(&b"GET /hello?q=1 HTTP/1.1"[..]).is_ok());
        assert!(super::parse_request_line(&b"GET /hello?q=1#local HTTP/1.1"[..]).is_err());
        assert!(
            super::parse_request_line(&b"GET https://localhost:8080/index.html HTTP/1.1"[..])
                .is_ok()
        );
        assert!(super::parse_request_line(&b"OPTIONS * HTTP/1.1"[..]).is_ok());

        let (method, _, _) = super::parse_request_line(&b"loGET /hello HTTP/1.1"[..]).unwrap();
        assert_eq!(
            method,
            crate::Method::NonStandard(Some("loGET".parse().unwrap()))
        );
    }
}
