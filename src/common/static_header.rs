use ascii::AsciiString;
use lazy_static::lazy_static;

use crate::{Header, HeaderField};

lazy_static! {
    pub(crate) static ref CONTENT_LENGTH_HEADER: Header = Header {
        field: CONTENT_LENGTH_HEADER_FIELD.clone(),
        value: AsciiString::new()
    };
    pub(crate) static ref CONTENT_LENGTH_HEADER_FIELD: HeaderField =
        HeaderField::from_bytes(&b"Content-Length"[..]).unwrap();
    pub(crate) static ref CONTENT_TYPE_HEADER_FIELD: HeaderField =
        HeaderField::from_bytes(&b"Content-Type"[..]).unwrap();
    pub(crate) static ref TE_CHUNKED_HEADER: Header =
        Header::from_bytes(&b"Transfer-Encoding"[..], &b"chunked"[..]).unwrap();
}
