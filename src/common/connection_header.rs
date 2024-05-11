//! The Http Connection header

use ascii::{AsciiStr, AsciiString};
use lazy_static::lazy_static;
use std::convert::TryFrom;
use std::fmt::Formatter;

use super::header::HeaderFieldValue;

/// Http protocol Connection header controls whether the network connection stays open
/// after the current transaction finishes.
///
/// If the value sent is keep-alive, the connection is persistent and not closed,
/// allowing for subsequent requests to the same server to be done.
#[derive(Debug)]
pub struct ConnectionHeader {
    inner: u8,
}

impl ConnectionHeader {
    /// [`ConnectionHeaderIterator`] priorizes [`ConnectionValue`]
    #[must_use]
    #[allow(clippy::iter_without_into_iter)]
    pub fn iter(&self) -> ConnectionHeaderIterator<'_> {
        // priorized
        ConnectionHeaderIterator {
            header: self,
            idx: if self.inner & 1 == 1 {
                1
            } else if self.inner & 2 == 2 {
                2
            } else if self.inner & 4 == 4 {
                4
            } else {
                0
            },
        }
    }
}

impl TryFrom<&[u8]> for ConnectionHeader {
    type Error = ();

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut inner = 0_u8;
        let mut pos = 0_usize;
        let mut space_non_pos = 0_usize;
        let mut start = true;
        let mut start_pos = 0_usize;

        #[allow(clippy::explicit_counter_loop)]
        for b in bytes {
            if start && *b == b' ' {
                start_pos += 1;
            } else if *b == b',' {
                let value = ConnectionValue::try_from(&bytes[start_pos..=space_non_pos])?;
                inner |= value as u8;
                start = true;
                start_pos = pos + 1;
            } else if start {
                start = false;
            }

            if !start && *b != b' ' {
                space_non_pos = pos;
            }

            pos += 1;
        }

        // last after last comma
        if start_pos < space_non_pos {
            let value = ConnectionValue::try_from(&bytes[start_pos..=space_non_pos])?;
            inner |= value as u8;
        }

        Ok(ConnectionHeader { inner })
    }
}

impl TryFrom<&HeaderFieldValue> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &HeaderFieldValue) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&str> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&AsciiStr> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &AsciiStr) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&AsciiString> for ConnectionHeader {
    type Error = ();

    fn try_from(value: &AsciiString) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl std::ops::Deref for ConnectionHeader {
    type Target = Vec<ConnectionValue>;

    fn deref(&self) -> &Self::Target {
        &CONNECTION_VALUE_VARIANTS[self.inner as usize]
    }
}

/// [`ConnectionHeader`] is `eq` if it contains single [`ConnectionValue`] of `other`
impl PartialEq for ConnectionHeader {
    /// [`ConnectionHeader`] is `eq` if it contains single [`ConnectionValue`] of `other`
    fn eq(&self, other: &Self) -> bool {
        if self.inner == other.inner {
            return true;
        }
        for n in [1u8, 2, 4] {
            if self.inner & n != 0 && self.inner & n == other.inner & n {
                return true;
            }
        }
        false
    }
}

/// [`ConnectionHeader`] is `eq` if it contains [`ConnectionValue`]
impl PartialEq<ConnectionValue> for ConnectionHeader {
    /// [`ConnectionHeader`] is `eq` if it contains single [`ConnectionValue`] of `other`
    fn eq(&self, other: &ConnectionValue) -> bool {
        self.inner & (*other as u8) == (*other as u8)
    }
}

/// Iterator over priorized [`ConnectionValue`] of [`ConnectionHeader`]
#[derive(Debug)]
pub struct ConnectionHeaderIterator<'a> {
    header: &'a ConnectionHeader,
    idx: u8,
}

impl Iterator for ConnectionHeaderIterator<'_> {
    type Item = ConnectionValue;

    /// Get the next important [`ConnectionValue`]
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx != 0 {
            let cur = self.idx;
            let mut next = cur << 1;
            while next <= CONNECTION_VALUE_HIGH_BIT {
                if self.header.inner & next == next {
                    self.idx = next;
                    break;
                }
                next <<= 1;
            }
            if next > CONNECTION_VALUE_HIGH_BIT {
                self.idx = 0;
            }
            CONNECTION_VALUE_BIT_MAP[cur as usize]
        } else {
            None
        }
    }
}

const CONNECTION_VALUE_HIGH_BIT: u8 = 4;

/// HTTP protocol Connection header values
// Attention: Using priorized values for bit masks
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConnectionValue {
    /// Connection: close
    Close = 1,
    /// Connection: keep-alive
    KeepAlive = 4,
    /// Connection: upgrade
    Upgrade = 2,
}

const CONNECTION_VALUE_BIT_MAP: [Option<ConnectionValue>; 5] = [
    None,
    Some(ConnectionValue::Close),
    Some(ConnectionValue::Upgrade),
    None,
    Some(ConnectionValue::KeepAlive),
];

lazy_static! {
    static ref CONNECTION_VALUE_VARIANTS: [Vec<ConnectionValue>; 8] = [
        Vec::new(),
        Vec::from([ConnectionValue::Close]),
        Vec::from([ConnectionValue::Upgrade]),
        Vec::from([ConnectionValue::Close, ConnectionValue::Upgrade]),
        Vec::from([ConnectionValue::KeepAlive]),
        Vec::from([ConnectionValue::Close, ConnectionValue::KeepAlive]),
        Vec::from([ConnectionValue::Upgrade, ConnectionValue::KeepAlive]),
        Vec::from([
            ConnectionValue::Close,
            ConnectionValue::Upgrade,
            ConnectionValue::KeepAlive,
        ]),
    ];
}

impl std::fmt::Display for ConnectionValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.into())
    }
}

impl From<ConnectionValue> for &'static [u8] {
    fn from(value: ConnectionValue) -> Self {
        match value {
            ConnectionValue::Close => b"close",
            ConnectionValue::KeepAlive => b"keep-alive",
            ConnectionValue::Upgrade => b"upgrade",
        }
    }
}

impl From<ConnectionValue> for &'static AsciiStr {
    fn from(value: ConnectionValue) -> Self {
        AsciiStr::from_ascii(<&[u8]>::from(value)).unwrap()
    }
}

impl From<ConnectionValue> for AsciiString {
    fn from(value: ConnectionValue) -> Self {
        <&AsciiStr>::from(value).to_ascii_string()
    }
}

impl From<ConnectionValue> for &'static str {
    fn from(value: ConnectionValue) -> Self {
        std::str::from_utf8(<&[u8]>::from(value)).unwrap()
    }
}

impl From<&ConnectionValue> for &'static str {
    fn from(value: &ConnectionValue) -> Self {
        (*value).into()
    }
}

impl From<ConnectionValue> for super::Header {
    fn from(value: ConnectionValue) -> Self {
        let mut header = super::static_header::CONNECTION_HEADER.clone();
        header.value = HeaderFieldValue::try_from(<&[u8]>::from(value)).unwrap();
        header
    }
}

impl From<ConnectionHeader> for super::Header {
    fn from(value: ConnectionHeader) -> Self {
        let mut header = super::static_header::CONNECTION_HEADER.clone();
        header.value = HeaderFieldValue::try_from(
            value
                .iter()
                .map(<&str>::from)
                .collect::<Vec<&str>>()
                .join(", ")
                .as_bytes(),
        )
        .unwrap();
        header
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

impl TryFrom<&[u8]> for ConnectionValue {
    type Error = ();

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let bytes_len = bytes.len();
        if bytes_len > 30 {
            return Err(());
        }

        // lowercase convert
        let mut lc_copy = [0u8; 30];
        let lc_copy = &mut lc_copy[..bytes_len];
        lc_copy.copy_from_slice(bytes);

        for b in &mut *lc_copy {
            if *b >= 65 && *b <= 90 {
                *b += 32;
            }
        }

        let lc_copy: &[u8] = lc_copy;

        Ok(match lc_copy {
            b"close" => Self::Close,
            b"keep-alive" => Self::KeepAlive,
            b"upgrade" => Self::Upgrade,
            _ => return Err(()),
        })
    }
}

impl TryFrom<&str> for ConnectionValue {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&AsciiStr> for ConnectionValue {
    type Error = ();

    fn try_from(value: &AsciiStr) -> Result<Self, Self::Error> {
        ConnectionValue::try_from(value.as_bytes())
    }
}

impl TryFrom<&AsciiString> for ConnectionValue {
    type Error = ();

    fn try_from(value: &AsciiString) -> Result<Self, Self::Error> {
        ConnectionValue::try_from(value.as_bytes())
    }
}

impl From<ConnectionValue> for ConnectionHeader {
    fn from(value: ConnectionValue) -> Self {
        Self { inner: value as u8 }
    }
}

#[cfg(test)]
mod tests {
    use crate::Header;

    use super::*;

    #[test]
    fn connection_header_cmp_eq() {
        let ch_1 = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8
                | ConnectionValue::Upgrade as u8
                | ConnectionValue::Close as u8,
        };

        let ch_2 = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8
                | ConnectionValue::Upgrade as u8
                | ConnectionValue::Close as u8,
        };

        assert_eq!(ch_1, ch_2);

        let ch_2 = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8,
        };

        assert_eq!(ch_1, ch_2);

        let ch_1 = ConnectionHeader {
            inner: ConnectionValue::Close as u8,
        };

        let ch_2 = ConnectionHeader {
            inner: ConnectionValue::Upgrade as u8,
        };

        assert_ne!(ch_1, ch_2);
    }

    #[test]
    fn connection_header_value_cmp_eq() {
        let ch = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8
                | ConnectionValue::Upgrade as u8
                | ConnectionValue::Close as u8,
        };

        assert_eq!(ch, ConnectionValue::KeepAlive);
        assert_eq!(ch, ConnectionValue::Upgrade);
        assert_eq!(ch, ConnectionValue::Close);

        let ch = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8,
        };

        assert_eq!(ch, ConnectionValue::KeepAlive);
        assert_ne!(ch, ConnectionValue::Upgrade);
        assert_ne!(ch, ConnectionValue::Close);

        let ch = ConnectionHeader {
            inner: ConnectionValue::Close as u8,
        };

        assert_ne!(ch, ConnectionValue::KeepAlive);
        assert_ne!(ch, ConnectionValue::Upgrade);
        assert_eq!(ch, ConnectionValue::Close);

        let ch = ConnectionHeader {
            inner: ConnectionValue::Upgrade as u8,
        };

        assert_ne!(ch, ConnectionValue::KeepAlive);
        assert_eq!(ch, ConnectionValue::Upgrade);
        assert_ne!(ch, ConnectionValue::Close);
    }

    #[test]
    fn connection_header_to_header_test() {
        let ch = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8
                | ConnectionValue::Upgrade as u8
                | ConnectionValue::Close as u8,
        };

        let h: Header = ch.into();

        let hs = h.to_string();
        assert_eq!(&hs, "Connection: close, upgrade, keep-alive"); // HTTP protocol no sense, but don't check it for performance

        let ch = ConnectionHeader {
            inner: ConnectionValue::Close as u8,
        };

        let h: Header = ch.into();

        let hs = h.to_string();
        assert_eq!(&hs, "Connection: close");

        let ch = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8 | ConnectionValue::Upgrade as u8,
        };

        let h: Header = ch.into();

        let hs = h.to_string();
        assert_eq!(&hs, "Connection: upgrade, keep-alive");

        let ch = ConnectionHeader { inner: 0 };

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

        let h = ConnectionHeader { inner: 0 };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert!(v.is_empty());

        let h = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8
                | ConnectionValue::Upgrade as u8
                | ConnectionValue::Close as u8,
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert_eq!(v, result);

        let h = ConnectionHeader {
            inner: ConnectionValue::KeepAlive as u8 | ConnectionValue::Close as u8,
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert_eq!(v, [ConnectionValue::Close, ConnectionValue::KeepAlive,]);

        let h = ConnectionHeader {
            inner: ConnectionValue::Upgrade as u8,
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert_eq!(v, result[1..2]);

        let h = ConnectionHeader {
            inner: ConnectionValue::Close as u8
                | ConnectionValue::Upgrade as u8
                | ConnectionValue::KeepAlive as u8,
        };

        let mut v = Vec::new();
        for cv in h.iter() {
            v.push(cv);
        }
        assert_eq!(v, result);
    }
}
