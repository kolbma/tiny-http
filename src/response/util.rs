use std::collections::HashSet;
use std::hash::Hash;
use std::io::{Result as IoResult, Write};
use std::{cmp::Ordering, str::FromStr};

use crate::{Header, HeaderField, HttpVersion, StatusCode};

use super::date_header::DateHeader;
use super::transfer_encoding::TransferEncoding;

pub(super) fn choose_transfer_encoding(
    status_code: StatusCode,
    request_headers: &[Header],
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

    // parsing the request's TE header
    let user_request = request_headers
        .iter()
        // finding TE and get value
        .find_map(|h| {
            // getting the corresponding TransferEncoding
            if h.field.equiv("TE") {
                // getting list of requested elements
                let mut parse = util::parse_header_value(h.value.as_str()); // TODO: remove conversion

                // sorting elements by most priority
                parse.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

                // trying to parse each requested encoding
                for value in parse {
                    // q=0 are ignored
                    if value.1 <= 0.0 {
                        continue;
                    }

                    if let Ok(te) = TransferEncoding::from_str(value.0) {
                        return Some(te);
                    }
                }
            }

            // No transfer encoding found
            None
        });

    if let Some(user_request) = user_request {
        return user_request;
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
    matches!(status_code.into().0, 100..=199 | 204 | 304) // status code 1xx, 204, 205 and 304 MUST not include a body
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

/// preparing headers for transfer
#[inline]
pub(super) fn update_te_headers(
    headers: &mut Vec<Header>,
    transfer_encoding: Option<TransferEncoding>,
    data_length: &Option<usize>,
) {
    match transfer_encoding {
        Some(TransferEncoding::Chunked) => {
            headers.push(Header::from_bytes(b"Transfer-Encoding", b"chunked").unwrap());
        }
        Some(TransferEncoding::Identity) => {
            assert!(data_length.is_some());
            let data_length = data_length.unwrap();

            headers.push(
                Header::from_bytes(b"Content-Length", data_length.to_string().as_bytes()).unwrap(),
            );
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
    // writing status line
    write!(
        writer,
        "{} {} {}\r\n",
        http_version.header(),
        status_code.0,
        status_code.default_reason_phrase()
    )?;

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

        writer.write_all(header.field.as_str().as_bytes())?;
        write!(writer, ": ")?;
        writer.write_all(header.value.as_str().as_bytes())?;
        write!(writer, "\r\n")?;
    }

    // separator between header and data
    write!(writer, "\r\n")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, str::FromStr};

    use crate::{common::HeaderError, Header, HeaderField, HttpVersion};

    use super::*;

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
            &Some(HashSet::from([
                HeaderField::from_bytes(b"Date").map_err(|_| HeaderError)?
            ])),
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
