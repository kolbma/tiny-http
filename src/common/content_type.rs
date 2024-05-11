#![cfg(feature = "content-type")]

use ascii::{AsciiStr, AsciiString};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::OnceLock;

macro_rules! create_content_types {
    ($(($ct:ident, $text:expr)),+) => {
        #[doc = "HTTP protocol Content-Type header values"]
        #[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
        #[allow(missing_docs)]
        pub enum ContentType {
            $($ct),+
        }

        const CONTENT_TYPES: &[&[u8]] = &[$($text),+];

        impl TryFrom<usize> for ContentType {
            type Error = ();

            fn try_from(idx: usize) -> Result<Self, Self::Error> {
                Ok(match idx {
                   $(_ if (Self::$ct as usize) == idx => Self::$ct,)+
                    _ => return Err(()),
                })
            }
        }
    };
}

// specify mime type always in lowercase or it will not work
create_content_types!(
    (ApplicationGzip, b"application/gzip"),
    (ApplicationJavascript, b"application/javascript"),
    (ApplicationJson, b"application/json"),
    (ApplicationOctetStream, b"application/octet-stream"),
    (ApplicationPdf, b"application/pdf"),
    (ApplicationRtf, b"application/rtf"),
    (ApplicationX7ZCompressed, b"application/x-7z-compressed"),
    (ApplicationXBzip2, b"application/bzip2"),
    (ApplicationXhtmlXml, b"application/xhtml+xml"),
    (ApplicationXml, b"application/xml"),
    (ApplicationZip, b"application/x-zip"),
    (FontOtf, b"font/otf"),
    (FontTtf, b"font/ttf"),
    (FontWoff, b"font/woff"),
    (FontWoff2, b"font/woff2"),
    (ImageGif, b"image/gif"),
    (ImageIcon, b"image/vnd.microsoft.icon"),
    (ImageJpeg, b"image/jpeg"),
    (ImagePng, b"image/png"),
    (ImageSvgXml, b"image/svg+xml"),
    (ImageWebp, b"image/webp"),
    (TextCsv, b"text/csv"),
    (TextHtml, b"text/html"),
    (TextHtmlUtf8, b"text/html; charset=utf8"),
    (TextJavascript, b"text/javascript"),
    (TextJavascriptUtf8, b"text/javascript; charset=utf8"),
    (TextPlain, b"text/plain"),
    (TextPlainUtf8, b"text/plain; charset=utf8"),
    (TextXml, b"text/xml")
);

static CONTENT_TYPE_LOOKUP: OnceLock<HashMap<&&[u8], usize>> = OnceLock::new();

#[inline]
#[allow(clippy::incompatible_msrv)]
fn content_type_lookup(value: &[u8]) -> Option<ContentType> {
    let map = CONTENT_TYPE_LOOKUP.get_or_init(|| {
        let mut map = HashMap::new();
        for (n, t) in CONTENT_TYPES.iter().enumerate() {
            let _ = map.insert(t, n);
        }
        map
    });

    let lc = value.to_ascii_lowercase();
    let field = if let Some(field) = lc.strip_prefix(b"content-type:") {
        if let Some(field) = field.strip_prefix(b" ") {
            field
        } else {
            field
        }
    } else {
        lc.as_slice()
    };

    let mut splits = field.splitn(2, |b| *b == b';');
    let field = splits.next().unwrap();
    let mut full_field = field.to_vec();
    let charset = splits.next();

    if let Some(charset) = charset {
        let charset = if charset[0] == b' ' {
            &charset[1..]
        } else {
            charset
        };

        if charset.starts_with(b"charset=") {
            full_field.extend_from_slice(b"; ");
            full_field.extend_from_slice(&charset[..8]);
            if charset.ends_with(b"utf-8") {
                full_field.extend_from_slice(b"utf8");
            } else {
                full_field.extend_from_slice(&charset[8..]);
            }
        }
    }

    let idx = map.get(&&full_field[..]);
    let idx = if let Some(idx) = idx {
        *idx
    } else {
        *map.get(&field)?
    };

    ContentType::try_from(idx).ok()
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.into())
    }
}

impl From<ContentType> for &'static [u8] {
    fn from(value: ContentType) -> Self {
        CONTENT_TYPES[value as usize]
    }
}

impl From<ContentType> for &'static str {
    fn from(value: ContentType) -> Self {
        std::str::from_utf8(CONTENT_TYPES[value as usize]).unwrap()
    }
}

impl From<&ContentType> for &'static str {
    fn from(value: &ContentType) -> Self {
        std::str::from_utf8(CONTENT_TYPES[*value as usize]).unwrap()
    }
}

impl From<ContentType> for AsciiString {
    fn from(value: ContentType) -> Self {
        AsciiString::from_ascii(CONTENT_TYPES[value as usize]).unwrap()
    }
}

impl From<ContentType> for super::Header {
    fn from(value: ContentType) -> Self {
        super::Header {
            field: super::static_header::CONTENT_TYPE_HEADER_FIELD.clone(),
            value: value.into(),
        }
    }
}

impl From<ContentType> for super::HeaderField {
    fn from(_: ContentType) -> Self {
        super::static_header::CONTENT_TYPE_HEADER_FIELD.clone()
    }
}

impl From<ContentType> for super::HeaderFieldValue {
    fn from(value: ContentType) -> Self {
        super::HeaderFieldValue::try_from(CONTENT_TYPES[value as usize]).unwrap()
    }
}

impl TryFrom<&super::Header> for ContentType {
    type Error = ();

    fn try_from(header: &super::Header) -> Result<Self, Self::Error> {
        if header.field == *super::static_header::CONTENT_TYPE_HEADER_FIELD {
            return ContentType::try_from(header.value.as_bytes()).map_err(|_err| ());
        }

        Err(())
    }
}

impl TryFrom<super::Header> for ContentType {
    type Error = ();

    fn try_from(header: super::Header) -> Result<Self, Self::Error> {
        Self::try_from(&header)
    }
}

impl TryFrom<&[u8]> for ContentType {
    type Error = ();

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        content_type_lookup(bytes).ok_or(())
    }
}

impl TryFrom<&str> for ContentType {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        content_type_lookup(value.as_bytes()).ok_or(())
    }
}

impl TryFrom<&AsciiStr> for ContentType {
    type Error = ();

    fn try_from(value: &AsciiStr) -> Result<Self, Self::Error> {
        ContentType::try_from(value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::TryFrom, str::FromStr};

    use crate::common::{ContentType, Header, HeaderField};

    use super::{content_type_lookup, CONTENT_TYPES};

    #[test]
    fn content_types_test() {
        assert!(!CONTENT_TYPES.is_empty());
        for (n, mt) in CONTENT_TYPES.iter().enumerate() {
            assert_eq!(
                ContentType::try_from(*mt).unwrap(),
                ContentType::try_from(n).unwrap(),
                "problem: n = {n} mt = {}",
                std::str::from_utf8(mt).unwrap()
            );
        }
    }

    #[test]
    fn content_type_lookup_test() {
        assert!(!CONTENT_TYPES.is_empty());
        for mt in CONTENT_TYPES {
            assert!(content_type_lookup(mt).is_some());
        }
    }

    #[test]
    fn content_type_header_test() {
        assert_eq!(
            Header::from(ContentType::ApplicationGzip),
            "Content-Type: application/gzip".parse().unwrap()
        );
    }

    #[test]
    fn from_to_header_field_test() {
        assert_eq!(
            Into::<HeaderField>::into(ContentType::ApplicationJson),
            HeaderField::from_str("Content-Type").unwrap()
        );
    }

    #[test]
    fn try_from_header_test() {
        assert_eq!(
            Header::from(ContentType::ApplicationGzip),
            "Content-Type: application/gzip".parse().unwrap()
        );

        assert_eq!(
            ContentType::try_from("Content-Type: text/plain".parse::<Header>().unwrap()).unwrap(),
            ContentType::TextPlain
        );

        assert_eq!(
            ContentType::try_from(
                "Content-Type: text/plain; charset=utf8"
                    .parse::<Header>()
                    .unwrap()
            )
            .unwrap(),
            ContentType::TextPlainUtf8
        );

        assert_eq!(
            ContentType::try_from(
                "Content-Type: text/plain; charset=iso8859-1"
                    .parse::<Header>()
                    .unwrap()
            )
            .unwrap(),
            ContentType::TextPlain
        );

        assert_eq!(
            ContentType::try_from(
                "Content-Type: text/plain; charset=utf-8"
                    .parse::<Header>()
                    .unwrap()
            )
            .unwrap(),
            ContentType::TextPlainUtf8
        );

        assert_eq!(
            ContentType::try_from(
                "Content-Type: application/gzip; charset=utf-8"
                    .parse::<Header>()
                    .unwrap()
            )
            .unwrap(),
            ContentType::ApplicationGzip
        );
    }
}
