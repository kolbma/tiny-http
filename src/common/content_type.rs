#![cfg(feature = "content-type")]

use ascii::{AsciiStr, AsciiString};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::{self, Formatter};
use std::str::FromStr;
use std::sync::OnceLock;

use crate::common;

/// HTTP protocol Content-Type header values
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum ContentType {
    ApplicationGzip,
    ApplicationJavascript,
    ApplicationJson,
    ApplicationOctetStream,
    ApplicationPdf,
    ApplicationRtf,
    ApplicationX7ZCompressed,
    ApplicationXBzip2,
    ApplicationXhtmlXml,
    ApplicationXml,
    ApplicationZip,
    FontOtf,
    FontTtf,
    FontWoff,
    FontWoff2,
    ImageGif,
    ImageIcon,
    ImageJpeg,
    ImagePng,
    ImageSvgXml,
    ImageWebp,
    TextCsv,
    TextHtml,
    TextJavascript,
    TextPlain,
    TextPlainUtf8,
    TextXml,
}

static CONTENT_TYPES: OnceLock<HashMap<ContentType, &[u8]>> = OnceLock::new();

static CONTENT_TYPE_LOOKUP: OnceLock<HashMap<&[u8], ContentType>> = OnceLock::new();

// all in lowercase!
// same order as enum
fn content_types() -> &'static HashMap<ContentType, &'static [u8]> {
    CONTENT_TYPES.get_or_init(|| {
        HashMap::from([
            (ContentType::ApplicationGzip, &b"application/gzip"[..]),
            (
                ContentType::ApplicationJavascript,
                b"application/javascript",
            ),
            (ContentType::ApplicationJson, b"application/json"),
            (
                ContentType::ApplicationOctetStream,
                b"application/octet-stream",
            ),
            (ContentType::ApplicationPdf, b"application/pdf"),
            (ContentType::ApplicationRtf, b"application/rtf"),
            (ContentType::ApplicationXhtmlXml, b"application/xhtml+xml"),
            (
                ContentType::ApplicationX7ZCompressed,
                b"application/x-7z-compressed",
            ),
            (ContentType::ApplicationXBzip2, b"application/bzip2"),
            (ContentType::ApplicationXml, b"application/xml"),
            (ContentType::ApplicationZip, b"application/zip"),
            (ContentType::FontOtf, b"font/otf"),
            (ContentType::FontTtf, b"font/ttf"),
            (ContentType::FontWoff, b"font/woff"),
            (ContentType::FontWoff2, b"font/woff2"),
            (ContentType::ImageGif, b"image/gif"),
            (ContentType::ImageIcon, b"image/vnd.microsoft.icon"),
            (ContentType::ImageJpeg, b"image/jpeg"),
            (ContentType::ImagePng, b"image/png"),
            (ContentType::ImageSvgXml, b"image/svg+xml"),
            (ContentType::ImageWebp, b"image/webp"),
            (ContentType::TextCsv, b"text/csv"),
            (ContentType::TextHtml, b"text/html"),
            (ContentType::TextJavascript, b"text/javascript"),
            (ContentType::TextPlain, b"text/plain"),
            (ContentType::TextPlainUtf8, b"text/plain; charset=utf8"),
            (ContentType::TextXml, b"text/xml"),
        ])
    })
}

fn content_type_lookup() -> &'static HashMap<&'static [u8], ContentType> {
    CONTENT_TYPE_LOOKUP.get_or_init(|| {
        let mut map = HashMap::new();
        for (k, v) in content_types() {
            let _ = map.insert(*v, *k);
        }
        map
    })
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.into())
    }
}

impl From<ContentType> for &'static [u8] {
    fn from(value: ContentType) -> Self {
        content_types().get(&value).unwrap()
    }
}

impl From<ContentType> for &'static str {
    fn from(value: ContentType) -> Self {
        std::str::from_utf8(<&[u8]>::from(value)).unwrap()
    }
}

impl From<&ContentType> for &'static str {
    fn from(value: &ContentType) -> Self {
        Self::from(*value)
    }
}

impl From<ContentType> for AsciiString {
    fn from(value: ContentType) -> Self {
        AsciiString::from_str(value.into()).unwrap()
    }
}

impl From<ContentType> for super::Header {
    fn from(value: ContentType) -> Self {
        super::Header {
            field: common::static_header::CONTENT_TYPE_HEADER_FIELD.clone(),
            value: value.into(),
        }
    }
}

impl From<ContentType> for super::HeaderField {
    fn from(_: ContentType) -> Self {
        common::static_header::CONTENT_TYPE_HEADER_FIELD.clone()
    }
}

impl From<ContentType> for super::HeaderFieldValue {
    fn from(value: ContentType) -> Self {
        super::HeaderFieldValue::try_from(<&[u8]>::from(value)).unwrap()
    }
}

impl TryFrom<&super::Header> for ContentType {
    type Error = ();

    fn try_from(header: &super::Header) -> Result<Self, Self::Error> {
        if header.field == ContentType::ApplicationGzip.into() {
            return ContentType::try_from(header.value.as_str()).map_err(|_err| ());
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

// TODO: implement TryFrom<&[u8]> for ContentType and convert as_bytes from &str
impl TryFrom<&[u8]> for ContentType {
    type Error = ();

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Self::try_from(std::str::from_utf8(bytes).map_err(|_| ())?)
    }
}

impl TryFrom<&str> for ContentType {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let lowercase = value.to_ascii_lowercase();
        let lowercase = lowercase.as_str();

        let field = if let Some(field) = lowercase.strip_prefix("content-type:") {
            if let Some(field) = field.strip_prefix(' ') {
                field
            } else {
                field
            }
        } else {
            lowercase
        };

        if let Some((field, charset)) = field.split_once(';') {
            if let Some((cfield, charset)) = charset.split_once('=') {
                if cfield.trim().to_ascii_lowercase() == "charset" {
                    let charset = charset.trim().to_ascii_lowercase().replace('-', "");
                    if &charset == "utf8" {
                        let field = format!("{field}; charset={charset}");
                        if let Some(ct) = content_type_lookup().get(field.as_bytes()).copied() {
                            return Ok(ct);
                        }
                    }
                }
            }
            return content_type_lookup()
                .get(field.as_bytes())
                .copied()
                .ok_or(());
        }

        content_type_lookup()
            .get(field.as_bytes())
            .copied()
            .ok_or(())
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

    use super::{content_type_lookup, content_types};

    #[test]
    fn content_types_test() {
        for (k, v) in content_types() {
            assert_eq!(
                *k,
                ContentType::try_from(*v).unwrap(),
                "problem: k = {k} v = {}",
                std::str::from_utf8(v).unwrap()
            );
        }
    }

    #[test]
    fn content_type_lookup_test() {
        for (k, v) in content_type_lookup() {
            let content_type = ContentType::try_from(*k);
            assert!(
                content_type.is_ok(),
                "problem: k = {} v = {}",
                std::str::from_utf8(k).unwrap(),
                v
            );
            assert_eq!(
                *v,
                content_type.unwrap(),
                "problem: k = {} v = {v}",
                std::str::from_utf8(k).unwrap()
            );
            assert_eq!(
                std::str::from_utf8(k).unwrap(),
                <&str>::from(v),
                "problem: k = {} v = {v}",
                std::str::from_utf8(k).unwrap()
            );
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
