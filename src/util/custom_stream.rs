use std::io::{Read, Result as IoResult, Write};

use crate::stream_traits::ReadTimeout;

pub(crate) struct CustomStream<R, W> {
    reader: R,
    writer: W,
}

impl<R, W> CustomStream<R, W>
where
    R: Read,
    W: Write,
{
    pub(crate) fn new(reader: R, writer: W) -> CustomStream<R, W> {
        CustomStream { reader, writer }
    }
}

impl<R, W> Read for CustomStream<R, W>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.reader.read(buf)
    }
}

impl<R, W> Write for CustomStream<R, W>
where
    W: Write,
{
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.writer.flush()
    }
}

impl<R, W> ReadTimeout for CustomStream<R, W>
where
    R: Read + ReadTimeout,
    W: Write,
{
    fn read_timeout(&self) -> IoResult<Option<std::time::Duration>> {
        self.reader.read_timeout()
    }

    fn set_read_timeout(&mut self, dur: Option<std::time::Duration>) -> IoResult<()> {
        self.reader.set_read_timeout(dur)
    }
}
