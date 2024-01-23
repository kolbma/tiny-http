use std::convert::TryFrom;
use std::io::Error as IoError;
use std::io::{self, Cursor, ErrorKind, Read, Write};
use std::net::SocketAddr;
use std::sync::mpsc::Sender;

use crate::util::{EqualReader, FusedReader};
use crate::{log, ConnectionHeader, StatusCode};
use crate::{Header, HttpVersion, Method, Response};
use ascii::AsciiString;
use chunked_transfer::Decoder;

/// Represents an HTTP request made by a client.
///
/// A `Request` object is what is produced by the server, and is your what
/// your code must analyse and answer.
///
/// This object implements the `Send` trait, therefore you can dispatch your requests to
/// worker threads.
///
/// # Pipelining
///
/// If a client sends multiple requests in a row (without waiting for the response), then you will
/// get multiple `Request` objects simultaneously. This is called *requests pipelining*.
/// Tiny-http automatically reorders the responses so that you don't need to worry about the order
/// in which you call `respond` or `into_writer`.
///
/// This mechanic is disabled if:
///
///  - The body of a request is large enough (handling requires pipelining requires storing the
///    body of the request in a buffer ; if the body is too big, tiny-http will avoid doing that)
///  - A request sends a `Expect: 100-continue` header (which means that the client waits to
///    know whether its body will be processed before sending it)
///  - A request sends a `Connection: close` header or `Connection: upgrade` header (used for
///    websockets), which indicates that this is the last request that will be received on this
///    connection
///
/// # Automatic cleanup
///
/// If a `Request` object is destroyed without `into_writer` or `respond` being called,
/// an empty response with a 500 status code (internal server error) will automatically be
/// sent back to the client.
/// This means that if your code fails during the handling of a request, this "internal server
/// error" response will automatically be sent during the stack unwinding.
///
/// # Testing
///
/// If you want to build fake requests to test your server, use [`TestRequest`](crate::test::TestRequest).
pub struct Request {
    body_length: Option<usize>,
    connection_header: Option<ConnectionHeader>,
    #[cfg(feature = "content-type")]
    content_type: Option<crate::ContentType>,
    // where to read the body from
    data_reader: Option<Box<dyn Read + Send + 'static>>,
    headers: Vec<Header>,
    http_version: HttpVersion,
    method: Method,
    // true if a `100 Continue` response must be sent when `as_reader()` is called
    must_send_continue: bool,
    // If Some, a message must be sent after responding
    notify_when_responded: Option<Sender<()>>,
    path: String,
    remote_addr: Option<SocketAddr>,
    // if this writer is empty, then the request has been answered
    response_writer: Option<Box<dyn Write + Send + 'static>>,
    // true if HTTPS, false if HTTP
    secure: bool,
}

struct NotifyOnDrop<R> {
    sender: Sender<()>,
    inner: R,
}

impl<R: Read> Read for NotifyOnDrop<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}
impl<R: Write> Write for NotifyOnDrop<R> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<R> Drop for NotifyOnDrop<R> {
    fn drop(&mut self) {
        self.sender.send(()).unwrap();
    }
}

/// Error that can happen when building a `Request` object
#[derive(Debug)]
pub(crate) enum CreateError {
    /// Content-Length not correct
    ContentLength,
    /// The client sent an `Expect` header that was not recognized by tiny-http
    Expect,
    /// Error while reading data from the socket during the creation of the `Request`
    IoError(IoError),
}

impl std::error::Error for CreateError {}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContentLength => f.write_str("content-length error"),
            Self::Expect => f.write_str("expect error"),
            Self::IoError(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl From<IoError> for CreateError {
    fn from(err: IoError) -> CreateError {
        CreateError::IoError(err)
    }
}

/// Builds a new request.
///
/// After the request line and headers have been read from the socket, a new `Request` object
/// is built.
///
/// You must pass a `Read` that will allow the `Request` object to read from the incoming data.
/// It is the responsibility of the `Request` to read only the data of the request and not further.
///
/// The `Write` object will be used by the `Request` to write the response.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn new_request<R, W>(
    secure: bool,
    method: Method,
    path: String,
    version: HttpVersion,
    headers: Vec<Header>,
    remote_addr: Option<SocketAddr>,
    mut source_data: R,
    writer: W,
) -> Result<Request, CreateError>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    struct SearchHeader<'a> {
        connection: Option<ConnectionHeader>,
        content_length: Option<usize>,
        #[cfg(feature = "content-type")]
        content_type: Option<crate::ContentType>,
        expect_continue: bool,
        transfer_encoding: Option<&'a AsciiString>,
    }

    // search headers for these fields and do bit marking in found_headers
    let mut search_header = SearchHeader {
        connection: None,
        content_length: None,
        #[cfg(feature = "content-type")]
        content_type: None,
        expect_continue: false,
        transfer_encoding: None,
    };

    let mut found_headers = 0u8;

    for header in &headers {
        let f = &header.field;
        if f.equiv("Connection") {
            search_header.connection = ConnectionHeader::try_from(header.value.as_str()).ok();
            found_headers |= 1;
        } else if f.equiv("Content-Length") {
            search_header.content_length = header.value.as_str().parse().ok();
            found_headers |= 2;
        } else if f.equiv("Expect") {
            // true if the client sent a `Expect: 100-continue` header
            if header.value.as_str().eq_ignore_ascii_case("100-continue") {
                search_header.expect_continue = true;
            } else {
                return Err(CreateError::Expect);
            }
            found_headers |= 4;
        } else if f.equiv("Transfer-Encoding") {
            search_header.transfer_encoding = Some(&header.value);
            found_headers |= 8;
        }

        #[cfg(feature = "content-type")]
        {
            if f.equiv("Content-Type") {
                search_header.content_type = crate::ContentType::try_from(header).ok();
                found_headers |= 16;
            }

            // bit field match
            if found_headers == 31 {
                break;
            }
        }

        // bit field match
        #[cfg(not(feature = "content-type"))]
        if found_headers == 15 {
            break;
        }
    }

    if search_header.transfer_encoding.is_some() {
        // if transfer-encoding is specified, the Content-Length
        // header must be ignored (RFC2616 #4.4)
        search_header.content_length = None;
    }

    // we wrap `source_data` around a reading whose nature depends on the transfer-encoding and
    // content-length headers
    let reader = if search_header.connection == Some(ConnectionHeader::Upgrade) {
        // if we have a `Connection: upgrade`, always keeping the whole reader
        Box::new(source_data)
    } else if let Some(content_length) = search_header.content_length {
        if content_length == 0 {
            Box::new(io::empty())
        } else if content_length <= 1024 && !search_header.expect_continue {
            // if the content-length is small enough (1024 byte), we just read everything into a buffer

            // next request in keep-alive connection, follows possible immediately after content-length
            let mut buffer =
                vec![
                    0;
                    content_length
                        + usize::from(search_header.connection == Some(ConnectionHeader::Close))
                ];
            let mut offset = 0;

            while offset < content_length {
                let read = source_data.read(&mut buffer[offset..])?;
                if read == 0 {
                    break;
                }

                offset += read;
            }

            if offset < content_length
                || (offset != content_length
                    && search_header.connection == Some(ConnectionHeader::Close))
            {
                // on Connection: close it needs to match, because there may be no next request in stream
                return Err(CreateError::ContentLength);
            }
            // keep-alive: if next data in stream is no new request but oversize data from this request
            // it will become bad request when handling the request header

            Box::new(Cursor::new(buffer))
        } else {
            let data_reader = EqualReader::new(source_data, content_length, None); // TODO:
            #[allow(trivial_casts)]
            {
                Box::new(FusedReader::new(data_reader)) as Box<dyn Read + Send + 'static>
            }
        }
    } else if search_header.transfer_encoding.is_some() {
        // if a transfer-encoding was specified, then "chunked" is ALWAYS applied over the message (RFC2616 #3.6)
        #[allow(trivial_casts)]
        {
            Box::new(FusedReader::new(Decoder::new(source_data))) as Box<dyn Read + Send + 'static>
        }
    } else {
        // if we have neither a Content-Length nor a Transfer-Encoding, assuming that we have no data
        // TODO: could also be multipart/byteranges
        Box::new(io::empty())
    };

    Ok(Request {
        body_length: search_header.content_length,
        must_send_continue: search_header.expect_continue,
        connection_header: search_header.connection,
        #[cfg(feature = "content-type")]
        content_type: search_header.content_type,
        data_reader: Some(reader),
        headers,
        http_version: version,
        method,
        notify_when_responded: None,
        path,
        remote_addr,
        response_writer: Some(Box::new(writer)),
        secure,
    })
}

impl Request {
    /// Allows to read the body of the request.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # extern crate rustc_serialize;
    /// # extern crate tiny_http;
    /// # use rustc_serialize::json::Json;
    /// # use std::io::Read;
    /// # fn get_content_type(_: &tiny_http::Request) -> &'static str { "" }
    /// # fn main() {
    /// # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    /// let mut request = server.recv().unwrap();
    ///
    /// if get_content_type(&request) == "application/json" {
    ///     let mut content = String::new();
    ///     request.as_reader().read_to_string(&mut content).unwrap();
    ///     let json: Json = content.parse().unwrap();
    /// }
    /// # }
    /// ```
    ///
    /// If the client sent a `Expect: 100-continue` header with the request, calling this
    ///  function will send back a `100 Continue` response.
    ///
    /// # Panics
    ///
    /// - when response can not be written
    ///
    #[inline]
    pub fn as_reader(&mut self) -> &mut dyn Read {
        if self.must_send_continue {
            let msg = Response::empty(100);
            let _ = msg.raw_print(
                self.response_writer.as_mut().unwrap().by_ref(),
                self.http_version,
                &self.headers,
                true,
                None,
            );
            let _ = self.response_writer.as_mut().unwrap().flush();
            self.must_send_continue = false;
        }

        self.data_reader.as_mut().unwrap()
    }

    /// Returns the length of the body in bytes.
    ///
    /// Returns `None` if the length is unknown.
    #[must_use]
    #[inline]
    pub fn body_length(&self) -> Option<usize> {
        self.body_length
    }

    /// The `[ConnectionHeader]` of `[Request]`
    #[must_use]
    pub fn connection_header(&self) -> Option<ConnectionHeader> {
        self.connection_header
    }

    /// One of the supported `[ContentType](crate::ContentType)` of `[Request]`
    #[cfg(feature = "content-type")]
    #[must_use]
    pub fn content_type(&self) -> Option<crate::ContentType> {
        self.content_type
    }

    /// Returns a list of all headers sent by the client.
    #[must_use]
    #[inline]
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// Returns the HTTP version of the request.
    #[must_use]
    #[inline]
    pub fn http_version(&self) -> HttpVersion {
        self.http_version
    }

    /// Turns the `Request` into a writer.
    ///
    /// The writer has a raw access to the stream to the user.
    /// This function is useful for things like CGI.
    ///
    /// Note that the destruction of the `Writer` object may trigger
    /// some events. For exemple if a client has sent multiple requests and the requests
    /// have been processed in parallel, the destruction of a writer will trigger
    /// the writing of the next response.
    /// Therefore you should always destroy the `Writer` as soon as possible.
    #[must_use]
    #[inline]
    pub fn into_writer(mut self) -> Box<dyn Write + Send + 'static> {
        let writer = self.extract_writer_impl();
        if let Some(sender) = self.notify_when_responded.take() {
            let writer = NotifyOnDrop {
                sender,
                inner: writer,
            };
            Box::new(writer)
        } else {
            writer
        }
    }

    /// Returns the method requested by the client (eg. `GET`, `POST`, etc.).
    #[must_use]
    #[inline]
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the address of the client that sent this request.
    ///
    /// The address is always `Some` for TCP listeners, but always `None` for UNIX listeners
    /// (as the remote address of a UNIX client is almost always unnamed).
    ///
    /// Note that this is gathered from the socket. If you receive the request from a proxy,
    /// this function will return the address of the proxy and not the address of the actual
    /// user.
    #[must_use]
    #[inline]
    pub fn remote_addr(&self) -> Option<&SocketAddr> {
        self.remote_addr.as_ref()
    }

    /// Returns the address of the client that sent this request as `[String]`.
    ///
    /// The address is always `Some` for TCP listeners, but always `None` for UNIX listeners
    /// (as the remote address of a UNIX client is almost always unnamed).
    /// No available address is returned as empty string.
    ///
    /// Note that this is gathered from the socket. If you receive the request from a proxy,
    /// this function will return the address of the proxy and not the address of the actual
    /// user.
    #[must_use]
    #[inline]
    pub fn remote_addr_string(&self) -> String {
        self.remote_addr
            .as_ref()
            .map_or(String::default(), std::string::ToString::to_string)
    }

    /// Sends a response to this request.
    ///
    /// # Errors
    ///
    /// - `std::io::Error` on response problem
    ///
    #[inline]
    pub fn respond<R>(mut self, response: Response<R>) -> Result<(), IoError>
    where
        R: Read,
    {
        // Modify `Connection` header to `Close` connection
        let response = if let Some(ConnectionHeader::Close) = self.connection_header {
            let mut response = response;
            Self::update_header(&mut response, ConnectionHeader::Close.into());
            response
        } else if self.http_version == HttpVersion::Version1_1 {
            let mut response = response;
            Self::update_header(&mut response, ConnectionHeader::KeepAlive.into());
            response
        } else if let Some(ConnectionHeader::KeepAlive) = self.connection_header {
            let mut response = response;
            Self::update_header(&mut response, ConnectionHeader::KeepAlive.into());
            response
        } else {
            response
        };

        let res = self.respond_impl(response);
        if let Some(sender) = self.notify_when_responded.take() {
            if let Err(err) = sender.send(()) {
                log::error!("send failed: {err:?}");
                let _ = err;
            }
        }
        res
    }

    /// Returns true if the request was made through HTTPS.
    #[must_use]
    #[inline]
    pub fn secure(&self) -> bool {
        self.secure
    }

    /// Sends a response with a `Connection: upgrade` header, then turns the `Request` into a `Stream`.
    ///
    /// The main purpose of this function is to support websockets.
    /// If you detect that the request wants to use some kind of protocol upgrade, you can
    ///  call this function to obtain full control of the socket stream.
    ///
    /// If you call this on a non-websocket request, tiny-http will wait until this `Stream` object
    ///  is destroyed before continuing to read or write on the socket. Therefore you should always
    ///  destroy it as soon as possible.
    ///
    /// # Panics
    ///
    /// - when response can not be written
    ///
    pub fn upgrade<R: Read>(
        mut self,
        protocol: &str,
        response: Response<R>,
    ) -> Box<dyn ReadWrite + Send> {
        use crate::util::CustomStream;

        let _ = response.raw_print(
            self.response_writer.as_mut().unwrap(),
            self.http_version,
            &self.headers,
            false,
            Some(protocol),
        ); // TODO: unused result

        let _ = self.response_writer.as_mut().unwrap().flush(); // TODO: unused result

        let stream = CustomStream::new(self.extract_reader_impl(), self.extract_writer_impl());
        if let Some(sender) = self.notify_when_responded.take() {
            let stream = NotifyOnDrop {
                sender,
                inner: stream,
            };
            Box::new(stream)
        } else {
            Box::new(stream)
        }
    }

    /// Returns the resource requested by the client.
    #[must_use]
    #[inline]
    pub fn url(&self) -> &str {
        &self.path
    }

    /// Set `[ConnectionHeader]` of `[Request]`
    pub(crate) fn set_connection_header(&mut self, connection_header: Option<ConnectionHeader>) {
        self.connection_header = connection_header;
    }

    pub(crate) fn with_notify_sender(mut self, sender: Sender<()>) -> Self {
        self.notify_when_responded = Some(sender);
        self
    }

    /// Extract the response `Writer` object from the Request, dropping this `Writer` has the same side effects
    /// as the object returned by `into_writer` above.
    ///
    /// This may only be called once on a single request.
    fn extract_writer_impl(&mut self) -> Box<dyn Write + Send + 'static> {
        use std::mem;

        assert!(self.response_writer.is_some());

        let mut writer = None;
        mem::swap(&mut self.response_writer, &mut writer);
        writer.unwrap()
    }

    /// Extract the body `Reader` object from the Request.
    ///
    /// This may only be called once on a single request.
    fn extract_reader_impl(&mut self) -> Box<dyn Read + Send + 'static> {
        use std::mem;

        assert!(self.data_reader.is_some());

        let mut reader = None;
        mem::swap(&mut self.data_reader, &mut reader);
        reader.unwrap()
    }

    fn ignore_client_closing_errors(result: io::Result<()>) -> io::Result<()> {
        result.or_else(|err| match err.kind() {
            ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset => {
                log::info!("{err:?}");
                Ok(())
            }
            _ => {
                log::error!("error: {err:?}");
                Err(err)
            }
        })
    }

    fn respond_impl<R>(&mut self, response: Response<R>) -> Result<(), IoError>
    where
        R: Read,
    {
        if response.status_code() < 400 {
            log::info!(
                "response [{}] ({})",
                self.remote_addr_string(),
                response.status_code()
            );
        }

        let mut writer = self.extract_writer_impl();

        let do_not_send_body = self.method == Method::Head;

        Self::ignore_client_closing_errors(response.raw_print(
            writer.by_ref(),
            self.http_version,
            &self.headers,
            do_not_send_body,
            None,
        ))?;

        Self::ignore_client_closing_errors(writer.flush())
    }

    fn update_header<R>(response: &mut Response<R>, header: Header)
    where
        R: Read,
    {
        let headers = response.headers_mut();
        if let Some(mut_header) = headers.iter_mut().find(|h| h.field == header.field) {
            mut_header.value = header.value;
        } else {
            headers.push(header);
        }
    }
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "Request({} {} from {:?})",
            self.method, self.path, self.remote_addr
        )
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        if self.response_writer.is_some() {
            let status = StatusCode(500);
            let msg = status.default_reason_phrase();
            let response = Response::from_string(msg).with_status_code(status);
            log::debug!(
                "drop unresponded request [{}] ({status})",
                self.remote_addr_string()
            );
            let _ = self.respond_impl(response); // ignoring any potential error
            if let Some(sender) = self.notify_when_responded.take() {
                if let Err(err) = sender.send(()) {
                    log::error!("notify_when_responded fail");
                    let _ = err;
                }
            }
        }
    }
}

/// Dummy trait that regroups the `Read` and `Write` traits.
///
/// Automatically implemented on all types that implement both `Read` and `Write`.
pub trait ReadWrite: Read + Write {}
impl<T> ReadWrite for T where T: Read + Write {}

#[cfg(test)]
mod tests {
    use super::Request;

    #[test]
    fn must_be_send() {
        #![allow(dead_code)]
        fn f<T: Send>(_: &T) {}
        fn bar(rq: &Request) {
            f(rq);
        }
    }
}
