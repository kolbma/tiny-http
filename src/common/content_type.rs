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

static CONTENT_TYPES: OnceLock<HashMap<ContentType, &str>> = OnceLock::new();

static CONTENT_TYPE_LOOKUP: OnceLock<HashMap<&str, ContentType>> = OnceLock::new();

// all in lowercase!
// same order as enum
fn content_types() -> &'static HashMap<ContentType, &'static str> {
    CONTENT_TYPES.get_or_init(|| {
        HashMap::from([
            (ContentType::ApplicationGzip, "application/gzip"),
            (ContentType::ApplicationJavascript, "application/javascript"),
            (ContentType::ApplicationJson, "application/json"),
            (
                ContentType::ApplicationOctetStream,
                "application/octet-stream",
            ),
            (ContentType::ApplicationPdf, "application/pdf"),
            (ContentType::ApplicationRtf, "application/rtf"),
            (ContentType::ApplicationXhtmlXml, "application/xhtml+xml"),
            (
                ContentType::ApplicationX7ZCompressed,
                "application/x-7z-compressed",
            ),
            (ContentType::ApplicationXBzip2, "application/bzip2"),
            (ContentType::ApplicationXml, "application/xml"),
            (ContentType::ApplicationZip, "application/zip"),
            (ContentType::FontOtf, "font/otf"),
            (ContentType::FontTtf, "font/ttf"),
            (ContentType::FontWoff, "font/woff"),
            (ContentType::FontWoff2, "font/woff2"),
            (ContentType::ImageGif, "image/gif"),
            (ContentType::ImageIcon, "image/vnd.microsoft.icon"),
            (ContentType::ImageJpeg, "image/jpeg"),
            (ContentType::ImagePng, "image/png"),
            (ContentType::ImageSvgXml, "image/svg+xml"),
            (ContentType::ImageWebp, "image/webp"),
            (ContentType::TextCsv, "text/csv"),
            (ContentType::TextHtml, "text/html"),
            (ContentType::TextJavascript, "text/javascript"),
            (ContentType::TextPlain, "text/plain"),
            (ContentType::TextPlainUtf8, "text/plain; charset=utf8"),
            (ContentType::TextXml, "text/xml"),
        ])
    })
}

fn content_type_lookup() -> &'static HashMap<&'static str, ContentType> {
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

impl From<ContentType> for AsciiString {
    fn from(value: ContentType) -> Self {
        AsciiString::from_str(value.into()).unwrap()
    }
}

impl From<ContentType> for &'static str {
    fn from(value: ContentType) -> Self {
        content_types().get(&value).unwrap()
    }
}

impl From<&ContentType> for &'static str {
    fn from(value: &ContentType) -> Self {
        (*value).into()
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
                        if let Some(ct) = content_type_lookup().get(field.as_str()).copied() {
                            return Ok(ct);
                        }
                    }
                }
            }
            return content_type_lookup().get(field).copied().ok_or(());
        }

        content_type_lookup().get(field).copied().ok_or(())
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
                "problem: k = {k} v = {v}"
            );
        }
    }

    #[test]
    fn content_type_lookup_test() {
        for (k, v) in content_type_lookup() {
            let content_type = ContentType::try_from(*k);
            assert!(content_type.is_ok(), "problem: k = {} v = {}", k, v);
            assert_eq!(*v, content_type.unwrap(), "problem: k = {k} v = {v}");
            assert_eq!(*k, <&str>::from(v), "problem: k = {k} v = {v}");
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
