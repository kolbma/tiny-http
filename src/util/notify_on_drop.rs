use std::{
    io::{Read, Result as IoResult, Write},
    sync::mpsc::Sender,
};

pub(crate) struct NotifyOnDrop<R> {
    pub(crate) sender: Sender<()>,
    pub(crate) inner: R,
}

impl<R: Read> Read for NotifyOnDrop<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.inner.read(buf)
    }
}
impl<R: Write> Write for NotifyOnDrop<R> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> IoResult<()> {
        self.inner.flush()
    }
}
impl<R> Drop for NotifyOnDrop<R> {
    fn drop(&mut self) {
        self.sender.send(()).unwrap();
    }
}
