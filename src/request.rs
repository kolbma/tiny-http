use std::convert::TryFrom;
use std::io::Error as IoError;
use std::io::{self, Cursor, ErrorKind as IoErrorKind, Read, Write};
use std::net::SocketAddr;
use std::sync::mpsc::Sender;

use crate::common::{ConnectionHeader, ConnectionValue, Header, HeaderData, HttpVersion, Method};
use crate::response::Standard::Continue100;
use crate::stream_traits::{DataRead, DataReadWrite};
use crate::util::{EqualReader, FusedReader, NotifyOnDrop};
use crate::{log, response, Response};

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
    connection_header: Option<ConnectionHeader>,
    #[cfg(feature = "content-type")]
    content_type: Option<crate::ContentType>,
    content_length: Option<usize>,
    // where to read the body from
    data_reader: Option<Box<dyn DataRead + Send + 'static>>,
    // true if a `100 Continue` response must be sent when `as_reader()` is called
    expect_continue: bool,
    headers: HeaderData,
    http_version: HttpVersion,
    method: Method,
    // If Some, a message must be sent after responding
    notify_when_responded: Option<Sender<()>>,
    path: String,
    remote_addr: Option<SocketAddr>,
    // if this writer is empty, then the request has been answered
    response_writer: Option<Box<dyn Write + Send + 'static>>,
    // true if HTTPS, false if HTTP
    secure: bool,
}

impl Request {
    /// Allows to read the body of the request.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # extern crate tiny_http;
    /// # use std::io::Read;
    /// # fn get_content_type(_: &tiny_http::Request) -> &'static str { "" }
    /// # fn main() {
    /// # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    /// let mut request = server.recv().unwrap();
    ///
    /// if get_content_type(&request) == "application/json" {
    ///     let mut content = String::new();
    ///     request.as_reader().read_to_string(&mut content).unwrap();
    ///     // let json: Json = content.parse().unwrap();
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
    pub fn as_reader(&mut self) -> &mut dyn DataRead {
        if self.expect_continue {
            let response = <&response::StandardResponse>::from(Continue100);
            log::info!(
                "response [{}] ({})",
                self.remote_addr_string(),
                response.status_code()
            );
            let writer = self.response_writer.as_mut().unwrap();
            let _ = response.raw_print_ref(
                writer.by_ref(),
                self.http_version,
                Some(&self.headers),
                true,
                None,
            );
            let _ = writer.flush();
            self.expect_continue = false;
        }

        self.data_reader.as_mut().unwrap()
    }

    /// Returns the length of the body in bytes.
    ///
    /// Returns `None` if the length is unknown.
    #[deprecated = "use content_length()"]
    #[must_use]
    #[inline]
    pub fn body_length(&self) -> Option<usize> {
        self.content_length
    }

    /// The `[ConnectionHeader]` of `[Request]`
    #[must_use]
    pub fn connection_header(&self) -> Option<&ConnectionHeader> {
        self.connection_header.as_ref()
    }

    /// Returns the length of the body content in bytes.
    ///
    /// Returns `None` if the length is unknown.
    #[must_use]
    #[inline]
    pub fn content_length(&self) -> Option<usize> {
        self.content_length
    }

    /// One of the supported `[ContentType](crate::ContentType)` of `[Request]`
    #[cfg(feature = "content-type")]
    #[must_use]
    pub fn content_type(&self) -> Option<crate::ContentType> {
        self.content_type
    }

    /// Get up to `limit` headers provided with `field`
    ///
    /// A [`Request`] can be made with multiple lines of the same header field.  
    /// This is equivalent to providing a comma separated list in one
    /// header field.
    ///
    /// Up to `limit` lines with `field` are returned. It can be less if the header
    /// has lesser.
    ///
    /// If there is no such header `field` available in `Request` `None` is returned.
    ///
    #[inline]
    pub fn header<B>(&self, field: &B, limit: Option<usize>) -> Option<Vec<&Header>>
    where
        B: AsRef<[u8]> + Into<Vec<u8>>,
    {
        self.headers.header(field, limit)
    }

    /// Get the first header provided with `field`
    ///
    /// A [`Request`] can be made with multiple lines of the same header field.  
    /// This is equivalent to providing a comma separated list in one
    /// header field.
    ///
    /// If there is no such header `field` available in `Request` `None` is returned.
    ///
    #[inline]
    pub fn header_first<B>(&self, field: &B) -> Option<&Header>
    where
        B: AsRef<[u8]> + Into<Vec<u8>>,
    {
        self.headers.header_first(field)
    }

    /// Get the last header provided with `field`
    ///
    /// See also [`Request::header_first`].
    ///
    /// A [`Request`] can be made with multiple lines of the same header field.  
    /// This is equivalent to providing a comma separated list in one
    /// header field.
    ///
    /// If there is no such header `field` available in `Request` `None` is returned.
    ///
    #[inline]
    pub fn header_last<B>(&self, field: &B) -> Option<&Header>
    where
        B: AsRef<[u8]> + Into<Vec<u8>>,
    {
        self.headers.header_last(field)
    }

    /// Returns a list of all headers sent by the client.
    #[must_use]
    #[inline]
    pub fn headers(&self) -> &[Header] {
        self.headers.headers()
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
        let mut response = response;
        self.respond_update_headers(&mut response);

        let res = self.respond_impl(response);
        if let Some(sender) = self.notify_when_responded.take() {
            if let Err(err) = sender.send(()) {
                log::error!("send failed: {err:?}");
                let _ = err;
            }
        }
        res
    }

    /// Sends a response to this request.
    ///
    /// # Errors
    ///
    /// - `std::io::Error` on response problem
    ///
    #[inline]
    pub fn respond_ref<R>(mut self, response: &mut Response<R>) -> Result<(), IoError>
    where
        R: Read + Clone,
    {
        self.respond_update_headers(response);

        let res = self.respond_ref_impl(response);
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
    /// call this function to obtain full control of the socket stream.
    ///
    /// If you call this on a non-websocket request, tiny-http will wait until this `Stream` object
    /// is destroyed before continuing to read or write on the socket. Therefore you should always
    /// destroy it as soon as possible.
    ///
    /// # Panics
    ///
    /// - when response can not be written
    ///
    pub fn upgrade<R>(
        mut self,
        protocol: &str,
        response: Response<R>,
    ) -> Box<dyn DataReadWrite + Send>
    where
        R: Read,
    {
        use crate::util::CustomStream;

        if let Err(err) = response.raw_print(
            self.response_writer.as_mut().unwrap(),
            self.http_version,
            Some(&self.headers),
            false,
            Some(protocol),
        ) {
            log::error!("upgrade fail: {err:?}");
            let _ = err;
        }

        let _ = self.response_writer.as_mut().unwrap().flush();

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

    /// Builds a new request.
    ///
    /// After the request line and headers have been read from the socket, a new `Request` object
    /// is built.
    ///
    /// You must pass a `Read` that will allow the `Request` object to read from the incoming data.
    /// It is the responsibility of the `Request` to read only the data of the request and not further.
    ///
    /// The `Write` object will be used by the `Request` to write the response.
    ///
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create<R, W>(
        buf_size: usize,
        headers: Vec<Header>,
        method: Method,
        path: String,
        secure: bool,
        version: HttpVersion,
        remote_addr: Option<SocketAddr>,
        mut source_data: R,
        writer: W,
    ) -> Result<Request, CreateError>
    where
        R: DataRead + Send + 'static,
        W: Write + Send + 'static,
    {
        let headers = HeaderData::new(headers);

        headers.cache_header(&[
            &b"Connection"[..],
            b"Content-Length",
            #[cfg(feature = "content-type")]
            b"Content-Type",
            b"Expect",
            b"Transfer-Encoding",
        ]);

        let is_transfer_encoding = headers.header_first(b"Transfer-Encoding").is_some();
        let connection_header = headers
            .header_first(b"Connection")
            .and_then(|h| ConnectionHeader::try_from(&h.value).ok());

        let is_upgrade = connection_header
            .as_ref()
            .map_or(false, |h| *h == ConnectionValue::Upgrade);
        // let mut search_header = SearchHeader::parse(&headers)?;

        let content_length = if is_upgrade || is_transfer_encoding {
            // if transfer-encoding is specified, the Content-Length
            // header must be ignored (RFC2616 #4.4)
            None
        } else if let Some(h) = headers.header_first(b"Content-Length") {
            h.value.as_str().parse::<usize>().ok()
        } else {
            None
        };

        let expect_continue = if let Some(h) = headers.header_first(b"Expect") {
            if h.value == "100-continue" {
                true
            } else {
                return Err(CreateError::Expect);
            }
        } else {
            false
        };

        // we wrap `source_data` around a reading whose nature depends on the transfer-encoding and
        // content-length headers
        let reader = if is_upgrade {
            // if we have a `Connection: upgrade`, always keeping the whole reader
            Box::new(source_data)
        } else if let Some(content_length) = content_length {
            if content_length == 0 {
                Box::new(io::empty())
            } else if content_length <= buf_size && !expect_continue {
                // if the content-length is small enough (`buf_size`), we just read everything into a buffer
                // see [`LimitsConfig`](crate::LimitsConfig)

                let is_connection_close = connection_header
                    .as_ref()
                    .map_or(false, |h| *h == ConnectionValue::Close);

                // next request in keep-alive connection, follows possible immediately after content-length
                let mut buffer = vec![0; content_length + usize::from(is_connection_close)];
                let mut offset = 0;

                while offset < content_length {
                    let read = source_data.read(&mut buffer[offset..])?;
                    if read == 0 {
                        break;
                    }

                    offset += read;
                }

                if offset < content_length || (offset != content_length && is_connection_close) {
                    // on Connection: close it needs to match, because there may be no next request in stream
                    return Err(CreateError::ContentLength);
                }
                // keep-alive: if next data in stream is no new request but oversize data from this request
                // it will be handled by bad request in the next the request handling

                Box::new(Cursor::new(buffer))
            } else {
                let data_reader = EqualReader::new(source_data, content_length, None); // TODO:
                #[allow(trivial_casts)]
                {
                    Box::new(FusedReader::new(data_reader)) as Box<dyn DataRead + Send + 'static>
                }
            }
        } else if is_transfer_encoding {
            // if a transfer-encoding was specified, then "chunked" is ALWAYS applied over the message (RFC2616 #3.6)
            #[allow(trivial_casts)]
            {
                Box::new(FusedReader::new(chunked_transfer::Decoder::new(
                    source_data,
                ))) as Box<dyn DataRead + Send + 'static>
            }
        } else {
            // if we have neither a Content-Length nor a Transfer-Encoding, assuming that we have no data
            // TODO: could also be multipart/byteranges
            Box::new(io::empty())
        };

        Ok(Request {
            connection_header,
            content_length,
            #[cfg(feature = "content-type")]
            content_type: headers
                .header_first(b"Content-Type")
                .and_then(|h| crate::ContentType::try_from(h).ok()),
            data_reader: Some(reader),
            expect_continue,
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

    /// Set `[ConnectionHeader]` of `[Request]`
    pub(crate) fn set_connection_header(&mut self, connection_header: Option<ConnectionValue>) {
        self.connection_header = connection_header.map(ConnectionHeader::from);
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
        debug_assert!(self.response_writer.is_some());
        self.response_writer.take().expect("extract writer failed")
    }

    /// Extract the body `Reader` object from the Request.
    ///
    /// This may only be called once on a single request.
    fn extract_reader_impl(&mut self) -> Box<dyn DataRead + Send + 'static> {
        debug_assert!(self.data_reader.is_some());
        self.data_reader.take().expect("extract reader failed")
    }

    fn ignore_client_closing_errors(result: io::Result<()>) -> io::Result<()> {
        result.or_else(|err| match err.kind() {
            IoErrorKind::BrokenPipe
            | IoErrorKind::ConnectionAborted
            | IoErrorKind::ConnectionRefused
            | IoErrorKind::ConnectionReset
            | IoErrorKind::TimedOut
            | IoErrorKind::WouldBlock => {
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
            Some(&self.headers),
            do_not_send_body,
            None,
        ))?;

        Self::ignore_client_closing_errors(writer.flush())
    }

    fn respond_ref_impl<R>(&mut self, response: &Response<R>) -> Result<(), IoError>
    where
        R: Read + Clone,
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

        Self::ignore_client_closing_errors(response.raw_print_ref(
            writer.by_ref(),
            self.http_version,
            Some(&self.headers),
            do_not_send_body,
            None,
        ))?;

        Self::ignore_client_closing_errors(writer.flush())
    }

    fn respond_update_headers<R>(&mut self, response: &mut Response<R>)
    where
        R: Read,
    {
        // Modify `Connection` header to `Close` connection
        if self.connection_header == Some(ConnectionValue::Close.into()) {
            response::util::update_optional_header(
                response.headers_mut(),
                ConnectionValue::Close.into(),
                false,
            );
        } else if self.connection_header == Some(ConnectionValue::KeepAlive.into()) {
            response::util::update_optional_header(
                response.headers_mut(),
                ConnectionValue::KeepAlive.into(),
                false,
            );
            if self.http_version == HttpVersion::Version1_0 {
                // keep-alive upgrades to 1.1
                self.http_version = HttpVersion::Version1_1;
            }
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
            let response =
                <&response::StandardResponse>::from(&response::Standard::InternalServerError500);
            log::debug!(
                "drop unresponded request [{}] ({})",
                self.remote_addr_string(),
                response.status_code()
            );
            let _ = self.respond_ref_impl(response); // ignoring any potential error
            if let Some(sender) = self.notify_when_responded.take() {
                if let Err(err) = sender.send(()) {
                    log::error!("notify_when_responded fail");
                    let _ = err;
                }
            }
        }
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
