use ascii::{AsciiStr, AsciiString};
use std::convert::TryFrom;
use std::fmt::{self, Formatter};
use std::str::FromStr;

/// HTTP protocol Connection header values
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionHeader {
    /// Connection: Close
    Close,
    /// Connection: Keep-Alive
    KeepAlive,
    /// Connection: Upgrade
    Upgrade,
}

impl std::fmt::Display for ConnectionHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.into())
    }
}

impl From<ConnectionHeader> for AsciiString {
    fn from(value: ConnectionHeader) -> Self {
        AsciiString::from_str(value.into()).unwrap()
    }
}

impl From<ConnectionHeader> for &'static str {
    fn from(value: ConnectionHeader) -> Self {
        match value {
            ConnectionHeader::Close => "Close",
            ConnectionHeader::KeepAlive => "Keep-Alive",
            ConnectionHeader::Upgrade => "Upgrade",
        }
    }
}

impl From<&ConnectionHeader> for &'static str {
    fn from(value: &ConnectionHeader) -> Self {
        (*value).into()
    }
}

impl From<ConnectionHeader> for super::Header {
    fn from(value: ConnectionHeader) -> Self {
        super::Header {
            field: "Connection".parse().unwrap(),
            value: value.into(),
        }
    }
}

impl TryFrom<&super::Header> for ConnectionHeader {
    type Error = ();

    fn try_from(header: &super::Header) -> Result<Self, Self::Error> {
        Self::try_from(header.value.as_str())
    }
}

impl TryFrom<super::Header> for ConnectionHeader {
    type Error = ();

    fn try_from(header: super::Header) -> Result<Self, Self::Error> {
        Self::try_from(header.value.as_str())
    }
}

impl TryFrom<&str> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let lowercase = value.to_ascii_lowercase();
        let lowercase = lowercase.as_str();
        Ok(match lowercase {
            "close" => Self::Close,
            "keep-alive" => Self::KeepAlive,
            "upgrade" => Self::Upgrade,
            _ => return Err(()),
        })
    }
}

impl TryFrom<&AsciiStr> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &AsciiStr) -> Result<Self, Self::Error> {
        ConnectionHeader::try_from(value.as_str())
    }
}
