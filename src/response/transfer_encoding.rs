use std::convert::TryFrom;

use ascii::AsciiStr;

/// Transfer encoding to use when sending the message.
/// Note that only *supported* encoding are listed here.
#[derive(Copy, Clone)]
pub(super) enum TransferEncoding {
    Identity,
    Chunked,
}

impl TryFrom<&[u8]> for TransferEncoding {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.eq_ignore_ascii_case(b"identity") {
            Ok(TransferEncoding::Identity)
        } else if value.eq_ignore_ascii_case(b"chunked") {
            Ok(TransferEncoding::Chunked)
        } else {
            Err(())
        }
    }
}

impl std::str::FromStr for TransferEncoding {
    type Err = ();

    fn from_str(value: &str) -> Result<TransferEncoding, ()> {
        let value = value.as_bytes();
        Self::try_from(value)
    }
}

impl TryFrom<&AsciiStr> for TransferEncoding {
    type Error = ();

    fn try_from(value: &AsciiStr) -> Result<Self, Self::Error> {
        let value = value.as_bytes();
        Self::try_from(value)
    }
}
