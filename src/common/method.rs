use std::{fmt, str::FromStr};

use ascii::{AsciiStr, AsciiString};

/// HTTP request methods
///
/// As per [RFC 7231](https://tools.ietf.org/html/rfc7231#section-4.1) and
/// [RFC 5789](https://tools.ietf.org/html/rfc5789)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`
    Get,
    /// `HEAD`
    Head,
    /// `POST`
    Post,
    /// `PUT`
    Put,
    /// `DELETE`
    Delete,
    /// `CONNECT`
    Connect,
    /// `OPTIONS`
    Options,
    /// `TRACE`
    Trace,
    /// `PATCH`
    Patch,
    /// Request methods not standardized by the IETF
    NonStandard(Option<AsciiString>),
}

impl Method {
    /// enum [Method] names as `&str`
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Connect => "CONNECT",
            Method::Options => "OPTIONS",
            Method::Trace => "TRACE",
            Method::Patch => "PATCH",
            Method::NonStandard(s) => s.as_ref().map_or("None", |s| s.as_str()),
        }
    }
}

impl FromStr for Method {
    type Err = ();

    fn from_str(s: &str) -> Result<Method, ()> {
        Ok(Method::from(s.as_bytes()))
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.write_str(self.as_str())
    }
}

impl From<&AsciiStr> for Method {
    fn from(s: &AsciiStr) -> Self {
        Self::from(s.as_bytes())
    }
}

impl From<&[u8]> for Method {
    fn from(b: &[u8]) -> Self {
        match b {
            b"GET" => Method::Get,
            b"HEAD" => Method::Head,
            b"POST" => Method::Post,
            b"PUT" => Method::Put,
            b"DELETE" => Method::Delete,
            b"CONNECT" => Method::Connect,
            b"OPTIONS" => Method::Options,
            b"TRACE" => Method::Trace,
            b"PATCH" => Method::Patch,
            _ => Method::NonStandard(AsciiString::from_ascii(b).ok()),
        }
    }
}
