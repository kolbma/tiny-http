use std::cmp::Ordering;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::hash::Hash;
use std::io::{Result as IoResult, Write};

use crate::common::{self, Header, HeaderField, HeaderFieldValue, HttpVersion, StatusCode};

use super::date_header::DateHeader;
use super::transfer_encoding::TransferEncoding;

pub(super) fn choose_transfer_encoding(
    status_code: StatusCode,
    te_headers: &Option<Vec<&Header>>,
    http_version: HttpVersion,
    entity_length: &Option<usize>,
    has_additional_headers: bool,
    chunked_threshold: usize,
) -> TransferEncoding {
    use crate::util;

    // HTTP 1.0 doesn't support other encoding
    if http_version <= HttpVersion::Version1_0 {
        return TransferEncoding::Identity;
    }

    // Per section 3.3.1 of RFC7230:
    // A server MUST NOT send a Transfer-Encoding header field in any response with a status code
    // of 1xx (Informational) or 204 (No Content).
    if status_code.0 < 200 || status_code.0 == 204 {
        return TransferEncoding::Identity;
    }

    // if we have additional headers, using chunked
    if has_additional_headers {
        return TransferEncoding::Chunked;
    }

    // if we don't have a Content-Length, or if the Content-Length is too big, using chunks writer
    if entity_length
        .as_ref()
        .map_or(true, |val| *val >= chunked_threshold)
    {
        return TransferEncoding::Chunked;
    }

    // parsing the request's TE header
    if let Some(te_headers) = &te_headers {
        // getting the corresponding TransferEncoding
        let h = te_headers[0];
        // getting list of requested elements
        let mut parse = util::parse_header_value(&h.value);

        // sorting elements by most priority
        parse.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

        // trying to parse each requested encoding
        for value in parse {
            // q=0 are ignored
            if value.1 <= 0.0 {
                continue;
            }

            if let Ok(te) = TransferEncoding::try_from(value.0) {
                return te;
            }
        }
    }

    // Identity by default
    TransferEncoding::Identity
}

/// Check if body is allowed with [`StatusCode`]
#[inline]
pub(super) fn is_body_for_status_ignored<S>(status_code: S) -> bool
where
    S: Into<StatusCode>,
{
    // TODO: check status codes with body supported
    matches!(status_code.into().0, 100..=199 | 204 | 205 | 304) // status code 1xx, 204, 205 and 304 MUST not include a body
}

#[inline]
pub(super) fn set_default_headers_if_not_set(headers: &Option<Vec<Header>>) -> Vec<Header> {
    let mut check_bitfield = 0u8;

    if let Some(headers) = headers {
        for header in headers {
            if check_bitfield == 3 {
                return Vec::with_capacity(0);
            }

            check_bitfield |= if header.field.equiv("Date") {
                1
            } else if header.field.equiv("Server") {
                2
            } else {
                0
            };
        }
    }

    let mut headers = Vec::with_capacity(6);

    // add `Date` if not in the headers
    if check_bitfield & 1 == 0 {
        headers.push(DateHeader::current());
    }

    // add `Server` if not in the headers
    if check_bitfield & 2 == 0 {
        headers.push(Header::from_bytes(b"Server", b"tiny-http").unwrap());
    }

    headers
}

#[inline]
pub(super) fn update_optional_hashset<T, const N: usize>(
    set: &mut Option<HashSet<T>>,
    values: [T; N],
) where
    T: Eq + Hash,
{
    if let Some(set) = set {
        set.extend(values);
    } else {
        *set = Some(HashSet::from(values));
    }
}

/// Sets `header` in `headers` and if `header.field` exists, overwrite it
#[inline]
pub(crate) fn update_optional_header(
    headers: &mut Option<Vec<Header>>,
    header: Header,
    always_push: bool,
) {
    if let Some(headers) = headers {
        if always_push {
            // push always, so multiple entries with same field possible
            headers.push(header);
        } else if let Some(type_header) = headers.iter_mut().find(|h| h.field == header.field) {
            // if the header is already set, overwrite it
            type_header.value = header.value;
        } else {
            // push only if not set
            headers.push(header);
        }
    } else {
        *headers = Some(Vec::from([header]));
    }
}

/// Get the digits of primitive number in bytes
// should be roughly double in speed than a to_string() conversion
macro_rules! number_to_bytes {
    ($n:expr, $buf:expr, $buf_len:expr) => {{
        let mut n = $n;
        let mut digits = [0u8; $buf_len];
        let mut idx = $buf_len;

        while n > 0 {
            idx -= 1;
            #[allow(clippy::cast_possible_truncation, trivial_numeric_casts)]
            // truncation intended
            {
                digits[idx] = (n % 10) as u8 + 48;
            }
            n /= 10;
        }

        // handle 0 digit
        if n == 0 && idx == $buf_len {
            idx -= 1;
            digits[idx] = 48;
        }

        let len = $buf_len - idx;

        for b in $buf.iter_mut() {
            // debug_assert!(idx < $buf_len);
            *b = digits[idx];
            idx += 1;
            if idx == $buf_len {
                break;
            }
        }

        &$buf[..len]
    }};
}
pub(crate) use number_to_bytes;

/// preparing headers for transfer
#[inline]
pub(super) fn update_te_headers(
    headers: &mut Vec<Header>,
    transfer_encoding: Option<TransferEncoding>,
    data_length: &Option<usize>,
) {
    match transfer_encoding {
        Some(TransferEncoding::Chunked) => {
            headers.push(common::static_header::TE_CHUNKED_HEADER.clone());
        }
        Some(TransferEncoding::Identity) => {
            debug_assert!(data_length.is_some());

            let mut cl_header = common::static_header::CONTENT_LENGTH_HEADER.clone();
            cl_header.value = HeaderFieldValue::try_from(data_length.unwrap()).unwrap();

            headers.push(cl_header);
        }
        _ => {}
    };
}

#[inline]
pub(super) fn write_message_header<W>(
    writer: &mut W,
    http_version: HttpVersion,
    status_code: StatusCode,
    prepend_headers: &[Header],
    headers: &Option<Vec<Header>>,
    filter_headers: &Option<HashSet<HeaderField>>,
) -> IoResult<()>
where
    W: Write,
{
    let mut status_line = [b' '; 15 + 31]; // 31 is longest reasonphrase
    status_line[0..8].copy_from_slice(http_version.header().as_bytes());
    let _ = number_to_bytes!(status_code.0, status_line[9..12], 3);
    let phrase = status_code.default_reason_phrase();
    let phrase_end = phrase.len() + 13;
    status_line[13..phrase_end].copy_from_slice(phrase.as_bytes());
    status_line[phrase_end..(phrase_end + 2)].copy_from_slice(&[b'\r', b'\n']);

    writer.write_all(&status_line[0..(phrase_end + 2)])?;

    // writing headers
    let header_iter = if let Some(headers) = headers {
        prepend_headers.iter().chain(headers.iter())
    } else {
        prepend_headers.iter().chain([].iter())
    };
    for header in header_iter {
        if let Some(filter_headers) = filter_headers {
            if filter_headers.contains(&header.field) && !header.field.equiv("Date") {
                continue;
            }
        }

        writer.write_all(header.field.as_bytes())?;
        writer.write_all(&[b':', b' '])?;
        writer.write_all(header.value.as_str().as_bytes())?;
        writer.write_all(&[b'\r', b'\n'])?;
    }

    // separator between header and data
    writer.write_all(&[b'\r', b'\n'])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, str::FromStr, time::Instant};

    use crate::{common::HeaderError, Header, HeaderField, HttpVersion};

    use super::*;

    #[test]
    fn number_to_bytes_bench_test() {
        let mut b = [0u8; 20];
        let b = number_to_bytes!(1, &mut b, 20);
        assert_eq!(b, "1".as_bytes());

        let rounds = 100_000;
        let numbers = [
            (32486usize, 5),
            (19739, 5),
            (12, 2),
            (329, 3),
            (2401, 4),
            (0, 1),
            (10_737_418_240, 11),
        ];

        let now = Instant::now();

        for _ in 0..rounds {
            for n in numbers {
                let s = n.0.to_string();
                let b = s.as_bytes();
                assert_eq!(b.len(), n.1);
                assert_eq!(
                    std::str::from_utf8(b).unwrap().parse::<usize>().unwrap(),
                    n.0
                );
            }
        }

        let elaps_string = now.elapsed();

        let now = Instant::now();

        for _ in 0..rounds {
            for n in numbers {
                let mut b = [0u8; 20];
                let b = number_to_bytes!(n.0, &mut b, 20);
                assert_eq!(b.len(), n.1);
                assert_eq!(
                    std::str::from_utf8(b).unwrap().parse::<usize>().unwrap(),
                    n.0,
                );
            }
        }

        let elaps_calc = now.elapsed();

        // be sure to check this out with release optimization
        // `cargo t -r --lib response::util::tests::usize_to_byte_digits_bench_test`
        // before thinking to_string() would be faster

        assert!(
            // elaps_calc * 10 < elaps_string * 4, // in release mode this is 99% successful
            elaps_calc * 10 < elaps_string * 8,
            "elaps_calc: {} elaps_string: {}",
            elaps_calc.as_micros(),
            elaps_string.as_micros()
        );
    }

    #[test]
    fn test_filter_header() -> Result<(), HeaderError> {
        assert!(HashSet::from([HeaderField::from_str("Server")?])
            .contains(&HeaderField::from_str("server")?));

        let mut writer = Vec::new();
        let result = write_message_header(
            &mut writer,
            HttpVersion::Version1_1,
            StatusCode(200),
            &[],
            &Some(vec![
                DateHeader::current(),
                Header::from_bytes(b"Server", b"tiny-http").unwrap(),
            ]),
            &Some(HashSet::from([HeaderField::from_bytes(b"Date")?])),
        );
        assert!(result.is_ok());

        let s = String::from_utf8(writer).expect("no utf8");
        assert!(s.contains("Server:"), "{}", s);
        assert!(s.contains("Date:"), "{}", s);

        let mut writer = Vec::new();
        let result = write_message_header(
            &mut writer,
            HttpVersion::Version1_1,
            StatusCode(200),
            &[],
            &Some(vec![
                DateHeader::current(),
                Header::from_str("Server: tiny-http").unwrap(),
            ]),
            &Some(HashSet::from([HeaderField::from_str("Server")?])),
        );
        assert!(result.is_ok());

        let s = String::from_utf8(writer).expect("no utf8");
        assert!(!s.contains("Server:"), "{}", s);
        Ok(())
    }
}
