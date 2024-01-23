use std::{convert::TryFrom, str::FromStr};

use ascii::{AsciiChar, AsciiStr, AsciiString, FromAsciiError};

/// Represents a HTTP header.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Header {
    /// `field` of [Header]
    pub field: HeaderField,
    /// `value` for [HeaderField]
    pub value: AsciiString,
}

impl Header {
    /// Builds a `Header` from two `Vec<u8>`s or two `&[u8]`s.
    ///
    /// # Errors
    ///
    /// - mapped `FromAsciiError` for `header`
    /// - mapped `FromAsciiError` for `value`
    ///
    /// # Examples
    ///
    /// ```
    /// let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap();
    /// ```
    #[allow(clippy::result_unit_err)]
    pub fn from_bytes(header: &[u8], value: &[u8]) -> Result<Header, ()> {
        let header = HeaderField::from_bytes(header).or(Err(()))?;
        let value = AsciiString::from_ascii(value).or(Err(()))?;

        Ok(Header {
            field: header,
            value,
        })
    }

    /// `true` if `[Header]` `field` can be added and modified
    #[inline]
    pub(crate) fn is_modifieable(field: &HeaderField) -> bool {
        HEADER_FORBIDDEN.contains(&field.as_str().to_ascii_lowercase().as_str())
    }
}

impl FromStr for Header {
    type Err = HeaderError;

    fn from_str(input: &str) -> Result<Header, HeaderError> {
        let mut elems = input.splitn(2, ':');

        let field = elems
            .next()
            .and_then(|f| f.parse().ok())
            .ok_or(HeaderError)?;
        let value = elems
            .next()
            .and_then(|v| AsciiString::from_ascii(v.trim()).ok())
            .ok_or(HeaderError)?;

        Ok(Header { field, value })
    }
}

impl std::fmt::Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}: {}", self.field, self.value.as_str())
    }
}

impl TryFrom<&AsciiStr> for Header {
    type Error = ();

    fn try_from(input: &AsciiStr) -> Result<Self, Self::Error> {
        let field_s = input.split(AsciiChar::Colon).next();
        let field = field_s
            .and_then(|f| HeaderField::try_from(f).ok())
            .ok_or(())?;

        let value = input[(field_s.unwrap().len() + 1)..].trim().to_owned();

        Ok(Header { field, value })
    }
}

/// Field of a header (eg. `Content-Type`, `Content-Length`, etc.)
///
/// Comparison between two `HeaderField`s ignores case.
#[derive(Debug, Clone, Eq)]
pub struct HeaderField(AsciiString);

impl HeaderField {
    /// Create `[HeaderField]` from `bytes`
    ///
    /// # Errors
    ///
    /// - `FromAsciiError` for `bytes` conversion
    ///
    pub fn from_bytes<B>(bytes: B) -> Result<HeaderField, FromAsciiError<B>>
    where
        B: Into<Vec<u8>> + AsRef<[u8]>,
    {
        AsciiString::from_ascii(bytes).map(HeaderField)
    }

    /// Get `[HeaderField]` as `&AsciiStr`
    #[must_use]
    pub fn as_str(&self) -> &AsciiStr {
        &self.0
    }

    /// Checks `[HeaderField]` for equivalence ignoring case of letters
    #[must_use]
    pub fn equiv(&self, other: &'static str) -> bool {
        other.eq_ignore_ascii_case(self.as_str().as_str())
    }
}

impl FromStr for HeaderField {
    type Err = HeaderError;

    fn from_str(s: &str) -> Result<HeaderField, HeaderError> {
        if s.contains(char::is_whitespace) {
            Err(HeaderError)
        } else {
            AsciiString::from_ascii(s)
                .map(HeaderField)
                .map_err(|_| HeaderError)
        }
    }
}

impl TryFrom<&AsciiStr> for HeaderField {
    type Error = ();

    fn try_from(asciistr: &AsciiStr) -> Result<Self, Self::Error> {
        for asciichar in asciistr {
            if *asciichar == AsciiChar::Space {
                return Err(());
            }
        }
        Ok(Self(asciistr.to_owned()))
    }
}

impl std::fmt::Display for HeaderField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_str(self.0.as_str())
    }
}

impl PartialEq for HeaderField {
    fn eq(&self, other: &HeaderField) -> bool {
        let self_str: &str = self.as_str().as_ref();
        let other_str = other.as_str().as_ref();
        self_str.eq_ignore_ascii_case(other_str)
    }
}

impl std::hash::Hash for HeaderField {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}

// Needs to be lower-case!!!
pub(crate) const HEADER_FORBIDDEN: &[&str] =
    &["connection", "trailer", "transfer-encoding", "upgrade"];

/// Header was not added
#[derive(Debug)]
pub struct HeaderError;

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

    use ascii::AsAsciiStr;
    use httpdate::HttpDate;

    use super::{Header, HEADER_FORBIDDEN};

    #[test]
    fn test_parse_header() {
        let header: Header = "Content-Type: text/html".parse().unwrap();

        assert!(header.field.equiv("content-type"));
        assert!(header.value.as_str() == "text/html");

        assert!("hello world".parse::<Header>().is_err());
    }

    #[test]
    fn test_header_try_from_ascii() {
        let header: Header =
            Header::try_from("Content-Type: text/html".as_ascii_str().unwrap()).unwrap();

        assert!(header.field.equiv("content-type"));
        assert!(header.value.as_str() == "text/html");
    }

    #[test]
    fn formats_date_correctly() {
        let http_date = HttpDate::from(SystemTime::UNIX_EPOCH + Duration::from_secs(420_895_020));

        assert_eq!(http_date.to_string(), "Wed, 04 May 1983 11:17:00 GMT");
    }

    #[test]
    fn test_parse_header_with_doublecolon() {
        let header: Header = "Time: 20: 34".parse().unwrap();

        assert!(header.field.equiv("time"));
        assert!(header.value.as_str() == "20: 34");
    }

    #[test]
    fn test_header_with_doublecolon_try_from_ascii() {
        let header: Header = Header::try_from("Time: 20: 34".as_ascii_str().unwrap()).unwrap();

        assert!(header.field.equiv("time"));
        assert!(header.value.as_str() == "20: 34");
    }

    // This tests resistance to RUSTSEC-2020-0031: "HTTP Request smuggling
    // through malformed Transfer Encoding headers"
    // (https://rustsec.org/advisories/RUSTSEC-2020-0031.html).
    #[test]
    fn test_strict_headers() {
        assert!("Transfer-Encoding : chunked".parse::<Header>().is_err());
        assert!(" Transfer-Encoding: chunked".parse::<Header>().is_err());
        assert!("Transfer Encoding: chunked".parse::<Header>().is_err());
        assert!(" Transfer\tEncoding : chunked".parse::<Header>().is_err());
        assert!("Transfer-Encoding: chunked".parse::<Header>().is_ok());
        assert!("Transfer-Encoding: chunked ".parse::<Header>().is_ok());
        assert!("Transfer-Encoding:   chunked ".parse::<Header>().is_ok());
    }

    #[test]
    fn test_strict_headers_try_from_ascii() {
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

    #[test]
    fn test_header_forbidden_lc() {
        for h in HEADER_FORBIDDEN {
            assert_eq!(h, &h.to_lowercase());
        }
    }
}
