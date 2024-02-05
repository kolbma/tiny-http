use std::cell::{Cell, RefCell};
use std::hash::Hasher;

use ahash::AHasher;

use crate::common::Header;

/// Container for [`Header`] data for cached keyed lookup of fields
#[derive(Debug)]
pub(crate) struct HeaderData {
    /// Contains the hash of header fields in same order as in headers
    field_hash: RefCell<Vec<u64>>,
    /// Counts 0 when all headers are cached
    field_ignore_cnt: Cell<usize>,
    /// Contains same unstable sorted field hashes in .0 and the header index belonging to the field
    #[allow(clippy::type_complexity)]
    field_map: RefCell<(Vec<u64>, Vec<Vec<usize>>)>,
    headers: Vec<Header>,
}

/// Creates hash for `field`
macro_rules! field_hash {
    ($field:expr) => {{
        let mut hasher = AHasher::default();
        for b in $field {
            let mut b = *b;
            #[allow(clippy::manual_range_contains)]
            {
                if b >= 65 && b <= 90 {
                    b += 32;
                }
            }
            hasher.write_u8(b);
        }
        hasher.finish()
    }};
}

impl HeaderData {
    /// Move the `Vec<Header>` in to create [`HeaderData`]
    #[must_use]
    pub(crate) fn new(headers: Vec<Header>) -> Self {
        Self {
            field_hash: RefCell::new(vec![0; headers.len()]),
            field_ignore_cnt: Cell::new(headers.len()),
            field_map: RefCell::new((Vec::new(), Vec::new())),
            headers,
        }
    }

    /// Prepares cache for multiple fields for faster retrieve
    pub(crate) fn cache_header<B>(&self, fields: &[B])
    where
        B: AsRef<[u8]> + Into<Vec<u8>>,
    {
        if self.field_ignore_cnt.get() == 0 {
            return;
        }

        let mut cache_hash = Vec::new();
        for field in fields {
            let field = field_hash!(field.as_ref());
            cache_hash.push(field);
        }
        cache_hash.sort_unstable();

        let mut field_hash = self.field_hash.borrow_mut();
        let mut field_map = self.field_map.borrow_mut();

        for (idx, header) in self.headers.iter().enumerate() {
            if field_hash[idx] != 0 {
                continue;
            }

            let header_field = field_hash!(header.field.as_bytes());
            if cache_hash.binary_search(&header_field).is_ok() {
                match field_map.0.binary_search(&header_field) {
                    Ok(i) => {
                        debug_assert_eq!(header_field, field_map.0[i]);
                        field_map.1[i].push(idx);
                    }
                    Err(i) => {
                        field_map.0.insert(i, header_field);
                        field_map.1.insert(i, vec![idx]);
                    }
                }
                field_hash[idx] = header_field;
                self.field_ignore_cnt.set(self.field_ignore_cnt.get() - 1);
            }
        }
    }

    /// Get up to `limit` headers provided with `field`
    ///
    /// A [`Request`](crate::Request) can be made with multiple lines of the same
    /// header field.  
    /// This is equivalent to providing a comma separated list in one
    /// header field.
    ///
    /// Up to `limit` lines with `field` are returned. It can be less if the header
    /// has lesser.
    ///
    /// If there is no such header `field` available in `Request` `None` is returned.
    ///
    pub(crate) fn header<B>(&self, field: &B, limit: Option<usize>) -> Option<Vec<&Header>>
    where
        B: AsRef<[u8]> + Into<Vec<u8>>,
    {
        let field = field_hash!(field.as_ref());
        let limit = limit.unwrap_or(usize::MAX);

        for n in 0..2_u8 {
            let field_map = self.field_map.borrow();
            if let Ok(idx) = field_map.0.binary_search(&field) {
                let mut hv = Vec::new();
                for i in field_map.1[idx].iter().take(limit) {
                    hv.push(&self.headers[*i]);
                }
                return Some(hv);
            }

            if self.field_ignore_cnt.get() == 0 || n == 1 {
                break;
            }

            drop(field_map);

            self.cache_header_field(field);
        }

        None
    }

    /// Get the first header provided with `field`
    ///
    /// A [`Request`](crate::Request) can be made with multiple lines of the same header field.  
    /// This is equivalent to providing a comma separated list in one
    /// header field.
    ///
    /// If there is no such header `field` available in `Request` `None` is returned.
    ///
    #[inline]
    pub(crate) fn header_first<B>(&self, field: &B) -> Option<&Header>
    where
        B: AsRef<[u8]> + Into<Vec<u8>>,
    {
        self.header(field, Some(1)).map(|h| h[0])
    }

    /// Get the last header provided with `field`
    ///
    /// See also [`Self::header_first`].
    ///
    /// A [`Request`] can be made with multiple lines of the same header field.  
    /// This is equivalent to providing a comma separated list in one
    /// header field.
    ///
    /// If there is no such header `field` available in `Request` `None` is returned.
    ///
    #[inline]
    pub(crate) fn header_last<B>(&self, field: &B) -> Option<&Header>
    where
        B: AsRef<[u8]> + Into<Vec<u8>>,
    {
        self.header(field, None).and_then(|h| h.last().copied())
    }

    /// Returns the list of [`Header`] sent by client in [`Request`](crate::Request)
    #[inline]
    pub(crate) fn headers(&self) -> &[Header] {
        &self.headers
    }

    #[inline]
    fn cache_header_field(&self, field: u64) {
        let mut field_hash = self.field_hash.borrow_mut();
        let mut field_map = self.field_map.borrow_mut();

        for (idx, header) in self.headers.iter().enumerate() {
            if field_hash[idx] != 0 {
                continue;
            }

            let header_field = field_hash!(header.field.as_bytes());
            if header_field == field {
                match field_map.0.binary_search(&header_field) {
                    Ok(i) => {
                        debug_assert_eq!(header_field, field_map.0[i]);
                        field_map.1[i].push(idx);
                    }
                    Err(i) => {
                        field_map.0.insert(i, header_field);
                        field_map.1.insert(i, vec![idx]);
                    }
                }
                field_hash[idx] = header_field;
                self.field_ignore_cnt.set(self.field_ignore_cnt.get() - 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    use crate::common::Header;

    #[test]
    fn cache_header_field_test() {
        let headers = vec![
            Header::from_bytes(b"Host", b"localhost").unwrap(),
            Header::from_bytes(b"Content-Length", b"69").unwrap(),
            Header::from_bytes(b"Content-Type", b"text/html").unwrap(),
            Header::from_bytes(b"X-Data", b"1").unwrap(),
            Header::from_bytes(b"x-data", b"2").unwrap(),
            Header::from_bytes(b"X-Data", b"3").unwrap(),
        ];

        let data = HeaderData::new(headers);

        assert_eq!(data.headers().len(), 6);

        let field = field_hash!(b"Content-Length");
        data.cache_header_field(field);

        assert_eq!(data.field_ignore_cnt.get(), 5);
        assert_eq!(data.field_hash.borrow()[1], field);
        assert_eq!(*data.field_map.borrow().0.last().unwrap(), field);
        let v = data.field_map.borrow().1.last().unwrap().clone();
        assert_eq!(data.headers[*v.last().unwrap()].field, "Content-Length");
        assert_eq!(data.headers[*v.last().unwrap()].value, "69");
    }

    #[test]
    fn cache_header_multi_field_test() {
        let headers = vec![
            Header::from_bytes(b"Host", b"localhost").unwrap(),
            Header::from_bytes(b"Content-Length", b"69").unwrap(),
            Header::from_bytes(b"Content-Type", b"text/html").unwrap(),
            Header::from_bytes(b"X-Data", b"1").unwrap(),
            Header::from_bytes(b"x-data", b"2").unwrap(),
            Header::from_bytes(b"X-Data", b"3").unwrap(),
        ];

        let data = HeaderData::new(headers);

        assert_eq!(data.headers().len(), 6);

        let field = field_hash!(b"X-Data");
        data.cache_header_field(field);

        assert_eq!(data.field_ignore_cnt.get(), 3);
        assert_eq!(data.field_hash.borrow()[3], field);
        assert_eq!(data.field_hash.borrow()[4], field);
        assert_eq!(data.field_hash.borrow()[5], field);
        assert_eq!(*data.field_map.borrow().0.last().unwrap(), field);
        let v = data.field_map.borrow().1.last().unwrap().clone();
        assert_eq!(v.len(), 3);
        assert_eq!(v, vec![3, 4, 5]);

        let _ct = data.header_first(b"Content-Type").unwrap();
        assert_eq!(data.field_ignore_cnt.get(), 2);
        assert_ne!(data.field_hash.borrow()[2], 0);
        assert_eq!(data.field_map.borrow().0.len(), 2);

        assert_eq!(data.header_first(b"Not-Exist"), None);
        assert_eq!(data.field_ignore_cnt.get(), 2);

        assert!(data.header_first(b"Host").is_some());
        assert_eq!(data.field_ignore_cnt.get(), 1);
        assert!(data.header_first(b"Content-Length").is_some());
        assert_eq!(data.field_ignore_cnt.get(), 0);

        assert!(data.header_first(b"X-Data").is_some());

        assert_eq!(data.field_map.borrow().0.len(), 4);
    }

    #[test]
    fn header_data_test() {
        let headers = vec![
            Header::from_bytes(b"Host", b"localhost").unwrap(),
            Header::from_bytes(b"Content-Length", b"69").unwrap(),
            Header::from_bytes(b"Content-Type", b"text/html").unwrap(),
            Header::from_bytes(b"X-Data", b"1").unwrap(),
            Header::from_bytes(b"x-data", b"2").unwrap(),
            Header::from_bytes(b"X-Data", b"3").unwrap(),
        ];

        let data = HeaderData::new(headers);

        assert_eq!(data.headers().len(), 6);

        let now = Instant::now();
        let r1 = data.header(b"X-Data", Some(2));
        let elaps1 = now.elapsed();

        let now = Instant::now();
        let r2 = data.header(b"X-Data", Some(2));
        let elaps2 = now.elapsed();

        assert!(
            elaps1 > elaps2,
            "elaps1: {} elaps2: {}",
            elaps1.as_nanos(),
            elaps2.as_nanos()
        );
        assert_eq!(r1, r2);

        assert_eq!(r1.unwrap().len(), 2);
        assert_eq!(r2.unwrap().len(), 2);

        let r3 = data.header(b"content-type", None);
        let r3 = r3.unwrap();
        assert_eq!(r3.len(), 1);
        assert_eq!(r3[0].field.as_bytes(), b"Content-Type");

        let now = Instant::now();
        let r4 = data.header(b"X-Data", None);
        let elaps4 = now.elapsed();

        assert_eq!(r4.unwrap().len(), 3);

        assert!(
            elaps1 > elaps4,
            "elaps1: {} elaps4: {}",
            elaps1.as_nanos(),
            elaps4.as_nanos()
        );
    }

    #[inline(never)]
    fn find_header<'a>(headers: &'a [Header], f: &[u8]) -> Vec<&'a Header> {
        let mut v = Vec::new();
        for h in headers {
            if h.field == f {
                v.push(h);
            }
        }
        v
    }

    #[test]
    fn header_data_speed_test() {
        let headers = vec![
            Header::from_bytes(b"Host", b"localhost").unwrap(),
            Header::from_bytes(b"Content-Length", b"69").unwrap(),
            Header::from_bytes(b"Content-Type", b"text/html").unwrap(),
            Header::from_bytes(b"X-Data", b"1").unwrap(),
            Header::from_bytes(b"x-data", b"2").unwrap(),
            Header::from_bytes(b"X-Data", b"3").unwrap(),
        ];
        let headers2 = headers.clone();

        let rounds = 500;

        let mut elaps1 = Duration::ZERO;

        let fields = [&b"Host"[..], b"Content-Length", b"Content-Type", b"X-Data"];
        let mut hv1 = Vec::new();

        for n in 0..rounds {
            for f in fields {
                let now = Instant::now();
                hv1.push(find_header(&headers, f));
                elaps1 += now.elapsed();
            }

            assert_eq!(hv1[n * 4].len(), 1);
            assert_eq!(hv1[1 + n * 4].len(), 1);
            assert_eq!(hv1[2 + n * 4].len(), 1);
            assert_eq!(hv1[3 + n * 4].len(), 3);
            assert_eq!(hv1.len(), 4 + n * 4);
        }

        let mut elaps2 = Duration::ZERO;

        let data = HeaderData::new(headers2);
        let fields = [&b"Host"[..], b"Content-Length", b"Content-Type", b"X-Data"];
        data.cache_header(&fields);

        let mut hv2 = Vec::new();

        for n in 0..rounds {
            for f in fields {
                let now = Instant::now();
                hv2.push(data.header(&f, None).unwrap());
                elaps2 += now.elapsed();
            }

            assert_eq!(hv2[n * 4].len(), 1);
            assert_eq!(hv2[1 + n * 4].len(), 1);
            assert_eq!(hv2[2 + n * 4].len(), 1);
            assert_eq!(hv2[3 + n * 4].len(), 3);
            assert_eq!(hv2.len(), 4 + n * 4);
        }

        assert_eq!(hv1.len(), hv2.len());

        let elaps_range = (elaps1 - (elaps1 / 100 * 2))..(elaps1 + (elaps1 / 100 * 2));

        assert!(
            elaps1 > elaps2 || elaps_range.contains(&elaps2),
            "elaps1: {} elaps2: {}",
            elaps1.as_nanos(),
            elaps2.as_nanos()
        );
    }
}
