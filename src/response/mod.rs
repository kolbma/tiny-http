//! `response` module
//!
//! See [`Response`]

use std::collections::HashSet;
use std::convert::TryFrom;
use std::fs::File;
use std::io::{self, Cursor, Read, Result as IoResult, Write};
use std::sync::mpsc::Receiver;

use crate::common::{self, Header, HeaderError, HttpVersion, StatusCode};
use crate::{ConnectionValue, HeaderField};

pub use self::standard::{Standard, StandardResponse};
use self::transfer_encoding::TransferEncoding;

mod date_header;
mod standard;
mod transfer_encoding;
pub(super) mod util;

/// A `Response` without a template parameter.
pub type ResponseBox = Response<Box<dyn Read + Send>>;

/// Object representing an HTTP response whose purpose is to be given to a `Request`.
///
/// Some headers cannot be changed. Trying to define the value
/// of one of these will have no effect:
///
///  - `Connection`
///  - `Trailer`
///  - `Transfer-Encoding`
///  - `Upgrade`
///
/// Some headers have special behaviors:
///
///  - `Content-Encoding`: If you define this header, the library
///     will assume that the data from the `Read` object has the specified encoding
///     and will just pass-through.
///
///  - `Content-Length`: The length of the data should be set manually
///     using the `Reponse` object's API. Attempting to set the value of this
///     header will be equivalent to modifying the size of the data but the header
///     itself may not be present in the final result.
///
///  - `Content-Type`: You may only set this header to one value at a time. If you
///     try to set it more than once, the existing value will be overwritten. This
///     behavior differs from the default for most headers, which is to allow them to
///     be set multiple times in the same response.
///
#[derive(Debug)]
pub struct Response<R> {
    chunked_threshold: Option<usize>,
    data: Option<R>,
    data_length: Option<usize>,
    filter_headers: Option<HashSet<HeaderField>>,
    headers: Option<Vec<Header>>,
    status_code: StatusCode,
}

impl<R> Default for Response<R> {
    fn default() -> Self {
        Self {
            chunked_threshold: None,
            data: None,
            data_length: None,
            filter_headers: None,
            headers: None,
            status_code: 200.into(),
        }
    }
}

impl<R> Response<R>
where
    R: Read,
{
    /// Creates a new Response object
    ///
    /// The `additional_headers` argument is a receiver that
    /// may provide headers even after the response has been sent.
    ///
    /// All the other arguments are straight-forward.
    pub fn new(
        status_code: StatusCode,
        headers: Vec<Header>,
        data: R,
        data_length: Option<usize>,
        additional_headers: Option<Receiver<Header>>,
    ) -> Self {
        let mut response = Response {
            data: Some(data),
            data_length,
            headers: Some(Vec::with_capacity(16)),
            status_code,
            ..Response::default()
        };

        for h in headers {
            let _ = response.add_header(h);
        }

        // dummy implementation - TODO: nothing implemented what is happening with these receivers
        if let Some(additional_headers) = additional_headers {
            for h in additional_headers {
                let _ = response.add_header(h);
            }
        }

        response
    }

    /// Set a threshold for `Content-Length` where we chose chunked
    /// transfer. Notice that chunked transfer might happen regardless of
    /// this threshold, for instance when the request headers indicate
    /// it is wanted or when there is no `Content-Length`.
    ///
    #[must_use]
    pub fn with_chunked_threshold(mut self, length: usize) -> Self {
        self.chunked_threshold = Some(length);
        self
    }

    /// Convert the response into the underlying `Read` type.
    ///
    /// This is mainly useful for testing as it must consume the `Response`.
    pub fn into_reader(self) -> Option<R> {
        self.data
    }

    /// The current `Content-Length` threshold for switching over to
    /// chunked transfer. The default is 32768 bytes. Notice that
    /// chunked transfer is mutually exclusive with sending a
    /// `Content-Length` header as per the HTTP spec.
    pub fn chunked_threshold(&self) -> usize {
        self.chunked_threshold.unwrap_or(32_768)
    }

    /// Adds a header to the list.
    /// Does all the checks.
    ///
    /// # Errors
    ///
    /// - `[HeaderError]` when header is not added
    ///
    pub fn add_header<H>(&mut self, header: H) -> Result<(), HeaderError>
    where
        H: Into<Header>,
    {
        let header = header.into();

        // ignoring forbidden headers
        if Header::is_modifieable(&header.field) {
            return Err(HeaderError::NonModifiable);
        }

        // if the header is Content-Length, setting the data length
        if header.field.equiv("Content-Length") {
            self.data_length = Some(
                header
                    .value
                    .as_str()
                    .parse::<usize>()
                    .map_err(|_err| HeaderError::Format)?,
            );

            return Ok(());
        }

        if header.field.equiv("Content-Type") {
            util::update_optional_header(&mut self.headers, header, false);
        } else {
            util::update_optional_header(&mut self.headers, header, true);
        }

        Ok(())
    }

    /// Adds headers to the list.
    /// Does all the checks.
    ///
    /// # Errors
    ///
    /// - `[HeaderError]` when header is not added
    ///
    #[inline]
    pub fn add_headers<H>(&mut self, headers: Vec<H>) -> Result<(), HeaderError>
    where
        H: Into<Header>,
    {
        for header in headers {
            let header: Header = header.into();
            self.add_header(header)?;
        }
        Ok(())
    }

    /// Append new filter for headers
    ///
    /// An header filter prevents the addition of the header to the response.
    ///
    /// # Errors
    ///
    /// - `[HeaderError]` when header is not added
    ///
    pub fn filter_header<H>(&mut self, header_field: H) -> Result<(), HeaderError>
    where
        H: Into<HeaderField>,
    {
        let header_field: HeaderField = header_field.into();
        if Header::is_modifieable(&header_field)
            || header_field.as_str().to_ascii_lowercase().as_str() == "date"
        {
            return Err(HeaderError::NonModifiable);
        }

        util::update_optional_hashset(&mut self.filter_headers, [header_field]);

        Ok(())
    }

    /// Returns the same request, but with an additional header.
    ///
    /// Some headers cannot be modified and some other have a
    /// special behavior. See the documentation above.
    ///
    /// # Errors
    ///
    /// - `[HeaderError]` when header is not added
    ///
    #[inline]
    pub fn with_header<H>(mut self, header: H) -> Result<Self, HeaderError>
    where
        H: Into<Header>,
    {
        self.add_header(header.into())?;
        Ok(self)
    }

    /// Returns the same request, but with additional headers.
    ///
    /// Some headers cannot be modified and some other have a
    /// special behavior. See the documentation above.
    ///
    /// # Errors
    ///
    /// - `[HeaderError]` when header is not added
    ///
    #[inline]
    pub fn with_headers<H>(mut self, headers: Vec<H>) -> Result<Self, HeaderError>
    where
        H: Into<Header>,
    {
        self.add_headers(headers)?;
        Ok(self)
    }

    /// Returns the same request, but with a different status code.
    #[must_use]
    #[inline]
    pub fn with_status_code<S>(mut self, code: S) -> Self
    where
        S: Into<StatusCode>,
    {
        self.status_code = code.into();
        self
    }

    /// Returns the same request, but with different data.
    pub fn with_data<D>(self, data: D, data_length: Option<usize>) -> Response<D>
    where
        D: Read,
    {
        Response {
            chunked_threshold: self.chunked_threshold,
            data: Some(data),
            data_length,
            filter_headers: self.filter_headers,
            headers: self.headers,
            status_code: self.status_code,
        }
    }

    /// Prints the HTTP response to a writer.
    ///
    /// This function is the one used to send the response to the client's socket.
    /// Therefore you shouldn't expect anything pretty-printed or even readable.
    ///
    /// The HTTP version and headers passed as arguments are used to
    /// decide which features (most notably, encoding) to use.
    ///
    /// Note: does not flush the writer.
    ///
    /// # Errors
    ///
    /// - `std::io::Error`
    ///
    /// # Panics
    ///
    /// - when `upgrade` is not ascii
    ///
    pub fn raw_print<W: Write>(
        mut self,
        mut writer: W,
        http_version: HttpVersion,
        request_headers: &[Header],
        do_not_send_body: bool,
        upgrade: Option<&str>,
    ) -> IoResult<()> {
        let mut headers = util::set_default_headers_if_not_set(&self.headers);

        let mut transfer_encoding = Some(util::choose_transfer_encoding(
            self.status_code,
            request_headers,
            http_version,
            &self.data_length,
            false, /* TODO */
            self.chunked_threshold(),
        ));

        // handling upgrade
        if let Some(upgrade) = upgrade {
            headers.push(ConnectionValue::Upgrade.into());
            headers.push(Header::from_bytes(b"Upgrade", upgrade.as_bytes()).unwrap());
            transfer_encoding = None;
        }

        // if the transfer encoding is identity, the content length must be known
        // therefore if we don't know it, we buffer the entire response first here
        // while this is an expensive operation, it is only ever needed for clients using HTTP 1.0
        let te_data = match (
            transfer_encoding,
            self.data_length.is_none(),
            self.data.as_mut(),
        ) {
            (Some(TransferEncoding::Identity), true, Some(data)) => {
                let mut buf = Vec::new();
                let _ = data.read_to_end(&mut buf)?;
                let l = buf.len();
                self.data_length = Some(l);
                Some(Cursor::new(buf))
            }
            _ => None,
        };

        // preparing headers for transfer
        util::update_te_headers(&mut headers, transfer_encoding, &self.data_length);

        // if assert fails the `Vec` at beginning should get a new capacity
        debug_assert!(headers.len() <= 6, "headers.len: {}", headers.len());

        // checking whether to ignore the body of the response
        let do_not_send_body =
            do_not_send_body || util::is_body_for_status_ignored(self.status_code);

        if do_not_send_body {
            util::update_optional_hashset(
                &mut self.filter_headers,
                [
                    common::static_header::CONTENT_LENGTH_HEADER_FIELD.clone(),
                    common::static_header::CONTENT_TYPE_HEADER_FIELD.clone(),
                ],
            );
        }

        // sending headers
        util::write_message_header(
            &mut writer,
            http_version,
            self.status_code,
            &headers,
            &self.headers,
            &self.filter_headers,
        )?;

        // sending the body
        if !do_not_send_body {
            match transfer_encoding {
                Some(TransferEncoding::Chunked) => {
                    if let Some(mut reader) = self.data {
                        let mut writer = chunked_transfer::Encoder::new(writer);
                        let _ = io::copy(&mut reader, &mut writer)?;
                    }
                }

                Some(TransferEncoding::Identity) => {
                    debug_assert!(self.data_length.is_some());
                    let data_length = self.data_length.unwrap();

                    if data_length >= 1 {
                        if let Some(mut reader) = te_data {
                            let _ = io::copy(&mut reader, &mut writer)?;
                        } else if let Some(mut reader) = self.data {
                            let _ = io::copy(&mut reader, &mut writer)?;
                        }
                    }
                }

                _ => {}
            }
        }

        Ok(())
    }

    /// Retrieves the current value of the `Response` status code
    pub fn status_code(&self) -> StatusCode {
        self.status_code
    }

    /// Retrieves the current value of the `Response` data length
    pub fn data_length(&self) -> Option<usize> {
        self.data_length
    }

    /// Retrieves the current list of `Response` headers
    pub fn headers(&self) -> Option<&Vec<Header>> {
        self.headers.as_ref()
    }

    /// List of `Response` headers
    pub(crate) fn headers_mut(&mut self) -> &mut Option<Vec<Header>> {
        &mut self.headers
    }
}

impl<R> Response<R>
where
    R: Read + Send + 'static,
{
    /// Turns this response into a `Response<Box<Read + Send>>`.
    pub fn boxed(self) -> ResponseBox {
        Response {
            chunked_threshold: self.chunked_threshold,
            data: if let Some(data) = self.data {
                Some(Box::new(data))
            } else {
                None
            },
            data_length: self.data_length,
            filter_headers: self.filter_headers,
            headers: self.headers,
            status_code: self.status_code,
        }
    }
}

impl Response<File> {
    /// Builds a new `Response` from a `File`.
    ///
    /// The `Content-Type` will **not** be automatically detected,
    /// you must set it yourself.
    #[must_use]
    pub fn from_file(file: File) -> Self {
        let data_length = file
            .metadata()
            .ok()
            .map(|v| usize::try_from(v.len()).unwrap_or(usize::MAX));

        Response {
            data: Some(file),
            data_length,
            ..Response::default()
        }
    }
}

impl Response<Cursor<Vec<u8>>> {
    /// Create [Response] from heap data
    pub fn from_data<D>(data: D) -> Self
    where
        D: Into<Vec<u8>>,
    {
        let data = data.into();

        Response {
            data_length: Some(data.len()),
            data: Some(Cursor::new(data)),
            ..Response::default()
        }
    }

    /// Create [Response] from kind of string
    ///
    /// # Panics
    ///
    /// On internal coding error
    ///
    pub fn from_string<S>(data: S) -> Self
    where
        S: Into<String>,
    {
        let data: String = data.into();

        Response {
            data_length: Some(data.len()),
            data: Some(Cursor::new(data.into_bytes())),
            #[cfg(feature = "content-type")]
            headers: Some(vec![crate::ContentType::TextPlainUtf8.into()]),
            #[cfg(not(feature = "content-type"))]
            headers: Some(vec![Header::from_bytes(
                b"Content-Type",
                b"text/plain; charset=utf8",
            )
            .unwrap()]),
            ..Response::default()
        }
    }
}

impl<'a> Response<Cursor<&'a [u8]>> {
    /// Create [Response] from data
    #[must_use]
    pub fn from_slice(data: &'a [u8]) -> Self {
        Response {
            data: Some(Cursor::new(data)),
            data_length: Some(data.len()),
            ..Response::default()
        }
    }

    /// Create [Response] from `&str`
    ///
    /// # Panics
    ///
    /// On internal coding error
    ///
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(data: &'a str) -> Self {
        let data = data.as_bytes();
        Response {
            data: Some(Cursor::new(data)),
            data_length: Some(data.len()),
            #[cfg(feature = "content-type")]
            headers: Some(vec![crate::ContentType::TextPlainUtf8.into()]),
            #[cfg(not(feature = "content-type"))]
            headers: Some(vec![Header::from_bytes(
                b"Content-Type",
                b"text/plain; charset=utf8",
            )
            .unwrap()]),
            ..Response::default()
        }
    }

    /// Tries to read data like utf8 to `&str`
    pub(crate) fn as_utf8_str(&'a self) -> Result<&'a str, std::str::Utf8Error> {
        self.data.as_ref().map_or(Ok(""), |data| {
            std::str::from_utf8(data.clone().into_inner())
        })
    }
}

impl<T> From<T> for Response<Cursor<&[u8]>>
where
    T: Into<StatusCode>,
{
    fn from(status_code: T) -> Self {
        let status_code: StatusCode = status_code.into();
        let data = status_code.default_reason_phrase().as_bytes();
        let headers = Standard::headers(status_code);

        Response {
            data: Some(Cursor::new(data)),
            data_length: Some(data.len()),
            headers,
            status_code,
            ..Response::default()
        }
    }
}

impl Clone for Response<Cursor<&[u8]>> {
    fn clone(&self) -> Self {
        Self {
            chunked_threshold: self.chunked_threshold,
            data: self.data.clone(),
            data_length: self.data_length,
            filter_headers: self.filter_headers.clone(),
            headers: self.headers.clone(),
            status_code: self.status_code,
        }
    }
}

impl Response<std::io::Empty> {
    /// Builds an empty `Response` with the given status code.
    #[inline]
    pub fn empty<S>(status_code: S) -> Self
    where
        S: Into<StatusCode>,
    {
        let status_code = status_code.into();
        let headers = Standard::headers(status_code);

        Response {
            data_length: Some(0),
            headers,
            status_code,
            ..Response::default()
        }
    }

    /// DEPRECATED. Use `empty` instead.
    #[must_use]
    #[deprecated = "replaced by empty(), will be removed after version 0.13"]
    #[inline]
    pub fn new_empty(status_code: StatusCode) -> Self {
        Self::empty(status_code)
    }
}

impl Clone for Response<io::Empty> {
    fn clone(&self) -> Response<io::Empty> {
        Response {
            data: None,
            data_length: Some(0),
            status_code: self.status_code,
            headers: self.headers.clone(),
            filter_headers: self.filter_headers.clone(),
            chunked_threshold: self.chunked_threshold,
        }
    }
}

impl<R> Response<R>
where
    R: Read + Clone,
{
    /// Prints the HTTP response to a writer.
    ///
    /// This function is the one used to send the response to the client's socket.
    /// Therefore you shouldn't expect anything pretty-printed or even readable.
    ///
    /// The HTTP version and headers passed as arguments are used to
    /// decide which features (most notably, encoding) to use.
    ///
    /// Note: does not flush the writer.
    ///
    /// # Errors
    ///
    /// - `std::io::Error`
    ///
    /// # Panics
    ///
    /// - when `upgrade` is not ascii
    ///
    pub fn raw_print_ref<W: Write>(
        &self,
        mut writer: W,
        http_version: HttpVersion,
        request_headers: &[Header],
        do_not_send_body: bool,
        upgrade: Option<&str>,
    ) -> IoResult<()> {
        let mut headers = util::set_default_headers_if_not_set(&self.headers);

        let mut transfer_encoding = Some(util::choose_transfer_encoding(
            self.status_code,
            request_headers,
            http_version,
            &self.data_length,
            false, /* TODO: additional_headers receiving feature */
            self.chunked_threshold(),
        ));

        // handling upgrade
        if let Some(upgrade) = upgrade {
            headers.push(ConnectionValue::Upgrade.into());
            headers.push(Header::from_bytes(b"Upgrade", upgrade.as_bytes()).unwrap());
            transfer_encoding = None;
        }

        // if the transfer encoding is identity, the content length must be known
        // therefore if we don't know it, we buffer the entire response first here
        // while this is an expensive operation, it is only ever needed for clients using HTTP 1.0
        debug_assert!(
            self.data_length.is_some()
                || (transfer_encoding.is_none()
                    || !matches!(transfer_encoding, Some(TransferEncoding::Identity)))
        );

        // preparing headers for transfer
        util::update_te_headers(&mut headers, transfer_encoding, &self.data_length);

        // if assert fails the `Vec` at beginning should get a new capacity
        debug_assert!(headers.len() <= 6, "headers.len: {}", headers.len());

        // checking whether to ignore the body of the response
        let do_not_send_body =
            do_not_send_body || util::is_body_for_status_ignored(self.status_code);

        let mut filter_headers = self.filter_headers.clone();
        if do_not_send_body {
            util::update_optional_hashset(
                &mut filter_headers,
                [
                    common::static_header::CONTENT_LENGTH_HEADER_FIELD.clone(),
                    common::static_header::CONTENT_TYPE_HEADER_FIELD.clone(),
                ],
            );
        }

        // sending headers
        util::write_message_header(
            &mut writer,
            http_version,
            self.status_code,
            &headers,
            &self.headers,
            &filter_headers,
        )?;

        // sending the body with cloned reader
        if !do_not_send_body {
            match transfer_encoding {
                Some(TransferEncoding::Chunked) => {
                    if let Some(reader) = &self.data {
                        let mut reader = reader.clone();
                        let mut writer = chunked_transfer::Encoder::new(writer);
                        let _ = io::copy(&mut reader, &mut writer)?;
                    }
                }

                Some(TransferEncoding::Identity) => {
                    debug_assert!(self.data_length.is_some());
                    let data_length = self.data_length.unwrap();

                    if data_length >= 1 {
                        if let Some(reader) = &self.data {
                            let mut reader = reader.clone();
                            let _ = io::copy(&mut reader, &mut writer)?;
                        }
                    }
                }

                _ => {}
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::{common::HeaderError, Header};

    use super::Response;

    #[test]
    fn test_with_header() -> Result<(), HeaderError> {
        let mut response = Response::empty(200);

        response = response.with_header(Header::from_str("Content-Type: text/plain")?)?;

        assert!(Header::from_str("BlaBla").is_err());

        let result = response.with_header(Header::from_str("Connection: close")?);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_with_headers() -> Result<(), HeaderError> {
        let mut response = Response::empty(200);

        response = response.with_headers(Vec::from([
            Header::from_str("Content-Type: text/plain")?,
            Header::from_str("Content-Length: 100")?,
        ]))?;

        let result = response.with_headers(Vec::from([
            Header::from_str("Content-Type: text/plain")?,
            Header::from_str("Connection: close")?,
        ]));
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_add_content_length_header() {
        let mut response = Response::from(200);

        response
            .add_header(Header::from_bytes(b"Content-Length", b"123456").unwrap())
            .unwrap();

        assert_eq!(response.data_length().unwrap(), 123_456_usize);
    }

    #[test]
    fn test_add_header() {
        let mut response = Response::from(200);

        for header in [
            Header::from_bytes(b"Content-Type", b"application/json").unwrap(),
            Header::from_bytes(b"Content-Type", b"application/binary").unwrap(),
            Header::from_bytes(b"X-Sample-Header", b"test").unwrap(),
            Header::from_bytes(b"X-Sample-Header", b"test").unwrap(),
        ] {
            response.add_header(header.clone()).unwrap();

            if header.field.equiv("Content-Type") {
                let count = response
                    .headers()
                    .unwrap()
                    .iter()
                    .filter(|h| h.field == header.field)
                    .count();
                assert_eq!(count, 1, "count: {count} header: {header:?}");

                let count = response
                    .headers()
                    .unwrap()
                    .iter()
                    .filter(|h| h.field == header.field && h.value == header.value)
                    .count();
                assert_eq!(count, 1, "count: {count} header: {header:?}");
            } else {
                let count = response
                    .headers()
                    .unwrap()
                    .iter()
                    .filter(|h| {
                        let _a = h.field.to_string();
                        let _b = header.field.to_string();
                        h.field == header.field && h.value == header.value
                    })
                    .count();
                assert!(count >= 1, "count: {}, header: {:?}", count, header);
            }
        }
    }
}
