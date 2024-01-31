use std::str::FromStr;

pub(crate) use custom_stream::CustomStream;
pub(crate) use equal_reader::EqualReader;
pub(crate) use fused_reader::FusedReader;
pub(crate) use message::Message;
pub(crate) use messages_queue::MessagesQueue;
pub(crate) use notify_on_drop::NotifyOnDrop;
pub(crate) use refined_tcp_stream::RefinedTcpStream;
pub(crate) use registration::Registration;
pub(crate) use sequential::{SequentialReader, SequentialReaderBuilder, SequentialWriterBuilder};
pub(crate) use task_pool::TaskPool;

use crate::HeaderFieldValue;

mod custom_stream;
mod equal_reader;
mod fused_reader;
mod message;
mod messages_queue;
mod notify_on_drop;
pub(crate) mod refined_tcp_stream;
pub(crate) mod registration;
mod sequential;
mod task_pool;

/// Parses a the value of a header.
/// Suitable for `Accept-*`, `TE`, etc.
///
/// For example with `text/plain, image/png; q=1.5` this function would
/// return `[ ("text/plain", 1.0), ("image/png", 1.5) ]`
pub(crate) fn parse_header_value(value: &HeaderFieldValue) -> Vec<(&[u8], f32)> {
    value
        .split(ascii::AsciiChar::Comma)
        .filter_map(|elem| {
            let mut params = elem.split(ascii::AsciiChar::Semicolon);

            let t = params.next()?;

            let mut value = 1.0_f32;

            for p in params {
                let p_trim = p.trim_start();
                if p_trim.as_bytes()[..2] == [b'q', b'='] {
                    if let Ok(val) = f32::from_str(p_trim[2..].trim().as_str()) {
                        value = val;
                        break;
                    }
                }
            }

            Some((t.trim().as_bytes(), value))
        })
        .collect()
}

#[cfg(test)]
mod test {

    use crate::HeaderFieldValue;

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_parse_header() {
        let hfv = "text/html, text/plain; q=1.5 , image/png ; q=2.0"
            .parse::<HeaderFieldValue>()
            .unwrap();
        let result = super::parse_header_value(&hfv);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, b"text/html");
        assert_eq!(result[0].1, 1.0);
        assert_eq!(result[1].0, b"text/plain");
        assert_eq!(result[1].1, 1.5);
        assert_eq!(result[2].0, b"image/png");
        assert_eq!(result[2].1, 2.0);
    }
}
