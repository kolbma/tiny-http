use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Cursor, Read, Result as IoResult, Write};
use std::str::FromStr;
use std::sync::mpsc::Receiver;
use std::time::SystemTime;

use httpdate::HttpDate;

use crate::common::{Header, HeaderError, HttpVersion, StatusCode};
use crate::HeaderField;

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
    reader: R,
    status_code: StatusCode,
    headers: Vec<Header>,
    filter_headers: HashSet<HeaderField>,
    data_length: Option<usize>,
    chunked_threshold: Option<usize>,
}

/// A `Response` without a template parameter.
pub type ResponseBox = Response<Box<dyn Read + Send>>;

/// Transfer encoding to use when sending the message.
/// Note that only *supported* encoding are listed here.
#[derive(Copy, Clone)]
enum TransferEncoding {
    Identity,
    Chunked,
}

impl FromStr for TransferEncoding {
    type Err = ();

    fn from_str(input: &str) -> Result<TransferEncoding, ()> {
        if input.eq_ignore_ascii_case("identity") {
            Ok(TransferEncoding::Identity)
        } else if input.eq_ignore_ascii_case("chunked") {
            Ok(TransferEncoding::Chunked)
        } else {
            Err(())
        }
    }
}

/// Builds a Date: header with the current date.
fn build_date_header() -> Header {
    let d = HttpDate::from(SystemTime::now());
    Header::from_bytes(&b"Date"[..], &d.to_string().into_bytes()[..]).unwrap()
}

fn write_message_header<W>(
    writer: &mut W,
    http_version: HttpVersion,
    status_code: StatusCode,
    headers: &[Header],
    filter_headers: &HashSet<HeaderField>,
) -> IoResult<()>
where
    W: Write,
{
    // writing status line
    write!(
        writer,
        "{} {} {}\r\n",
        http_version.header(),
        status_code.0,
        status_code.default_reason_phrase()
    )?;

    // writing headers
    for header in headers {
        if !filter_headers.contains(&header.field) || header.field.equiv("Date") {
            writer.write_all(header.field.as_str().as_ref())?;
            write!(writer, ": ")?;
            writer.write_all(header.value.as_str().as_ref())?;
            write!(writer, "\r\n")?;
        }
    }

    // separator between header and data
    write!(writer, "\r\n")?;

    Ok(())
}

fn choose_transfer_encoding(
    status_code: StatusCode,
    request_headers: &[Header],
    http_version: HttpVersion,
    entity_length: &Option<usize>,
    has_additional_headers: bool,
    chunked_threshold: usize,
) -> TransferEncoding {
    use crate::util;

    // HTTP 1.0 doesn't support other encoding
    if http_version <= HttpVersion::Version1_0 {
        return TransferEncoding::Identity;
    }

    // Per section 3.3.1 of RFC7230:
    // A server MUST NOT send a Transfer-Encoding header field in any response with a status code
    // of 1xx (Informational) or 204 (No Content).
    if status_code.0 < 200 || status_code.0 == 204 {
        return TransferEncoding::Identity;
    }

    // parsing the request's TE header
    let user_request = request_headers
        .iter()
        // finding TE and get value
        .find_map(|h| {
            // getting the corresponding TransferEncoding
            if h.field.equiv("TE") {
                // getting list of requested elements
                let mut parse = util::parse_header_value(h.value.as_str()); // TODO: remove conversion

                // sorting elements by most priority
                parse.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

                // trying to parse each requested encoding
                for value in parse {
                    // q=0 are ignored
                    if value.1 <= 0.0 {
                        continue;
                    }

                    if let Ok(te) = TransferEncoding::from_str(value.0) {
                        return Some(te);
                    }
                }
            }

            // No transfer encoding found
            None
        });

    if let Some(user_request) = user_request {
        return user_request;
    }

    // if we have additional headers, using chunked
    if has_additional_headers {
        return TransferEncoding::Chunked;
    }

    // if we don't have a Content-Length, or if the Content-Length is too big, using chunks writer
    if entity_length
        .as_ref()
        .map_or(true, |val| *val >= chunked_threshold)
    {
        return TransferEncoding::Chunked;
    }

    // Identity by default
    TransferEncoding::Identity
}

impl<R> Response<R>
where
    R: Read,
{
    /// Creates a new Response object.
    ///
    /// The `additional_headers` argument is a receiver that
    ///  may provide headers even after the response has been sent.
    ///
    /// All the other arguments are straight-forward.
    pub fn new(
        status_code: StatusCode,
        headers: Vec<Header>,
        data: R,
        data_length: Option<usize>,
        additional_headers: Option<Receiver<Header>>,
    ) -> Response<R> {
        let mut response = Response {
            reader: data,
            status_code,
            headers: Vec::with_capacity(16),
            filter_headers: HashSet::with_capacity(0),
            data_length,
            chunked_threshold: None,
        };

        for h in headers {
            let _ = response.add_header(h);
        }

        // dummy implementation
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
    pub fn with_chunked_threshold(mut self, length: usize) -> Response<R> {
        self.chunked_threshold = Some(length);
        self
    }

    /// Convert the response into the underlying `Read` type.
    ///
    /// This is mainly useful for testing as it must consume the `Response`.
    pub fn into_reader(self) -> R {
        self.reader
    }

    /// The current `Content-Length` threshold for switching over to
    /// chunked transfer. The default is 32768 bytes. Notice that
    /// chunked transfer is mutually exclusive with sending a
    /// `Content-Length` header as per the HTTP spec.
    pub fn chunked_threshold(&self) -> usize {
        self.chunked_threshold.unwrap_or(32768)
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
            return Err(HeaderError);
        }

        // if the header is Content-Length, setting the data length
        if header.field.equiv("Content-Length") {
            if let Ok(val) = usize::from_str(header.value.as_str()) {
                self.data_length = Some(val);
            }

            return Ok(());
        // if the header is Content-Type and it's already set, overwrite it
        } else if header.field.equiv("Content-Type") {
            if let Some(content_type_header) = self
                .headers
                .iter_mut()
                .find(|h| h.field.equiv("Content-Type"))
            {
                content_type_header.value = header.value;
                return Ok(());
            }
        }

        self.headers.push(header);
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
            return Err(HeaderError);
        }
        let _ = self.filter_headers.insert(header_field);
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
    pub fn with_header<H>(mut self, header: H) -> Result<Response<R>, HeaderError>
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
    pub fn with_headers<H>(mut self, headers: Vec<H>) -> Result<Response<R>, HeaderError>
    where
        H: Into<Header>,
    {
        self.add_headers(headers)?;
        Ok(self)
    }

    /// Returns the same request, but with a different status code.
    #[must_use]
    #[inline]
    pub fn with_status_code<S>(mut self, code: S) -> Response<R>
    where
        S: Into<StatusCode>,
    {
        self.status_code = code.into();
        self
    }

    /// Returns the same request, but with different data.
    pub fn with_data<S>(self, reader: S, data_length: Option<usize>) -> Response<S>
    where
        S: Read,
    {
        Response {
            reader,
            headers: self.headers,
            filter_headers: self.filter_headers,
            status_code: self.status_code,
            data_length,
            chunked_threshold: self.chunked_threshold,
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
        let mut transfer_encoding = Some(choose_transfer_encoding(
            self.status_code,
            request_headers,
            http_version,
            &self.data_length,
            false, /* TODO */
            self.chunked_threshold(),
        ));

        // add `Date` if not in the headers
        if !self.headers.iter().any(|h| h.field.equiv("Date")) {
            self.headers.insert(0, build_date_header());
        }

        // add `Server` if not in the headers
        if !self.headers.iter().any(|h| h.field.equiv("Server")) {
            self.headers.insert(
                0,
                Header::from_bytes(b"Server", b"tiny-http (Rust)").unwrap(),
            );
        }

        // handling upgrade
        if let Some(upgrade) = upgrade {
            self.headers.insert(
                0,
                Header::from_bytes(b"Upgrade", upgrade.as_bytes()).unwrap(),
            );
            self.headers
                .insert(0, Header::from_bytes(b"Connection", b"upgrade").unwrap());
            transfer_encoding = None;
        }

        // if the transfer encoding is identity, the content length must be known ; therefore if
        // we don't know it, we buffer the entire response first here
        // while this is an expensive operation, it is only ever needed for clients using HTTP 1.0
        let (mut reader, data_length): (Box<dyn Read>, _) =
            match (self.data_length, transfer_encoding) {
                (Some(l), _) => (Box::new(self.reader), Some(l)),
                (None, Some(TransferEncoding::Identity)) => {
                    let mut buf = Vec::new();
                    let _ = self.reader.read_to_end(&mut buf)?;
                    let l = buf.len();
                    (Box::new(Cursor::new(buf)), Some(l))
                }
                _ => (Box::new(self.reader), None),
            };

        // checking whether to ignore the body of the response
        let do_not_send_body =
            do_not_send_body || matches!(self.status_code.0, 100..=199 | 204 | 304); // status code 1xx, 204 and 304 MUST not include a body

        // preparing headers for transfer
        match transfer_encoding {
            Some(TransferEncoding::Chunked) => self
                .headers
                .push(Header::from_bytes(b"Transfer-Encoding", b"chunked").unwrap()),

            Some(TransferEncoding::Identity) => {
                assert!(data_length.is_some());
                let data_length = data_length.unwrap();

                self.headers.push(
                    Header::from_bytes(b"Content-Length", format!("{data_length}").as_bytes())
                        .unwrap(),
                );
            }

            _ => (),
        };

        // sending headers
        write_message_header(
            &mut writer,
            http_version,
            self.status_code,
            &self.headers,
            &self.filter_headers,
        )?;

        // sending the body
        if !do_not_send_body {
            match transfer_encoding {
                Some(TransferEncoding::Chunked) => {
                    use chunked_transfer::Encoder;

                    let mut writer = Encoder::new(writer);
                    let _ = io::copy(&mut reader, &mut writer)?;
                }

                Some(TransferEncoding::Identity) => {
                    assert!(data_length.is_some());
                    let data_length = data_length.unwrap();

                    if data_length >= 1 {
                        let _ = io::copy(&mut reader, &mut writer)?;
                    }
                }

                _ => (),
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
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// List of `Response` headers
    pub(crate) fn headers_mut(&mut self) -> &mut Vec<Header> {
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
            reader: Box::new(self.reader),
            status_code: self.status_code,
            headers: self.headers,
            filter_headers: self.filter_headers,
            data_length: self.data_length,
            chunked_threshold: self.chunked_threshold,
        }
    }
}

impl Response<File> {
    /// Builds a new `Response` from a `File`.
    ///
    /// The `Content-Type` will **not** be automatically detected,
    /// you must set it yourself.
    #[must_use]
    pub fn from_file(file: File) -> Response<File> {
        #[allow(clippy::cast_possible_truncation)]
        let file_size = file.metadata().ok().map(|v| v.len() as usize);

        Response::new(
            StatusCode(200),
            Vec::with_capacity(0),
            file,
            file_size,
            None,
        )
    }
}

impl Response<Cursor<Vec<u8>>> {
    /// Create [Response] from heap data
    pub fn from_data<D>(data: D) -> Response<Cursor<Vec<u8>>>
    where
        D: Into<Vec<u8>>,
    {
        let data = data.into();
        let data_len = data.len();

        Response::new(
            StatusCode(200),
            Vec::with_capacity(0),
            Cursor::new(data),
            Some(data_len),
            None,
        )
    }

    /// Create [Response] from kind of string
    ///
    /// # Panics
    ///
    ///
    pub fn from_string<S>(data: S) -> Response<Cursor<Vec<u8>>>
    where
        S: Into<String>,
    {
        let data = data.into();
        let data_len = data.len();

        Response::new(
            StatusCode(200),
            vec![Header::from_bytes(b"Content-Type", b"text/plain; charset=UTF-8").unwrap()],
            Cursor::new(data.into_bytes()),
            Some(data_len),
            None,
        )
    }
}

impl Response<io::Empty> {
    /// Builds an empty `Response` with the given status code.
    pub fn empty<S>(status_code: S) -> Response<io::Empty>
    where
        S: Into<StatusCode>,
    {
        Response::new(
            status_code.into(),
            Vec::with_capacity(0),
            io::empty(),
            Some(0),
            None,
        )
    }

    /// DEPRECATED. Use `empty` instead.
    #[must_use]
    #[deprecated = "replaced by empty()"]
    pub fn new_empty(status_code: StatusCode) -> Response<io::Empty> {
        Response::empty(status_code)
    }
}

impl Clone for Response<io::Empty> {
    fn clone(&self) -> Response<io::Empty> {
        Response {
            reader: io::empty(),
            status_code: self.status_code,
            headers: self.headers.clone(),
            filter_headers: self.filter_headers.clone(),
            data_length: self.data_length,
            chunked_threshold: self.chunked_threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, str::FromStr};

    use crate::{
        common::HeaderError,
        response::{build_date_header, write_message_header},
        Header, HeaderField, HttpVersion,
    };

    use super::Response;

    #[test]
    fn test_filter_header() -> Result<(), HeaderError> {
        assert!(HashSet::from([HeaderField::from_str("Server")?])
            .contains(&HeaderField::from_str("server")?));

        let mut writer = Vec::new();
        let result = write_message_header(
            &mut writer,
            HttpVersion::Version1_1,
            200.into(),
            &[
                build_date_header(),
                Header::from_str("Server: tiny-http").unwrap(),
            ],
            &HashSet::from([HeaderField::from_str("Date")?]),
        );
        assert!(result.is_ok());

        let s = String::from_utf8(writer).expect("no utf8");
        assert!(s.contains("Server:"), "{}", s);
        assert!(s.contains("Date:"), "{}", s);

        let mut writer = Vec::new();
        let result = write_message_header(
            &mut writer,
            HttpVersion::Version1_1,
            200.into(),
            &[
                build_date_header(),
                Header::from_str("Server: tiny-http").unwrap(),
            ],
            &HashSet::from([HeaderField::from_str("Server")?]),
        );
        assert!(result.is_ok());

        let s = String::from_utf8(writer).expect("no utf8");
        assert!(!s.contains("Server:"), "{}", s);
        Ok(())
    }

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
}
