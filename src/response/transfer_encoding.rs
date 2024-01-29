/// Transfer encoding to use when sending the message.
/// Note that only *supported* encoding are listed here.
#[derive(Copy, Clone)]
pub(super) enum TransferEncoding {
    Identity,
    Chunked,
}

impl std::str::FromStr for TransferEncoding {
    type Err = ();

    fn from_str(input: &str) -> Result<TransferEncoding, ()> {
        if input.eq_ignore_ascii_case("identity") {
            Ok(TransferEncoding::Identity)
        } else if input.eq_ignore_ascii_case("chunked") {
            Ok(TransferEncoding::Chunked)
        } else {
            Err(())
        }
    }
}
