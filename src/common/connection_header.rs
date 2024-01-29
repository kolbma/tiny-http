use ascii::{AsciiStr, AsciiString};
use std::collections::HashSet;
use std::convert::TryFrom;
use std::fmt::{self, Formatter};
use std::str::FromStr;

/// Http protocol Connection header line content
#[derive(Debug)]
pub struct ConnectionHeader {
    pub(crate) inner: HashSet<ConnectionValue>,
}

impl ConnectionHeader {
    /// [`ConnectionHeaderIterator`] priorizes [`ConnectionValue`]
    #[must_use]
    #[allow(clippy::iter_without_into_iter)]
    pub fn iter(&self) -> ConnectionHeaderIterator<'_> {
        // priorized
        for (idx, v) in [
            ConnectionValue::Close,
            ConnectionValue::Upgrade,
            ConnectionValue::KeepAlive,
        ]
        .iter()
        .enumerate()
        {
            if self.inner.contains(v) {
                return ConnectionHeaderIterator {
                    header: self,
                    idx: Some(idx),
                };
            }
        }

        ConnectionHeaderIterator {
            header: self,
            idx: None,
        }
    }
}

impl TryFrom<&str> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut inner = HashSet::with_capacity(1);
        let values = value.split(',');
        for value in values {
            let value = value.trim_start();
            let _ = inner.insert(ConnectionValue::try_from(value)?);
        }
        Ok(ConnectionHeader { inner })
    }
}

impl TryFrom<&AsciiStr> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &AsciiStr) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl TryFrom<&AsciiString> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &AsciiString) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl std::ops::Deref for ConnectionHeader {
    type Target = HashSet<ConnectionValue>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for ConnectionHeader {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// [`ConnectionHeader`] is `eq` if it contains single [`ConnectionValue`] of `other`
impl PartialEq for ConnectionHeader {
    /// [`ConnectionHeader`] is `eq` if it contains single [`ConnectionValue`] of `other`
    fn eq(&self, other: &Self) -> bool {
        for o in &other.inner {
            if self.inner.contains(o) {
                return true;
            }
        }
        false
    }
}

/// Iterator over priorized [`ConnectionValue`] of [`ConnectionHeader`]
#[derive(Debug)]
pub struct ConnectionHeaderIterator<'a> {
    header: &'a ConnectionHeader,
    idx: Option<usize>,
}

impl Iterator for ConnectionHeaderIterator<'_> {
    type Item = ConnectionValue;

    /// Get the next important [`ConnectionValue`]
    fn next(&mut self) -> Option<Self::Item> {
        const VALUES: [ConnectionValue; 3] = [
            ConnectionValue::Close,
            ConnectionValue::Upgrade,
            ConnectionValue::KeepAlive,
        ];
        const VALUES_MAX_IDX: usize = VALUES.len() - 1;
        let cur = self.idx.take();
        if let Some(cur) = cur {
            let mut next = cur + 1;
            while next <= VALUES_MAX_IDX {
                if self.header.contains(&VALUES[next]) {
                    self.idx = Some(next);
                    break;
                }
                next += 1;
            }
            Some(VALUES[cur])
        } else {
            None
        }
    }
}

/// HTTP protocol Connection header values
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConnectionValue {
    /// Connection: close
    Close,
    /// Connection: keep-alive
    KeepAlive,
    /// Connection: upgrade
    Upgrade,
}

impl std::fmt::Display for ConnectionValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.into())
    }
}

impl From<ConnectionValue> for AsciiString {
    fn from(value: ConnectionValue) -> Self {
        AsciiString::from_str(value.into()).unwrap()
    }
}

impl From<ConnectionValue> for &'static str {
    fn from(value: ConnectionValue) -> Self {
        match value {
            ConnectionValue::Close => "close",
            ConnectionValue::KeepAlive => "keep-alive",
            ConnectionValue::Upgrade => "upgrade",
        }
    }
}

impl From<&ConnectionValue> for &'static str {
    fn from(value: &ConnectionValue) -> Self {
        (*value).into()
    }
}

impl From<ConnectionValue> for super::Header {
    fn from(value: ConnectionValue) -> Self {
        super::Header {
            field: "Connection".parse().unwrap(),
            value: value.into(),
        }
    }
}

impl From<ConnectionHeader> for super::Header {
    fn from(value: ConnectionHeader) -> Self {
        super::Header {
            field: "Connection".parse().unwrap(),
            value: AsciiString::from_str(
                &value
                    .iter()
                    .map(<&str>::from)
                    .collect::<Vec<&str>>()
                    .join(", "),
            )
            .unwrap(),
        }
    }
}

impl TryFrom<&super::Header> for ConnectionValue {
    type Error = ();

    fn try_from(header: &super::Header) -> Result<Self, Self::Error> {
        Self::try_from(header.value.as_str())
    }
}

impl TryFrom<super::Header> for ConnectionValue {
    type Error = ();

    fn try_from(header: super::Header) -> Result<Self, Self::Error> {
        Self::try_from(header.value.as_str())
    }
}

impl TryFrom<&str> for ConnectionValue {
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

impl TryFrom<&AsciiStr> for ConnectionValue {
    type Error = ();

    fn try_from(value: &AsciiStr) -> Result<Self, Self::Error> {
        ConnectionValue::try_from(value.as_str())
    }
}

impl TryFrom<&AsciiString> for ConnectionValue {
    type Error = ();

    fn try_from(value: &AsciiString) -> Result<Self, Self::Error> {
        ConnectionValue::try_from(value.as_str())
    }
}

impl From<ConnectionValue> for ConnectionHeader {
    fn from(value: ConnectionValue) -> Self {
        Self {
            inner: HashSet::from([value]),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Header;

    use super::*;

    #[test]
    fn connection_header_to_header_test() {
        let ch = ConnectionHeader {
            inner: HashSet::from([
                ConnectionValue::KeepAlive,
                ConnectionValue::Upgrade,
                ConnectionValue::Close,
            ]),
        };

        let h: Header = ch.into();

        let hs = h.to_string();
        assert_eq!(&hs, "Connection: close, upgrade, keep-alive"); // HTTP protocol no sense, but don't check it for performance

        let ch = ConnectionHeader {
            inner: HashSet::from([ConnectionValue::Close]),
        };

        let h: Header = ch.into();

        let hs = h.to_string();
        assert_eq!(&hs, "Connection: close");

        let ch = ConnectionHeader {
            inner: HashSet::from([ConnectionValue::KeepAlive, ConnectionValue::Upgrade]),
        };

        let h: Header = ch.into();

        let hs = h.to_string();
        assert_eq!(&hs, "Connection: upgrade, keep-alive");

        let ch = ConnectionHeader {
            inner: HashSet::new(),
        };

        let h: Header = ch.into();

        let hs = h.to_string();
        assert_eq!(&hs, "Connection: ");
    }

    #[test]
    fn connection_header_iterator_test() {
        let result = Vec::from([
            ConnectionValue::Close,
            ConnectionValue::Upgrade,
            ConnectionValue::KeepAlive,
        ]);

        let h = ConnectionHeader {
            inner: HashSet::new(),
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert!(v.is_empty());

        let h = ConnectionHeader {
            inner: HashSet::from([
                ConnectionValue::KeepAlive,
                ConnectionValue::Upgrade,
                ConnectionValue::Close,
            ]),
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert_eq!(v, result);

        let h = ConnectionHeader {
            inner: HashSet::from([ConnectionValue::KeepAlive, ConnectionValue::Close]),
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert_eq!(v, [ConnectionValue::Close, ConnectionValue::KeepAlive,]);

        let h = ConnectionHeader {
            inner: HashSet::from([ConnectionValue::Upgrade]),
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert_eq!(v, result[1..2]);

        let h = ConnectionHeader {
            inner: HashSet::from([
                ConnectionValue::Close,
                ConnectionValue::Upgrade,
                ConnectionValue::KeepAlive,
            ]),
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert_eq!(v, result);
    }
}
