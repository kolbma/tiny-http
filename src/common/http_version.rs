use std::{convert::TryFrom, fmt};

use ascii::AsciiStr;

const HTTP_VERSION_HEADER: &[&str] = &["HTTP/0.9", "HTTP/1.0", "HTTP/1.1", "HTTP/2", "HTTP/3"];

/// HTTP/{version} Request Version (HTTP/1.0 or HTTP/1.1)
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HttpVersion {
    /// HTTP/0.9
    Version0_9,
    /// HTTP/1.0
    Version1_0,
    /// HTTP/1.1
    Version1_1,
    /// HTTP/2
    Version2,
    /// HTTP/3
    Version3,
}

impl HttpVersion {
    /// Http version in header format (e.g. HTTP/1.1)
    #[must_use]
    #[inline]
    pub const fn header(&self) -> &'static str {
        HTTP_VERSION_HEADER[(*self) as usize]
    }
}

impl std::fmt::Display for HttpVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let v = match self {
            Self::Version1_1 => "1.1",
            Self::Version3 => "3",
            Self::Version2 => "2",
            Self::Version1_0 => "1.0",
            Self::Version0_9 => "0.9",
        };
        f.write_str(v)
    }
}

impl TryFrom<(u8, u8)> for HttpVersion {
    type Error = HttpVersionError;

    fn try_from(value: (u8, u8)) -> Result<Self, Self::Error> {
        match value {
            // ordered for most occurrence
            (1, 1) => Ok(Self::Version1_1),
            (3, 0) => Ok(Self::Version3),
            (2, 0) => Ok(Self::Version2),
            (1, 0) => Ok(Self::Version1_0),
            (0, 9) => Ok(Self::Version0_9),
            _ => Err(HttpVersionError(value.0, value.1)),
        }
    }
}

impl TryFrom<&str> for HttpVersion {
    type Error = HttpVersionError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&AsciiStr> for HttpVersion {
    type Error = HttpVersionError;

    fn try_from(value: &AsciiStr) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&[u8]> for HttpVersion {
    type Error = HttpVersionError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let len = value.len();
        let (value, len) = if (len == 8 || len == 6) && &value[0..5] == b"HTTP/" {
            // "HTTP/1.0"
            let v = &value[5..];
            (v, v.len())
        } else {
            (value, len)
        };

        // "1.0"
        if len == 3 && value[1] == b'.' {
            let major = value[0];
            let minor = value[2];
            let range = b'0'..=b'9';
            if range.contains(&major) && range.contains(&minor) {
                return Self::try_from((major - 48, minor - 48));
            }
        } else if len == 1 {
            let major = value[0];
            let range = b'2'..=b'9';
            if range.contains(&major) {
                return Self::try_from((major - 48, 0));
            }
        }

        Err(HttpVersionError(0, 0))
    }
}

/// Error for unsupported or unparseable [`HttpVersion`]
#[derive(Debug)]
pub struct HttpVersionError(u8, u8);

impl std::error::Error for HttpVersionError {}

impl std::fmt::Display for HttpVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 2 {
            f.write_fmt(format_args!("unsupported HTTP/{}.{}", self.0, self.1))
        } else {
            f.write_fmt(format_args!("unsupported HTTP/{}", self.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use ascii::AsciiStr;

    use super::HttpVersion;

    #[test]
    fn test_parse_http_version() {
        let table = [
            ("HTTP/0.9", Some(HttpVersion::Version0_9)),
            ("HTTP/1.0", Some(HttpVersion::Version1_0)),
            ("HTTP/1.1", Some(HttpVersion::Version1_1)),
            ("HTTP/2", Some(HttpVersion::Version2)),
            ("HTTP/2.0", Some(HttpVersion::Version2)),
            ("HTTP/3", Some(HttpVersion::Version3)),
            ("HTTP/3.0", Some(HttpVersion::Version3)),
            ("0.9", Some(HttpVersion::Version0_9)),
            ("1.0", Some(HttpVersion::Version1_0)),
            ("1.1", Some(HttpVersion::Version1_1)),
            ("2", Some(HttpVersion::Version2)),
            ("2.0", Some(HttpVersion::Version2)),
            ("3", Some(HttpVersion::Version3)),
            ("3.0", Some(HttpVersion::Version3)),
            ("HTTP/0.8", None),
            ("HTTP/1.2", None),
            ("HTTP/2.1", None),
            ("HTTP1.1", None),
            ("1", None),
            ("HTTP 1.1", None),
            (" HTTP1.1", None),
            ("111", None),
        ];

        for entry in table {
            let v = HttpVersion::try_from(AsciiStr::from_ascii(entry.0).unwrap());
            if let Some(src_v) = entry.1 {
                assert!(v.is_ok(), "[{}] error: {}", src_v, v.unwrap_err());
                assert_eq!(v.unwrap(), src_v);
            } else {
                assert!(v.is_err());
            }
        }
    }
}
