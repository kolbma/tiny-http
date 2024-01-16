use std::io::Read;
use std::io::Result as IoResult;
use std::sync::mpsc::Sender;

/// A `Reader` that reads exactly the number of bytes from a sub-reader.
///
/// If the limit is reached, it returns EOF. If the limit is not reached
/// when the destructor is called, the remaining bytes will be read and
/// thrown away.
pub struct EqualReader<R>
where
    R: Read,
{
    reader: R,
    size: usize,
    last_read_signal: Option<Sender<IoResult<()>>>,
}

impl<R> EqualReader<R>
where
    R: Read,
{
    pub fn new(reader: R, size: usize, tx: Option<Sender<IoResult<()>>>) -> Self {
        Self {
            reader,
            size,
            last_read_signal: tx,
        }
    }
}

impl<R> Read for EqualReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if self.size == 0 {
            return Ok(0);
        }

        let buf = if buf.len() < self.size {
            buf
        } else {
            &mut buf[..self.size]
        };

        let len = self.reader.read(buf)?;
        self.size -= len;
        Ok(len)
    }
}

impl<R> Drop for EqualReader<R>
where
    R: Read,
{
    fn drop(&mut self) {
        if self.size == 0 {
            return;
        }

        let mut buf = &mut [0u8; 256][..];
        if self.size < 256 {
            buf = &mut buf[..self.size];
        }

        while self.size > 0 {
            match self.reader.read(buf) {
                Ok(0) => {
                    if let Some(last_read_signal) = &self.last_read_signal {
                        last_read_signal.send(Ok(())).ok();
                    }
                    break;
                }
                Ok(nr_bytes) => self.size -= nr_bytes,
                Err(e) => {
                    if let Some(last_read_signal) = &self.last_read_signal {
                        last_read_signal.send(Err(e)).ok();
                    }
                    break;
                }
            }

            if self.size < 256 {
                buf = &mut buf[..self.size];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EqualReader;
    use std::io::{Cursor, Read};

    #[test]
    fn test_limit() {
        let mut org_reader = Cursor::new("hello world".to_string().into_bytes());

        {
            let mut equal_reader = EqualReader::new(org_reader.by_ref(), 5, None);

            let mut string = String::new();
            equal_reader.read_to_string(&mut string).unwrap();
            assert_eq!(string, "hello");
        }

        let mut string = String::new();
        org_reader.read_to_string(&mut string).unwrap();
        assert_eq!(string, " world");
    }

    #[test]
    fn test_equal_reader_drop() {
        let data = b"hello world";
        let reader = Cursor::new(data);
        let mut string = String::new();

        let mut equal_reader = EqualReader::new(reader.clone(), 5, None);
        equal_reader.read_to_string(&mut string).unwrap();
        assert_eq!(string, "hello");

        string.clear();
        equal_reader.read_to_string(&mut string).unwrap();
        assert_eq!(string.len(), 0);
        drop(equal_reader);

        let mut equal_reader = EqualReader::new(reader.clone(), data.len() + 1, None);
        string.clear();
        equal_reader.read_to_string(&mut string).unwrap();
        assert_eq!(string.len(), data.len());
        drop(equal_reader);

        let equal_reader = EqualReader::new(reader.clone(), data.len() + 1, None);
        assert_eq!(equal_reader.size, data.len() + 1);
        drop(equal_reader);

        let mut equal_reader = EqualReader::new(reader.clone(), 0, None);
        string.clear();
        equal_reader.read_to_string(&mut string).unwrap();
        assert_eq!(string.len(), 0);
        drop(equal_reader);
    }

    #[test]
    fn test_not_enough() {
        let mut org_reader = Cursor::new("hello world".to_string().into_bytes());

        {
            let mut equal_reader = EqualReader::new(org_reader.by_ref(), 5, None);

            let mut vec = [0];
            equal_reader.read_exact(&mut vec).unwrap();
            assert_eq!(vec[0], b'h');
        }

        let mut string = String::new();
        org_reader.read_to_string(&mut string).unwrap();
        assert_eq!(string, " world");
    }
}
