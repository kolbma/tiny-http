//! The Connection general header controls whether the network connection stays open after
//! the current transaction finishes.  
//! If the value sent is keep-alive, the connection is persistent and not closed,
//! allowing for subsequent requests to the same server to be done.

use ascii::{AsciiStr, AsciiString};
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::fmt::{self, Formatter};

use crate::HeaderField;

const CONNECTION_HEADER_SORTED: &[ConnectionValue] = &[
    ConnectionValue::Close,
    ConnectionValue::Upgrade,
    ConnectionValue::KeepAlive,
];

lazy_static! {
    static ref CONNECTION_HEADER_ITER: std::iter::Enumerate<std::slice::Iter<'static, ConnectionValue>> =
        CONNECTION_HEADER_SORTED.iter().enumerate();
    static ref CONNECTION_HEADER_SORTED_LAST_IDX: usize = CONNECTION_HEADER_SORTED.len() - 1;
}

/// Http protocol Connection header line content
#[derive(Debug)]
pub struct ConnectionHeader {
    inner: HashSet<ConnectionValue>,
}

impl ConnectionHeader {
    /// [`ConnectionHeaderIterator`] priorizes [`ConnectionValue`]
    #[must_use]
    #[allow(clippy::iter_without_into_iter)]
    pub fn iter(&self) -> ConnectionHeaderIterator<'_> {
        // priorized
        for (idx, v) in CONNECTION_HEADER_ITER.clone() {
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
        let cur = self.idx.take();
        if let Some(cur) = cur {
            let mut next = cur + 1;
            while next <= *CONNECTION_HEADER_SORTED_LAST_IDX {
                if self.header.contains(&CONNECTION_HEADER_SORTED[next]) {
                    self.idx = Some(next);
                    break;
                }
                next += 1;
            }
            Some(CONNECTION_HEADER_SORTED[cur])
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

impl From<ConnectionValue> for &'static AsciiStr {
    fn from(value: ConnectionValue) -> Self {
        match value {
            ConnectionValue::Close => AsciiStr::from_ascii(b"close"),
            ConnectionValue::KeepAlive => AsciiStr::from_ascii(b"keep-alive"),
            ConnectionValue::Upgrade => AsciiStr::from_ascii(b"upgrade"),
        }
        .unwrap()
    }
}

impl From<ConnectionValue> for AsciiString {
    fn from(value: ConnectionValue) -> Self {
        <&AsciiStr>::from(value).to_ascii_string()
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
            field: HeaderField::from_bytes(&b"Connection"[..]).unwrap(),
            value: value.into(),
        }
    }
}

impl From<ConnectionHeader> for super::Header {
    fn from(value: ConnectionHeader) -> Self {
        super::Header {
            field: HeaderField::from_bytes(&b"Connection"[..]).unwrap(),
            value: <AsciiString as std::str::FromStr>::from_str(
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
    use std::time::Instant;

    use crate::Header;

    use super::*;

    #[ignore = "seems to be no difference and all vary"]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn connection_header_conversion_bench_test() {
        #[allow(clippy::inline_always)]
        #[inline(always)]
        fn header_with_ascii_string(value: &ConnectionHeader) -> Header {
            let mut ascii = AsciiString::new();

            for v in value.iter() {
                let s = <&AsciiStr>::from(v);
                ascii.push_str(s);
                ascii.push(ascii::AsciiChar::Comma);
                ascii.push(ascii::AsciiChar::Space);
            }
            if !ascii.is_empty() {
                ascii.truncate(ascii.len() - 2);
            }

            Header {
                field: HeaderField::from_bytes(&b"Connection"[..]).unwrap(),
                value: ascii,
            }
        }

        #[allow(clippy::inline_always)]
        #[inline(always)]
        fn header_with_ascii_string_2(value: &ConnectionHeader) -> Header {
            let mut ascii = AsciiString::new();
            let mut it = value.iter();
            let mut v = it.next();

            loop {
                ascii.push_str(<&AsciiStr>::from(v.unwrap()));

                v = it.next();

                if v.is_none() {
                    break;
                }

                ascii.push(ascii::AsciiChar::Comma);
                ascii.push(ascii::AsciiChar::Space);
            }

            Header {
                field: HeaderField::from_bytes(&b"Connection"[..]).unwrap(),
                value: ascii,
            }
        }

        #[allow(clippy::inline_always)]
        #[inline(always)]
        fn header_with_iter_join(value: &ConnectionHeader) -> Header {
            Header {
                field: HeaderField::from_bytes(&b"Connection"[..]).unwrap(),
                value: <AsciiString as std::str::FromStr>::from_str(
                    &value
                        .iter()
                        .map(<&str>::from)
                        .collect::<Vec<&str>>()
                        .join(", "),
                )
                .unwrap(),
            }
        }

        let headers = [
            ConnectionHeader {
                inner: HashSet::from([
                    ConnectionValue::Upgrade,
                    ConnectionValue::Close,
                    ConnectionValue::KeepAlive,
                ]),
            },
            ConnectionHeader {
                inner: HashSet::from([ConnectionValue::Close]),
            },
            ConnectionHeader {
                inner: HashSet::from([ConnectionValue::KeepAlive, ConnectionValue::Close]),
            },
        ];

        for _ in 0..50 {
            let rounds = 50_000;

            let now = Instant::now();

            for _ in 0..rounds {
                for cheader in &headers {
                    let header = header_with_ascii_string(cheader);
                    assert!(!header.value.is_empty());
                }
            }

            let elaps_ascii_string = now.elapsed();

            let now = Instant::now();

            for _ in 0..rounds {
                for cheader in &headers {
                    let header = header_with_ascii_string_2(cheader);
                    assert!(!header.value.is_empty());
                }
            }

            let elaps_ascii_string_2 = now.elapsed();

            let now = Instant::now();

            for _ in 0..rounds {
                for cheader in &headers {
                    let header = header_with_iter_join(cheader);
                    assert!(!header.value.is_empty());
                }
            }

            let elaps_iter_join = now.elapsed();

            assert!(
                elaps_ascii_string_2 <= elaps_ascii_string,
                "elaps_ascii_string_2: {} elaps_ascii_string: {}",
                elaps_ascii_string_2.as_micros(),
                elaps_ascii_string.as_micros()
            );

            assert!(
                elaps_ascii_string_2 >= elaps_iter_join,
                "elaps_ascii_string_2: {} elaps_iter_join: {}",
                elaps_ascii_string_2.as_micros(),
                elaps_iter_join.as_micros()
            );

            assert!(
                elaps_ascii_string >= elaps_iter_join,
                "elaps_ascii_string: {} elaps_iter_join: {}",
                elaps_ascii_string.as_micros(),
                elaps_iter_join.as_micros()
            );
        }
    }

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
