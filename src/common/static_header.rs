use lazy_static::lazy_static;

use crate::{Header, HeaderField, HeaderFieldValue};

lazy_static! {
    pub(crate) static ref CONNECTION_HEADER: Header = Header {
        field: CONNECTION_HEADER_FIELD.clone(),
        value: HeaderFieldValue::from_bytes(b"").unwrap()
    };
    pub(crate) static ref CONNECTION_HEADER_FIELD: HeaderField =
        HeaderField::from_bytes(b"Connection").unwrap();
    pub(crate) static ref CONTENT_LENGTH_HEADER: Header = Header {
        field: CONTENT_LENGTH_HEADER_FIELD.clone(),
        value: HeaderFieldValue::from_bytes(b"").unwrap()
    };
    pub(crate) static ref CONTENT_LENGTH_HEADER_FIELD: HeaderField =
        HeaderField::from_bytes(b"Content-Length").unwrap();
    pub(crate) static ref CONTENT_TYPE_HEADER_FIELD: HeaderField =
        HeaderField::from_bytes(b"Content-Type").unwrap();
    pub(crate) static ref TE_CHUNKED_HEADER: Header =
        Header::from_bytes(b"Transfer-Encoding", b"chunked").unwrap();
}
