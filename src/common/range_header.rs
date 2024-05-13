#![cfg(feature = "range-support")]
//! HTTP/1.1 supports [Range Requests](https://datatracker.ietf.org/doc/html/rfc9110#name-range-requests)

use std::convert::TryFrom;

use crate::{Header, HeaderError, HeaderFieldValue};

use super::static_header;

/// Byte Range specified in [RFC 9110](https://datatracker.ietf.org/doc/html/rfc9110#name-byte-ranges)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    /// Relevant when used in [RFC 9110 Content-Range](https://datatracker.ietf.org/doc/html/rfc9110#name-content-range)
    pub complete_length: Option<usize>,
    /// First inclusive position
    pub first_pos: Option<isize>,
    /// `true` for response status 416 [RFC 9110 Content-Range](https://datatracker.ietf.org/doc/html/rfc9110#name-content-range)
    pub is_unsatisfied: bool,
    /// Last inclusive position
    pub last_pos: Option<isize>,
}

impl ByteRange {
    /// Construct `ByteRange` by `first_pos` and `last_pos`
    #[must_use]
    pub fn new(first_pos: isize, last_pos: isize) -> Self {
        Self {
            complete_length: None,
            first_pos: Some(first_pos),
            is_unsatisfied: false,
            last_pos: Some(last_pos),
        }
    }

    /// Relevant when used in [RFC 9110 Content-Range](https://datatracker.ietf.org/doc/html/rfc9110#name-content-range)
    ///
    /// For _unknown_ length use `None`
    pub fn set_complete_length(&mut self, length: Option<usize>) {
        self.complete_length = length;
    }

    /// Relevant when used in [RFC 9110 Content-Range](https://datatracker.ietf.org/doc/html/rfc9110#name-content-range)
    pub fn set_unsatisfied(&mut self) {
        self.is_unsatisfied = true;
    }
}

impl std::fmt::Display for ByteRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_unsatisfied {
            if let Some(complete_length) = self.complete_length {
                f.write_fmt(format_args!("*/{complete_length}"))
            } else {
                f.write_str("*/*")
            }
        } else if let Some(first_pos) = self.first_pos {
            if let Some(last_pos) = self.last_pos {
                if let Some(complete_length) = self.complete_length {
                    f.write_fmt(format_args!("{first_pos}-{last_pos}/{complete_length}"))
                } else {
                    f.write_fmt(format_args!("{first_pos}-{last_pos}"))
                }
            } else if let Some(complete_length) = self.complete_length {
                f.write_fmt(format_args!(
                    "{first_pos}-{}/{complete_length}",
                    complete_length - 1
                ))
            } else {
                f.write_fmt(format_args!("{first_pos}-"))
            }
        } else if let Some(last_pos) = self.last_pos {
            if let Some(complete_length) = self.complete_length {
                f.write_fmt(format_args!("0-{last_pos}/{complete_length}"))
            } else {
                f.write_str(&last_pos.to_string())
            }
        } else {
            f.write_str("")
        }
    }
}

impl TryFrom<&[u8]> for ByteRange {
    type Error = ();

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut content_range = bytes.splitn(2, |b| *b == b'/');

        let values = content_range.next().ok_or(())?.splitn(2, |b| *b == b'-');

        let mut range = Self {
            complete_length: None,
            first_pos: None,
            is_unsatisfied: false,
            last_pos: None,
        };

        for value in values.enumerate() {
            let mut val = value.1;
            let mut pos = 0;
            for b in val {
                if *b != b' ' && *b != b'\t' {
                    break;
                }
                pos += 1;
            }
            val = &val[pos..];
            pos = 0;
            for b in val {
                if *b != b' ' && *b != b'\t' {
                    pos += 1;
                } else {
                    break;
                }
            }
            val = &val[..pos];

            if value.0 == 0 {
                if val.is_empty() {
                    range.last_pos = Some(-1);
                } else if val[0] == b'*' {
                    range.is_unsatisfied = true;
                    break;
                } else {
                    range.first_pos = Some(
                        str::parse::<isize>(std::str::from_utf8(val).map_err(|_| ())?)
                            .map_err(|_| ())?,
                    );
                }
            } else if let Some(last_pos) = &range.last_pos {
                let v = str::parse::<isize>(std::str::from_utf8(val).map_err(|_| ())?)
                    .map_err(|_| ())?;
                range.last_pos = Some(*last_pos * v);
            } else if !val.is_empty() {
                let v = str::parse::<isize>(std::str::from_utf8(val).map_err(|_| ())?)
                    .map_err(|_| ())?;
                range.last_pos = Some(v);
            }
        }

        if let Some(mut complete_length) = content_range.next() {
            let mut pos = 0;
            for b in complete_length {
                if *b != b' ' && *b != b'\t' {
                    break;
                }
                pos += 1;
            }
            complete_length = &complete_length[pos..];
            pos = 0;
            for b in complete_length {
                if *b != b' ' && *b != b'\t' {
                    pos += 1;
                } else {
                    break;
                }
            }
            complete_length = &complete_length[..pos];
            if complete_length[0] != b'*' {
                range.complete_length = Some(
                    str::parse::<usize>(std::str::from_utf8(complete_length).map_err(|_| ())?)
                        .map_err(|_| ())?,
                );
            }
        }

        Ok(range)
    }
}

/// `RangeHeader` useable in headers _Range_ and _Content-Range_
#[derive(Clone, Debug)]
pub struct RangeHeader {
    /// See [`RangeUnit`]
    pub range_unit: RangeUnit,
    /// One or more [`ByteRange`] specified in [RFC 9110](https://datatracker.ietf.org/doc/html/rfc9110#name-byte-ranges)
    pub ranges: Vec<ByteRange>,
}

impl std::fmt::Display for RangeHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.range_unit {
            RangeUnit::None => f.write_str(&RangeUnit::None.to_string()),
            RangeUnit::Bytes => {
                let mut ranges = String::new();
                for range in &self.ranges {
                    ranges += ",";
                    if let Some(pos) = range.first_pos {
                        ranges += &pos.to_string();
                    }
                    ranges += "-";
                    if let Some(pos) = range.last_pos {
                        ranges += &pos.to_string();
                    }
                }
                f.write_fmt(format_args!("{} {}", RangeUnit::Bytes, &ranges[1..]))
            }
        }
    }
}

impl TryFrom<&[u8]> for RangeHeader {
    type Error = ();

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut value = bytes.rsplitn(2, |b| *b == b':').next().ok_or(())?;

        while !value.is_empty() && (value[0] == b' ' || value[0] == b'\t') {
            value = &value[1..];
        }

        if &value[..4] == RangeUnit::None.to_string().as_bytes() {
            return Ok(Self {
                range_unit: RangeUnit::None,
                ranges: Vec::new(),
            });
        }

        if &value[..5] != RangeUnit::Bytes.to_string().as_bytes() {
            return Err(());
        };

        let mut value = value.splitn(2, |b| *b == value[5]);

        let range_unit = RangeUnit::try_from(value.next().ok_or(())?)?;

        let ranges_split = value.next().ok_or(())?.split(|b| *b == b',');

        let mut ranges = Vec::new();

        for range in ranges_split {
            let range = ByteRange::try_from(range).unwrap_or(ByteRange {
                complete_length: None,
                first_pos: None,
                is_unsatisfied: true,
                last_pos: None,
            });

            ranges.push(range);
        }

        Ok(Self { range_unit, ranges })
    }
}

impl TryFrom<&str> for RangeHeader {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&HeaderFieldValue> for RangeHeader {
    type Error = ();

    fn try_from(value: &HeaderFieldValue) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&Header> for RangeHeader {
    type Error = ();

    fn try_from(header: &Header) -> Result<Self, Self::Error> {
        Self::try_from(&header.value)
    }
}

impl TryFrom<&RangeHeader> for Header {
    type Error = HeaderError;

    fn try_from(value: &RangeHeader) -> Result<Self, Self::Error> {
        let mut range = value.range_unit.to_string();
        range += " ";
        let mut ranges = value.ranges.iter();
        let r = ranges.next();
        if let Some(r) = r {
            range += &r.to_string();
            for r in ranges {
                range += ",";
                range += &r.to_string();
            }
        }

        Self::from_bytes(
            &static_header::CONTENT_RANGE_HEADER_FIELD.as_bytes(),
            &range,
        )
    }
}

/// `RangeUnit` as specified in [RFC 9110](https://datatracker.ietf.org/doc/html/rfc9110#name-range-units)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeUnit {
    /// reserved as keyword to indicate range requests are not supported
    None,
    /// a range of octets
    Bytes,
}

impl std::fmt::Display for RangeUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Bytes => f.write_str("bytes"),
        }
    }
}

impl TryFrom<&[u8]> for RangeUnit {
    type Error = ();

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes == b"bytes" {
            Ok(Self::Bytes)
        } else if bytes == b"none" {
            Ok(Self::None)
        } else {
            Err(())
        }
    }
}

impl TryFrom<&HeaderFieldValue> for RangeUnit {
    type Error = ();

    fn try_from(value: &HeaderFieldValue) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

impl TryFrom<&str> for RangeUnit {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.as_bytes())
    }
}

pub(crate) mod request {
    use std::{convert::TryFrom, io::Read};

    use crate::{common::HeaderData, HttpVersion, Method, Request, Response};

    // https://datatracker.ietf.org/doc/html/rfc9110#name-range-requests
    #[inline]
    pub(crate) fn create_ranges(
        version: HttpVersion,
        method: &Method,
        headers: &HeaderData,
    ) -> (Option<crate::RangeHeader>, Option<crate::RangeHeader>) {
        use crate::common::static_header;

        if version >= HttpVersion::Version1_1 {
            if *method == Method::Get {
                (
                    None,
                    headers
                        .header_first(&static_header::RANGE_HEADER_FIELD.as_bytes())
                        .and_then(|h| crate::RangeHeader::try_from(h).ok()),
                )
            } else if *method == Method::Put {
                (
                    headers
                        .header_first(&static_header::CONTENT_RANGE_HEADER_FIELD.as_bytes())
                        .and_then(|h| crate::RangeHeader::try_from(h).ok()),
                    None,
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    }

    #[inline]
    pub(crate) fn respond_update_headers<R>(request: &mut Request, response: &mut Response<R>)
    where
        R: Read,
    {
        if request.http_version() >= HttpVersion::Version1_1
            && (200..300).contains(&response.status_code())
        {
            if let Some(header) = &request.range() {
                let mut is_ok = false;

                if header.range_unit == crate::RangeUnit::Bytes && header.ranges.len() == 1 {
                    let range = header.ranges[0];

                    if !range.is_unsatisfied {
                        let first_pos = range.first_pos.unwrap_or(0);
                        #[allow(clippy::cast_possible_wrap)]
                        let data_length = response.data_length().unwrap_or_default() as isize;

                        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
                        if first_pos >= 0 && (data_length != 0 && first_pos < data_length) {
                            if let Some(last_pos) = range.last_pos {
                                if last_pos >= 0
                                    && first_pos <= last_pos
                                    && (data_length != 0 && last_pos < data_length)
                                {
                                    response.set_content_range(
                                        first_pos as usize,
                                        last_pos as usize,
                                        response.data_length(),
                                    );
                                    super::response::update_headers_for_range(response);
                                    is_ok = true;
                                } else if last_pos < 0 {
                                    if let Some(data_length) = response.data_length() {
                                        if ((-last_pos) as usize) <= data_length {
                                            let n = data_length as isize - (-last_pos);
                                            if n >= 0 {
                                                response.set_content_range(
                                                    n as usize,
                                                    data_length - 1,
                                                    response.data_length(),
                                                );
                                                super::response::update_headers_for_range(response);
                                                is_ok = true;
                                            }
                                        }
                                    }
                                }
                            } else if let Some(data_length) = response.data_length() {
                                response.set_content_range(
                                    first_pos as usize,
                                    data_length - 1,
                                    response.data_length(),
                                );
                                super::response::update_headers_for_range(response);
                                is_ok = true;
                            }
                        }
                    }
                } else if header.range_unit == crate::RangeUnit::None {
                    is_ok = true;
                }

                if !is_ok {
                    response.set_content_range_unsatisfied();
                    super::response::update_headers_for_range(response);
                }
            }
        }
    }
}

pub(crate) mod response {
    use std::{
        convert::TryFrom,
        io::{self, Read, Write},
    };

    use crate::{
        common,
        response::{util, Standard},
        Header, HeaderFieldValue, Response,
    };

    /// Check _Range_ for unsatisfied value
    pub(crate) fn is_content_range_unsatisfied<R: Read>(response: &Response<R>) -> bool {
        if let Some(content_range) = &response.content_range {
            if !content_range.ranges.is_empty() {
                return content_range.ranges[0].is_unsatisfied;
            }
        }
        false
    }

    /// Calculate _Content-Length_ value for `content_range`
    pub(crate) fn range_content_length(content_range: &crate::RangeHeader) -> Option<usize> {
        if content_range.range_unit == crate::RangeUnit::Bytes && !content_range.ranges.is_empty() {
            let range = content_range.ranges[0];
            if range.is_unsatisfied {
                return Some(0);
            }
            if let Some(first_pos) = range.first_pos {
                if let Some(last_pos) = range.last_pos {
                    #[allow(clippy::cast_sign_loss)]
                    return Some((last_pos + 1 - first_pos) as usize);
                }
            }
        }
        None
    }

    /// Copy range by http header _Content-Range_ from supplied `reader` to `writer`
    #[inline]
    pub(crate) fn range_copy<T: Read, W: Write>(
        reader: &mut T,
        writer: &mut W,
        content_range: &crate::RangeHeader,
    ) -> io::Result<u64> {
        if content_range.range_unit == crate::RangeUnit::Bytes && !content_range.ranges.is_empty() {
            let range = content_range.ranges[0];
            if let Some(first_pos) = range.first_pos {
                if first_pos > 0 {
                    let mut buf = [0u8; 4096];
                    #[allow(clippy::cast_sign_loss)]
                    let mut count = first_pos as usize;
                    while count > 0 {
                        let buf_size = if count > 4096 { 4096 } else { count };
                        reader.read_exact(&mut buf[0..buf_size])?;
                        count -= buf_size;
                    }
                }
                if let Some(last_pos) = range.last_pos {
                    #[allow(clippy::cast_sign_loss)]
                    let limit = (last_pos + 1 - first_pos) as u64;
                    return io::copy(&mut reader.take(limit), writer);
                }
            }
        }

        io::copy(reader, writer)
    }

    /// Set `content_range` of [`Response`]
    pub(crate) fn set_content_range<R: Read>(
        response: &mut Response<R>,
        first_pos: usize,
        last_pos: usize,
        complete_length: Option<usize>,
    ) {
        response.content_range = Some(crate::RangeHeader {
            range_unit: crate::RangeUnit::Bytes,
            ranges: vec![crate::ByteRange {
                complete_length,
                first_pos: Some(if first_pos > isize::MAX as usize {
                    isize::MAX
                } else {
                    #[allow(clippy::cast_possible_wrap)] // limited to isize::MAX
                    {
                        first_pos as isize
                    }
                }),
                is_unsatisfied: false,
                last_pos: Some(if last_pos > isize::MAX as usize {
                    isize::MAX
                } else {
                    #[allow(clippy::cast_possible_wrap)] // limited to isize::MAX
                    {
                        last_pos as isize
                    }
                }),
            }],
        });
    }

    /// Mark _Range_ as unsatisfied value
    pub(crate) fn set_content_range_unsatisfied<R: Read>(response: &mut Response<R>) {
        if let Some(content_range) = &mut response.content_range {
            if !content_range.ranges.is_empty() {
                content_range.ranges[0].is_unsatisfied = true;
            }
        } else {
            response.content_range = Some(crate::RangeHeader {
                range_unit: crate::RangeUnit::Bytes,
                ranges: vec![crate::ByteRange {
                    complete_length: None,
                    first_pos: None,
                    last_pos: None,
                    is_unsatisfied: true,
                }],
            });
        }
    }

    /// Update HTTP header _Content-Length_ for range
    #[inline]
    pub(crate) fn update_header_content_length<R: Read>(
        response: &mut Response<R>,
        headers: &mut [Header],
    ) {
        if let Some(content_range) = &response.content_range {
            if let Some(content_length) = range_content_length(content_range) {
                let mut b = [0u8; 20];
                let content_length =
                    crate::response::util::number_to_bytes!(content_length, &mut b, 20);

                // let headers = response.headers_mut().as_mut().unwrap();
                if let Some(header) = headers
                    .iter_mut()
                    .find(|h| h.field == *common::static_header::CONTENT_LENGTH_HEADER_FIELD)
                {
                    header.value = HeaderFieldValue::try_from(content_length).unwrap();
                }
            }
        }
    }

    /// Update HTTP headers of [`Response`] for range handling
    #[inline]
    pub(crate) fn update_headers_for_range<R: Read>(response: &mut Response<R>) {
        let range_header = response
            .content_range
            .as_ref()
            .and_then(|content_range| Header::try_from(content_range).ok());

        if let Some(range_header) = range_header {
            {
                let headers = response.headers_mut().as_mut().unwrap();
                headers.push(range_header);
            }

            if response.is_content_range_unsatisfied() {
                response.set_status_code(Standard::RangeNotSatisfiable416.into());
                util::update_optional_hashset(
                    response.filter_headers_mut(),
                    [
                        common::static_header::CONTENT_LENGTH_HEADER_FIELD.clone(),
                        common::static_header::CONTENT_TYPE_HEADER_FIELD.clone(),
                    ],
                );
            } else {
                response.set_status_code(Standard::PartialContent206.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::Header;

    use super::*;

    #[test]
    fn byte_range_test() {
        let values = [
            "0-499",
            "500-999",
            "-500",
            "9500-",
            " 0 - 499 ",
            " 500 - 999 ",
            " - 500 ",
            " 9500 - ",
        ];

        let results = [
            ByteRange::new(0, 499),
            ByteRange::new(500, 999),
            ByteRange {
                complete_length: None,
                first_pos: None,
                is_unsatisfied: false,
                last_pos: Some(-500),
            },
            ByteRange {
                complete_length: None,
                first_pos: Some(9500),
                is_unsatisfied: false,
                last_pos: None,
            },
            ByteRange::new(0, 499),
            ByteRange::new(500, 999),
            ByteRange {
                complete_length: None,
                first_pos: None,
                is_unsatisfied: false,
                last_pos: Some(-500),
            },
            ByteRange {
                complete_length: None,
                first_pos: Some(9500),
                is_unsatisfied: false,
                last_pos: None,
            },
        ];

        for v in values.iter().enumerate() {
            assert_eq!(
                ByteRange::try_from(v.1.as_bytes()).unwrap(),
                results[v.0],
                "v: {}",
                v.1
            );
        }
    }

    #[test]
    fn byte_range_to_string_test() {
        let ranges = [
            ByteRange::new(0, 499),
            ByteRange::new(500, 999),
            ByteRange {
                complete_length: None,
                first_pos: None,
                is_unsatisfied: false,
                last_pos: Some(-500),
            },
            ByteRange {
                complete_length: None,
                first_pos: Some(9500),
                is_unsatisfied: false,
                last_pos: None,
            },
            ByteRange::new(0, 499),
            ByteRange::new(500, 999),
            ByteRange {
                complete_length: None,
                first_pos: None,
                is_unsatisfied: false,
                last_pos: Some(-500),
            },
            ByteRange {
                complete_length: None,
                first_pos: Some(9500),
                is_unsatisfied: false,
                last_pos: None,
            },
            ByteRange {
                complete_length: Some(22222),
                first_pos: Some(9500),
                is_unsatisfied: false,
                last_pos: Some(9999),
            },
            ByteRange {
                complete_length: Some(22222),
                first_pos: None,
                is_unsatisfied: false,
                last_pos: Some(9999),
            },
            ByteRange {
                complete_length: Some(22222),
                first_pos: Some(9500),
                is_unsatisfied: false,
                last_pos: None,
            },
            ByteRange {
                complete_length: Some(1111),
                first_pos: None,
                is_unsatisfied: true,
                last_pos: None,
            },
        ];

        let results = [
            "0-499",
            "500-999",
            "-500",
            "9500-",
            "0-499",
            "500-999",
            "-500",
            "9500-",
            "9500-9999/22222",
            "0-9999/22222",
            "9500-22221/22222",
            "*/1111",
        ];

        for v in ranges.iter().enumerate() {
            let s = v.1.to_string();
            assert_eq!(&s, results[v.0], "s: {}", &s);
        }
    }

    #[test]
    fn range_header_range_test() {
        let values = [
            "0-499",
            "500-999",
            "-500",
            "9500-",
            "0-0,-1",
            "0-999,4500-5499,-1000",
            " 0-999, 4500-5499, -1000",
            "500-600,601-999",
            "500-700,601-999",
        ];

        let results = [
            vec![ByteRange::new(0, 499)],
            vec![ByteRange::new(500, 999)],
            vec![ByteRange {
                complete_length: None,
                first_pos: None,
                is_unsatisfied: false,
                last_pos: Some(-500),
            }],
            vec![ByteRange {
                complete_length: None,
                first_pos: Some(9500),
                is_unsatisfied: false,
                last_pos: None,
            }],
            vec![
                ByteRange::new(0, 0),
                ByteRange {
                    complete_length: None,
                    first_pos: None,
                    is_unsatisfied: false,
                    last_pos: Some(-1),
                },
            ],
            vec![
                ByteRange::new(0, 999),
                ByteRange::new(4500, 5499),
                ByteRange {
                    complete_length: None,
                    first_pos: None,
                    is_unsatisfied: false,
                    last_pos: Some(-1000),
                },
            ],
            vec![
                ByteRange::new(0, 999),
                ByteRange::new(4500, 5499),
                ByteRange {
                    complete_length: None,
                    first_pos: None,
                    is_unsatisfied: false,
                    last_pos: Some(-1000),
                },
            ],
            vec![ByteRange::new(500, 600), ByteRange::new(601, 999)],
            vec![ByteRange::new(500, 700), ByteRange::new(601, 999)],
        ];

        for v in values.iter().enumerate() {
            let header = Header::try_from(format!("Range: bytes={}", v.1).as_bytes()).unwrap();
            let range_header =
                RangeHeader::try_from(&header).unwrap_or_else(|()| panic!("v: {:?}", v));
            assert_eq!(range_header.range_unit, RangeUnit::Bytes);
            assert_eq!(range_header.ranges, results[v.0]);
        }
    }

    #[test]
    fn range_header_overflow_test() {
        let values = [
            usize::MAX.to_string(),
            String::from("-") + &usize::MAX.to_string(),
        ];

        for v in values.iter().enumerate() {
            let header = Header::try_from(format!("Range: bytes={}", v.1).as_bytes()).unwrap();
            let range_header =
                RangeHeader::try_from(&header).unwrap_or_else(|()| panic!("v: {:?}", v));
            assert_eq!(range_header.range_unit, RangeUnit::Bytes);
            assert!(range_header.ranges[0].is_unsatisfied);
        }
    }

    #[test]
    fn range_header_content_range_test() {
        let values = [
            "0-499/*",
            "500-999/31923",
            "-500/*",
            "9500-/*",
            "0-0/*",
            " 0-999 / *",
            "*/34738",
            " * / 34738 ",
        ];

        let results = [
            ByteRange {
                complete_length: None,
                first_pos: Some(0),
                is_unsatisfied: false,
                last_pos: Some(499),
            },
            ByteRange {
                complete_length: Some(31923),
                first_pos: Some(500),
                is_unsatisfied: false,
                last_pos: Some(999),
            },
            ByteRange {
                complete_length: None,
                first_pos: None,
                is_unsatisfied: false,
                last_pos: Some(-500),
            },
            ByteRange {
                complete_length: None,
                first_pos: Some(9500),
                is_unsatisfied: false,
                last_pos: None,
            },
            ByteRange {
                complete_length: None,
                first_pos: Some(0),
                is_unsatisfied: false,
                last_pos: Some(0),
            },
            ByteRange {
                complete_length: None,
                first_pos: Some(0),
                is_unsatisfied: false,
                last_pos: Some(999),
            },
            ByteRange {
                complete_length: Some(34738),
                first_pos: None,
                is_unsatisfied: true,
                last_pos: None,
            },
            ByteRange {
                complete_length: Some(34738),
                first_pos: None,
                is_unsatisfied: true,
                last_pos: None,
            },
        ];

        for v in values.iter().enumerate() {
            let header =
                Header::try_from(format!("Content-Range:  bytes  {}", v.1).as_bytes()).unwrap();
            let range_header =
                RangeHeader::try_from(&header).unwrap_or_else(|()| panic!("v: {:?}", v));
            assert_eq!(range_header.range_unit, RangeUnit::Bytes);
            assert_eq!(range_header.ranges[0], results[v.0]);
        }
    }

    #[test]
    fn range_unit_convert_test() {
        for p in [("bytes", RangeUnit::Bytes), ("none", RangeUnit::None)] {
            assert_eq!(RangeUnit::try_from(p.0).unwrap(), p.1);
            assert_eq!(RangeUnit::try_from(p.0.as_bytes()).unwrap(), p.1);

            let header = Header::from_str(&(String::from("Accept-Ranges: ") + p.0)).unwrap();
            assert_eq!(RangeUnit::try_from(&header.value).unwrap(), p.1);
        }
    }

    #[test]
    fn range_unit_to_string_test() {
        for p in [("bytes", RangeUnit::Bytes), ("none", RangeUnit::None)] {
            assert_eq!(p.1.to_string(), p.0);
        }
    }
}
