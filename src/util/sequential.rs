use std::io::{Error as IoError, ErrorKind as IoErrorKind, Read, Result as IoResult, Write};
use std::mem;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::stream_traits::ReadTimeout;

pub(crate) struct SequentialReaderBuilder<R>
where
    R: Read + Send,
{
    inner: SequentialReaderBuilderInner<R>,
}

enum SequentialReaderBuilderInner<R>
where
    R: Read + Send,
{
    First(R),
    NotFirst(Receiver<R>),
}

pub(crate) struct SequentialReader<R>
where
    R: Read + Send,
{
    inner: SequentialReaderInner<R>,
    next: Sender<R>,
}

enum SequentialReaderInner<R>
where
    R: Read + Send,
{
    MyTurn(R),
    Waiting(Receiver<R>),
    Empty,
}

pub(crate) struct SequentialWriterBuilder<W>
where
    W: Write + Send,
{
    writer: Arc<Mutex<W>>,
    next_trigger: Option<Receiver<()>>,
}

pub(crate) struct SequentialWriter<W>
where
    W: Write + Send,
{
    trigger: Option<Receiver<()>>,
    writer: Arc<Mutex<W>>,
    on_finish: Sender<()>,
}

impl<R: Read + Send> SequentialReaderBuilder<R> {
    pub(crate) fn new(reader: R) -> SequentialReaderBuilder<R> {
        SequentialReaderBuilder {
            inner: SequentialReaderBuilderInner::First(reader),
        }
    }
}

impl<W: Write + Send> SequentialWriterBuilder<W> {
    pub(crate) fn new(writer: W) -> SequentialWriterBuilder<W> {
        SequentialWriterBuilder {
            writer: Arc::new(Mutex::new(writer)),
            next_trigger: None,
        }
    }
}

impl<R: Read + Send> Iterator for SequentialReaderBuilder<R> {
    type Item = SequentialReader<R>;

    fn next(&mut self) -> Option<SequentialReader<R>> {
        let (tx, rx) = mpsc::channel();

        let inner = mem::replace(&mut self.inner, SequentialReaderBuilderInner::NotFirst(rx));

        match inner {
            SequentialReaderBuilderInner::First(reader) => Some(SequentialReader {
                inner: SequentialReaderInner::MyTurn(reader),
                next: tx,
            }),

            SequentialReaderBuilderInner::NotFirst(previous) => Some(SequentialReader {
                inner: SequentialReaderInner::Waiting(previous),
                next: tx,
            }),
        }
    }
}

impl<W: Write + Send> Iterator for SequentialWriterBuilder<W> {
    type Item = SequentialWriter<W>;
    fn next(&mut self) -> Option<SequentialWriter<W>> {
        let (tx, rx) = mpsc::channel();
        let mut next_next_trigger = Some(rx);
        mem::swap(&mut next_next_trigger, &mut self.next_trigger);

        Some(SequentialWriter {
            trigger: next_next_trigger,
            writer: self.writer.clone(),
            on_finish: tx,
        })
    }
}

impl<R: Read + ReadTimeout + Send> Read for SequentialReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let mut reader = match self.inner {
            SequentialReaderInner::MyTurn(ref mut reader) => return reader.read(buf),
            SequentialReaderInner::Waiting(ref mut recv) => recv.recv().unwrap(),
            SequentialReaderInner::Empty => unreachable!(),
        };

        let result = reader.read(buf);
        self.inner = SequentialReaderInner::MyTurn(reader);
        result
    }
}

impl<W: Write + Send> Write for SequentialWriter<W> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        if let Some(v) = self.trigger.as_mut() {
            v.recv().unwrap();
        }
        self.trigger = None;

        self.writer.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> IoResult<()> {
        if let Some(v) = self.trigger.as_mut() {
            v.recv().unwrap();
        }
        self.trigger = None;

        self.writer.lock().unwrap().flush()
    }
}

impl<R> Drop for SequentialReader<R>
where
    R: Read + Send,
{
    fn drop(&mut self) {
        let inner = mem::replace(&mut self.inner, SequentialReaderInner::Empty);

        match inner {
            SequentialReaderInner::MyTurn(reader) => {
                let _ = self.next.send(reader);
            }
            SequentialReaderInner::Waiting(recv) => {
                if let Ok(reader) = recv.recv() {
                    let _ = self.next.send(reader);
                }
            }
            SequentialReaderInner::Empty => (),
        }
    }
}

impl<W> Drop for SequentialWriter<W>
where
    W: Write + Send,
{
    fn drop(&mut self) {
        let _ = self.on_finish.send(());
    }
}

impl<R> ReadTimeout for SequentialReader<R>
where
    R: Read + ReadTimeout + Send,
{
    fn read_timeout(&self) -> IoResult<Option<std::time::Duration>> {
        match &self.inner {
            SequentialReaderInner::MyTurn(reader) => reader.read_timeout(),
            SequentialReaderInner::Waiting(recv) => recv
                .try_recv()
                .map_err(|err| IoError::new(IoErrorKind::WouldBlock, err.to_string()))?
                .read_timeout(),
            SequentialReaderInner::Empty => Ok(None),
        }
    }

    fn set_read_timeout(&mut self, dur: Option<std::time::Duration>) -> IoResult<()> {
        match &mut self.inner {
            SequentialReaderInner::MyTurn(reader) => reader.set_read_timeout(dur),
            SequentialReaderInner::Waiting(recv) => recv
                .try_recv()
                .map_err(|err| IoError::new(IoErrorKind::WouldBlock, err.to_string()))?
                .set_read_timeout(dur),
            SequentialReaderInner::Empty => Ok(()),
        }
    }
}
