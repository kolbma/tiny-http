//! Traits for handling streams of [`Request`](crate::Request)

use std::{
    io::{Cursor, Error as IoError, ErrorKind as IoErrorKind, Read, Result as IoResult, Write},
    time::Duration,
};

use crate::util::NotifyOnDrop;

/// Trait combining the `Read` and `Write` traits
///
/// Automatically implemented on all types that implement both `Read` and `Write`
pub trait ReadWrite: Read + Write {}
impl<T> ReadWrite for T where T: Read + Write {}

/// Trait marking available `read_timeout()` and `set_read_timeout()` on streams
pub trait ReadTimeout {
    /// Returns the read timeout of this socket.
    ///
    /// If the timeout is [`None`], then [`read`] calls will block indefinitely.
    ///
    /// # Platform-specific behavior
    ///
    /// Some platforms do not provide access to the current timeout.
    ///
    /// See also [`TcpStream::read_timeout`].
    ///
    /// [`read`]: Read::read
    /// [`TcpStream::read_timeout`]: std::net::TcpStream::read_timeout
    ///
    #[allow(clippy::missing_errors_doc)]
    fn read_timeout(&self) -> IoResult<Option<Duration>>;

    /// Sets the read timeout to the timeout specified.
    ///
    /// If the value specified is [`None`], then [`read`] calls will block
    /// indefinitely. An [`Err`] is returned if the zero [`Duration`] is
    /// passed to this method.
    ///
    /// # Platform-specific behavior
    ///
    /// Platforms may return a different error code whenever a read times out as
    /// a result of setting this option. For example Unix typically returns an
    /// error of the kind [`WouldBlock`], but Windows may return [`TimedOut`].
    ///
    /// See also [`TcpStream::set_read_timeout`].
    ///
    /// [`Duration`]: std::time::Duration
    /// [`read`]: Read::read
    /// [`TcpStream::set_read_timeout`]: std::net::TcpStream::set_read_timeout
    /// [`TimedOut`]: std::io::ErrorKind::TimedOut
    /// [`WouldBlock`]: std::io::ErrorKind::WouldBlock
    ///
    #[allow(clippy::missing_errors_doc)]
    fn set_read_timeout(&mut self, dur: Option<Duration>) -> IoResult<()>;
}

/// Trait combining the `Read` and [`ReadTimeout`] traits
///
/// Automatically implemented on all types that implement both `Read` and
pub trait DataRead: Read + ReadTimeout {}
impl<T> DataRead for T where T: Read + ReadTimeout {}

/// Trait combining the `Read`, [`ReadTimeout`] and `Write` a traits
///
/// Automatically implemented on all types that implements `Read`, `ReadTimeout`, `Write`
pub trait DataReadWrite: Read + Write + ReadTimeout {}
impl<T> DataReadWrite for T where T: Read + ReadTimeout + Write {}

impl ReadTimeout for Box<dyn DataRead + Send> {
    fn read_timeout(&self) -> IoResult<Option<Duration>> {
        self.as_ref().read_timeout()
    }

    fn set_read_timeout(&mut self, dur: Option<Duration>) -> IoResult<()> {
        self.as_mut().set_read_timeout(dur)
    }
}

impl<R> ReadTimeout for NotifyOnDrop<R>
where
    R: Read + ReadTimeout,
{
    fn read_timeout(&self) -> IoResult<Option<Duration>> {
        self.inner.read_timeout()
    }

    fn set_read_timeout(&mut self, dur: Option<Duration>) -> IoResult<()> {
        self.inner.set_read_timeout(dur)
    }
}

impl<R> ReadTimeout for std::io::BufReader<R>
where
    R: Read + ReadTimeout,
{
    fn read_timeout(&self) -> IoResult<Option<Duration>> {
        self.get_ref().read_timeout()
    }

    fn set_read_timeout(&mut self, dur: Option<Duration>) -> IoResult<()> {
        self.get_mut().set_read_timeout(dur)
    }
}

impl ReadTimeout for std::io::Empty {
    fn read_timeout(&self) -> IoResult<Option<Duration>> {
        Err(IoError::new(IoErrorKind::Unsupported, "no timeout"))
    }

    fn set_read_timeout(&mut self, _dur: Option<Duration>) -> IoResult<()> {
        Err(IoError::new(IoErrorKind::Unsupported, "no timeout"))
    }
}

impl ReadTimeout for &[u8] {
    fn read_timeout(&self) -> IoResult<Option<Duration>> {
        Err(IoError::new(IoErrorKind::Unsupported, "no timeout"))
    }

    fn set_read_timeout(&mut self, _dur: Option<Duration>) -> IoResult<()> {
        Err(IoError::new(IoErrorKind::Unsupported, "no timeout"))
    }
}

impl ReadTimeout for Cursor<Vec<u8>> {
    fn read_timeout(&self) -> IoResult<Option<Duration>> {
        Err(IoError::new(IoErrorKind::Unsupported, "no timeout"))
    }

    /// Always `Ok(())`
    fn set_read_timeout(&mut self, _dur: Option<Duration>) -> IoResult<()> {
        Err(IoError::new(IoErrorKind::Unsupported, "no timeout"))
    }
}

impl<R> ReadTimeout for chunked_transfer::Decoder<R>
where
    R: Read + ReadTimeout,
{
    fn read_timeout(&self) -> IoResult<Option<Duration>> {
        self.get_ref().read_timeout()
    }

    fn set_read_timeout(&mut self, dur: Option<Duration>) -> IoResult<()> {
        self.get_mut().set_read_timeout(dur)
    }
}
