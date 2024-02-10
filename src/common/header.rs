use std::{
    convert::TryFrom,
    hash::{Hash, Hasher},
    str::FromStr,
};

use ascii::{AsAsciiStrError, AsciiStr, AsciiString};

use crate::response::util::number_to_bytes;

/// Represents a HTTP header.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Header {
    /// `field` of ['Header']
    pub field: HeaderField,
    /// `value` ['HeaderFieldValue'] for ['HeaderField']
    pub value: HeaderFieldValue,
}

impl Header {
    /// Builds a `Header` from two `Vec<u8>`s or two `&[u8]`s.
    ///
    /// # Errors
    ///
    /// An [`HeaderError`] is caused by content with invalid range of ASCII.
    ///
    /// # Examples
    ///
    /// ```
    /// let header = tiny_http::Header::from_bytes(b"Content-Type", b"text/plain").unwrap();
    /// ```
    pub fn from_bytes<F, V>(field: &F, value: &V) -> Result<Header, HeaderError>
    where
        F: Into<Vec<u8>> + AsRef<[u8]>,
        V: Into<Vec<u8>> + AsRef<[u8]>,
    {
        let field = HeaderField::from_bytes(field)?;
        let value = HeaderFieldValue::from_bytes(value)?;

        Ok(Header { field, value })
    }

    /// `true` if [`Header`] `field` can be added and modified
    #[inline]
    pub(crate) fn is_modifieable(field: &HeaderField) -> bool {
        HEADER_FORBIDDEN.contains(&field.as_str().to_ascii_lowercase().as_str())
    }
}

impl FromStr for Header {
    type Err = HeaderError;

    fn from_str(input: &str) -> Result<Header, HeaderError> {
        Self::try_from(input.as_bytes())
    }
}

impl std::fmt::Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_str(self.field.as_str())?;
        f.write_str(": ")?;
        f.write_str(self.value.as_str())
    }
}

/// Tries to create `Header` by a header line
impl TryFrom<&[u8]> for Header {
    type Error = HeaderError;

    fn try_from(input: &[u8]) -> Result<Self, Self::Error> {
        let mut after_colon_pos = 0_usize;
        for b in input {
            after_colon_pos += 1;
            if *b == b':' {
                break;
            }
        }

        let input_len = input.len();

        if after_colon_pos == 0 || after_colon_pos == input_len {
            return Err(HeaderError::Format);
        }

        Self::try_from((&input[0..(after_colon_pos - 1)], &input[after_colon_pos..]))
    }
}

/// Tries to create `Header` by tuple of _field_, _value_
impl TryFrom<(&[u8], &[u8])> for Header {
    type Error = HeaderError;

    fn try_from((input_field, input_value): (&[u8], &[u8])) -> Result<Self, Self::Error> {
        let field = HeaderField::try_from(input_field)?;

        let mut first_non_space = 0;
        for b in input_value {
            if *b != b' ' {
                break;
            }
            first_non_space += 1;
        }

        let mut last_non_space = input_value.len() - 1;

        #[allow(clippy::mut_range_bound, clippy::needless_range_loop)]
        for n in last_non_space..first_non_space {
            if input_value[n] != b' ' {
                break;
            }

            last_non_space = n; // intention
        }

        debug_assert!(
            first_non_space <= last_non_space,
            "input: {:?} {} [{} <= {} ?]",
            &input_value,
            std::str::from_utf8(input_value).unwrap(),
            first_non_space,
            last_non_space
        );

        let value = HeaderFieldValue::try_from(&input_value[first_non_space..=last_non_space])?;

        Ok(Header { field, value })
    }
}

impl TryFrom<&AsciiStr> for Header {
    type Error = HeaderError;

    fn try_from(input: &AsciiStr) -> Result<Self, Self::Error> {
        Self::try_from(input.as_bytes())
    }
}

/// Field of an header (eg. `Content-Type`, `Content-Length`, etc.)
///
/// Comparison between two `HeaderField`s ignores case.
#[derive(Debug, Clone, Eq)]
pub struct HeaderField(AsciiString);

impl HeaderField {
    /// Create [`HeaderField`] from `bytes`
    ///
    /// # Errors
    ///
    /// - [`HeaderError`] for `bytes` conversion
    ///
    pub fn from_bytes<B>(bytes: &B) -> Result<HeaderField, HeaderError>
    where
        B: Into<Vec<u8>> + AsRef<[u8]>,
    {
        let bytes = bytes.as_ref();
        field_byte_range_check(bytes)?;

        Ok(HeaderField(
            AsciiString::from_ascii(bytes).map_err(|err| HeaderError::Ascii(err.ascii_error()))?,
        ))
    }

    /// Get [`HeaderField`] as `&[u8]`
    #[must_use]
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Get [`HeaderField`] as `&AsciiStr`
    #[must_use]
    #[inline]
    pub fn as_ascii_str(&self) -> &AsciiStr {
        &self.0
    }

    /// Get [`HeaderField`] as `&str`
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Checks [`HeaderField`] for equivalence ignoring case of letters
    #[must_use]
    pub fn equiv(&self, other: &'static str) -> bool {
        other.eq_ignore_ascii_case(self.as_str())
    }
}

/// Checks `bytes` for valid byte range for field names as
/// defined in [RFC9110](https://datatracker.ietf.org/doc/html/rfc9110#name-tokens)
#[inline]
fn field_byte_range_check(bytes: &[u8]) -> Result<(), HeaderError> {
    for &b in bytes {
        // Ordered to most used in header fields
        #[allow(clippy::manual_range_contains)]
        if (b >= 94 && b <= 122)
            || (b >= 65 && b <= 90)
            || b == 45
            || (b >= 48 && b <= 57)
            || ([33, 35, 36, 37, 38, 39, 42, 43, 46].contains(&b))
        {
            continue;
        }
        return Err(HeaderError::Range);
    }
    Ok(())
}

impl FromStr for HeaderField {
    type Err = HeaderError;

    fn from_str(s: &str) -> Result<HeaderField, HeaderError> {
        // be sure to check byte range if this is changed
        Self::try_from(s.as_bytes())
    }
}

impl TryFrom<&[u8]> for HeaderField {
    type Error = HeaderError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        field_byte_range_check(bytes)?;

        Ok(HeaderField(
            AsciiString::from_ascii(bytes).map_err(|err| HeaderError::Ascii(err.ascii_error()))?,
        ))
    }
}

impl TryFrom<&AsciiStr> for HeaderField {
    type Error = HeaderError;

    fn try_from(asciistr: &AsciiStr) -> Result<Self, Self::Error> {
        // be sure to check byte range if this is changed
        Self::try_from(asciistr.to_ascii_string())
    }
}

impl TryFrom<AsciiString> for HeaderField {
    type Error = HeaderError;

    fn try_from(ascii_string: AsciiString) -> Result<Self, Self::Error> {
        field_byte_range_check(ascii_string.as_bytes())?;

        Ok(HeaderField(ascii_string))
    }
}

impl std::fmt::Display for HeaderField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_str(self.0.as_str())
    }
}

impl PartialEq for HeaderField {
    fn eq(&self, other: &HeaderField) -> bool {
        let self_bytes = self.as_bytes();
        let other_bytes = other.as_bytes();
        self_bytes.eq_ignore_ascii_case(other_bytes)
    }
}

impl PartialEq<&[u8]> for HeaderField {
    fn eq(&self, other: &&[u8]) -> bool {
        self.0.as_bytes().eq_ignore_ascii_case(other)
    }
}

impl PartialEq<&str> for HeaderField {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_str().eq_ignore_ascii_case(other)
    }
}

impl Hash for HeaderField {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}

/// Value for an header field
///
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HeaderFieldValue(AsciiString);

impl HeaderFieldValue {
    /// Create [`HeaderFieldValue`] from `bytes`
    ///
    /// # Errors
    ///
    /// - [`HeaderError`] for `bytes` conversion
    ///
    pub fn from_bytes<B>(bytes: &B) -> Result<HeaderFieldValue, HeaderError>
    where
        B: Into<Vec<u8>> + AsRef<[u8]>,
    {
        let bytes = bytes.as_ref();
        field_value_byte_range_check(bytes)?;

        Ok(HeaderFieldValue(
            AsciiString::from_ascii(bytes).map_err(|err| HeaderError::Ascii(err.ascii_error()))?,
        ))
    }

    /// Get [`HeaderFieldValue`] as `&[u8]`
    #[must_use]
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Get [`HeaderFieldValue`] as `&AsciiStr`
    #[must_use]
    #[inline]
    pub fn as_ascii_str(&self) -> &AsciiStr {
        &self.0
    }

    /// Get [`HeaderFieldValue`] as `&str`
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Create [`HeaderFieldValue`] from existing `AsciiString`
    pub(crate) fn from_ascii_unchecked(ascii_string: AsciiString) -> HeaderFieldValue {
        Self(ascii_string)
    }
}

/// Checks `bytes` for valid byte range for field values as
/// defined in [RFC9110](https://datatracker.ietf.org/doc/html/rfc9110#name-field-values)
#[inline]
fn field_value_byte_range_check(bytes: &[u8]) -> Result<(), HeaderError> {
    for &b in bytes {
        // Ordered to most used in header fields
        #[allow(clippy::manual_range_contains)]
        if (b >= 32 && b <= 126) || b == 9 || b >= 128 {
            continue;
        }
        return Err(HeaderError::Range);
    }
    Ok(())
}

impl FromStr for HeaderFieldValue {
    type Err = HeaderError;

    fn from_str(s: &str) -> Result<HeaderFieldValue, HeaderError> {
        // be sure to check byte range if this is changed
        Self::try_from(s.as_bytes())
    }
}

impl TryFrom<&[u8]> for HeaderFieldValue {
    type Error = HeaderError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        field_value_byte_range_check(bytes)?;

        Ok(HeaderFieldValue(
            AsciiString::from_ascii(bytes).map_err(|err| HeaderError::Ascii(err.ascii_error()))?,
        ))
    }
}

impl TryFrom<&AsciiStr> for HeaderFieldValue {
    type Error = HeaderError;

    fn try_from(asciistr: &AsciiStr) -> Result<Self, Self::Error> {
        // be sure to check byte range if this is changed
        Self::try_from(asciistr.to_ascii_string())
    }
}

impl TryFrom<AsciiString> for HeaderFieldValue {
    type Error = HeaderError;

    fn try_from(ascii_string: AsciiString) -> Result<Self, Self::Error> {
        field_value_byte_range_check(ascii_string.as_bytes())?;

        Ok(HeaderFieldValue(ascii_string))
    }
}

impl TryFrom<usize> for HeaderFieldValue {
    type Error = HeaderError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(HeaderFieldValue({
            let mut buf = [0; 20]; // 20 is 64bit max digits
            AsciiString::from_ascii(number_to_bytes!(value, &mut buf, 20))
                .map_err(|err| HeaderError::Ascii(err.ascii_error()))?
        }))
    }
}

impl std::fmt::Display for HeaderFieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_str(self.0.as_str())
    }
}

impl PartialEq<&[u8]> for HeaderFieldValue {
    fn eq(&self, other: &&[u8]) -> bool {
        self.0.as_bytes() == *other
    }
}

impl PartialEq<&str> for HeaderFieldValue {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_str() == *other
    }
}

impl std::ops::Deref for HeaderFieldValue {
    type Target = AsciiString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Needs to be lower-case!!!
pub(crate) const HEADER_FORBIDDEN: &[&str] =
    &["connection", "trailer", "transfer-encoding", "upgrade"];

/// Header was not added
#[derive(Debug)]
pub enum HeaderError {
    /// Value is not completly in ASCII range
    Ascii(AsAsciiStrError),
    /// Provided data is no valid [`Header`] line
    Format,
    /// It is not possible to change the specific [`Header`]
    NonModifiable,
    /// Provided data could be ASCII but is not in a more restrictive range
    Range,
}

impl std::error::Error for HeaderError {}

impl std::fmt::Display for HeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("header not allowed")
    }
}

#[cfg(test)]
mod test {
    use std::{
        convert::TryFrom,
        time::{Duration, SystemTime},
    };

    use ascii::{AsAsciiStr, AsciiStr, AsciiString};
    use httpdate::HttpDate;

    use super::{
        field_byte_range_check, field_value_byte_range_check, Header, HeaderField,
        HeaderFieldValue, HEADER_FORBIDDEN,
    };

    #[test]
    fn field_converter_byte_range_check_test() {
        assert!(HeaderField::from_bytes(b"user@host").is_err());
        assert!("user@host".parse::<HeaderField>().is_err());
        assert!(HeaderField::try_from(&b"user@host"[..]).is_err());
        assert!(HeaderField::try_from(AsciiStr::from_ascii("user@host").unwrap()).is_err());
        assert!(HeaderField::try_from(AsciiString::from_ascii("user@host").unwrap()).is_err());
    }

    #[test]
    fn field_value_converter_byte_range_check_test() {
        assert!(HeaderFieldValue::from_bytes(b"\n").is_err());
        assert!("\n".parse::<HeaderFieldValue>().is_err());
        assert!(HeaderFieldValue::try_from(&b"\n"[..]).is_err());
        assert!(HeaderFieldValue::try_from(AsciiStr::from_ascii("\n").unwrap()).is_err());
        assert!(HeaderFieldValue::try_from(AsciiString::from_ascii("\n").unwrap()).is_err());
    }

    #[test]
    fn field_byte_range_check_test() {
        let field_ok_array = &[
            "Host",
            "HOST",
            "host",
            "User-Agent",
            "Upgrade-Insecure-Requests",
            "X_CUSTOM_HEADER",
            "$X_CUSTOM_HEADER",
        ];

        for s in field_ok_array {
            assert!(field_byte_range_check(s.as_bytes()).is_ok(), "field: {}", s);
        }

        let field_err_array = &[
            "\"Host\"",
            "HOST:",
            "user@host",
            "User-(Mozilla-Agent",
            "User-Mozilla)-Agent",
            "Upgrade-Insecure-Requests;",
            "Upgrade-Insecure-Requests,",
            "{$X_CUSTOM_HEADER",
            "$X_CUSTOM_HEADER}",
            "Host\rHost: localhost",
            "Host\0",
            "Host\n",
            "Host\\",
            "Host<user",
            "Host>user",
            "Host=user",
            "Host/user",
            "User-[Mozilla-Agent",
            "User-Mozilla]-Agent",
            "Host?",
            " Host",
            "\tHost",
            "	Host",
        ];

        for s in field_err_array {
            assert!(
                field_byte_range_check(s.as_bytes()).is_err(),
                "field: {}",
                s
            );
        }
    }

    #[test]
    fn field_value_byte_range_check_test() {
        let value_ok_array = &[
            "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/115.0",
            "Mozilla/5.0 (X11; Linux x86_64; rv:109.0)	Gecko/20100101 Firefox/115.0",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ];

        for s in value_ok_array {
            assert!(
                field_value_byte_range_check(s.as_bytes()).is_ok(),
                "value: {}",
                s
            );
        }

        let value_err_array = &[
            "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/115.0\r",
            "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/115.0\n",
            "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/115.0\0",
        ];

        for s in value_err_array {
            assert!(
                field_value_byte_range_check(s.as_bytes()).is_err(),
                "value: {}",
                s
            );
        }

        for b in [8_u8, 31, 127] {
            assert!(
                field_value_byte_range_check(&[b]).is_err(),
                "value: {:X}",
                b
            );
        }
    }

    #[test]
    fn formats_date_correctly_test() {
        let http_date = HttpDate::from(SystemTime::UNIX_EPOCH + Duration::from_secs(420_895_020));

        assert_eq!(http_date.to_string(), "Wed, 04 May 1983 11:17:00 GMT");
    }

    #[test]
    fn header_forbidden_lc_test() {
        for h in HEADER_FORBIDDEN {
            assert_eq!(h, &h.to_lowercase());
        }
    }

    #[test]
    fn header_try_from_ascii_test() {
        let header: Header =
            Header::try_from("Content-Type: text/html".as_ascii_str().unwrap()).unwrap();

        assert!(header.field.equiv("content-type"));
        assert!(header.value.as_str() == "text/html");
    }

    #[test]
    fn header_with_doublecolon_try_from_ascii_test() {
        let header: Header = Header::try_from("Time: 20: 34".as_ascii_str().unwrap()).unwrap();

        assert!(header.field.equiv("time"));
        assert!(header.value.as_str() == "20: 34");
    }

    #[test]
    fn parse_header_test() {
        let s = "Content-Type: text/html";
        let header: Header = s.parse().unwrap();

        assert!(header.field.equiv("content-type"));
        assert!(header.value.as_str() == "text/html");

        assert_eq!(&header.to_string(), s);

        assert!("hello world".parse::<Header>().is_err());
    }

    #[test]
    fn parse_header_with_doublecolon_test() {
        let header: Header = "Time: 20: 34".parse().unwrap();

        assert!(header.field.equiv("time"));
        assert!(header.value.as_str() == "20: 34");
    }

    // This tests resistance to RUSTSEC-2020-0031: "HTTP Request smuggling
    // through malformed Transfer Encoding headers"
    // (https://rustsec.org/advisories/RUSTSEC-2020-0031.html).
    #[test]
    fn strict_headers_test() {
        assert!("Transfer-Encoding : chunked".parse::<Header>().is_err());
        assert!(" Transfer-Encoding: chunked".parse::<Header>().is_err());
        assert!("Transfer Encoding: chunked".parse::<Header>().is_err());
        assert!(" Transfer\tEncoding : chunked".parse::<Header>().is_err());
        assert!("Transfer-Encoding: chunked".parse::<Header>().is_ok());
        assert!("Transfer-Encoding: chunked ".parse::<Header>().is_ok());
        assert!("Transfer-Encoding:   chunked ".parse::<Header>().is_ok());
    }

    #[test]
    fn strict_headers_try_from_ascii_test() {
        for s in [
            "Transfer-Encoding : chunked",
            " Transfer-Encoding: chunked",
            "Transfer Encoding: chunked",
            " Transfer\tEncoding : chunked",
        ] {
            let header = Header::try_from(s.as_ascii_str().unwrap());
            assert!(
                header.is_err(),
                "{} should not convert to {:#?}",
                s,
                header.unwrap()
            );
        }

        for s in [
            "Transfer-Encoding: chunked",
            "Transfer-Encoding: chunked ",
            "Transfer-Encoding:   chunked ",
        ] {
            let header = Header::try_from(s.as_ascii_str().unwrap());
            assert!(
                header.is_ok(),
                "{} should convert: {:#?}",
                s,
                header.unwrap_err()
            );
        }
    }
}
