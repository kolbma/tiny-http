use std::io::{Read, Result as IoResult, Write};
use std::net::{Shutdown, SocketAddr};

#[cfg(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
))]
use crate::ssl::SslStream;
use crate::stream_traits::ReadTimeout;
use crate::ConnectionStream;

pub(crate) enum Stream {
    Http(ConnectionStream),
    #[cfg(any(
        feature = "ssl-openssl",
        feature = "ssl-rustls",
        feature = "ssl-native-tls"
    ))]
    Https(SslStream),
}

impl Clone for Stream {
    fn clone(&self) -> Self {
        match self {
            Stream::Http(tcp_stream) => Stream::Http(tcp_stream.try_clone().unwrap()),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(ssl_stream) => Stream::Https(ssl_stream.clone()),
        }
    }
}

impl From<ConnectionStream> for Stream {
    fn from(tcp_stream: ConnectionStream) -> Self {
        Stream::Http(tcp_stream)
    }
}

impl Stream {
    fn peer_addr(&mut self) -> IoResult<Option<SocketAddr>> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.peer_addr(),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(ssl_stream) => ssl_stream.peer_addr(),
        }
    }

    fn read_timeout(&self) -> IoResult<Option<std::time::Duration>> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.read_timeout(),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(ssl_stream) => ssl_stream.read_timeout(),
        }
    }

    fn secure(&self) -> bool {
        match self {
            Stream::Http(_) => false,
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(_) => true,
        }
    }

    fn set_read_timeout(&mut self, dur: Option<std::time::Duration>) -> IoResult<()> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.set_read_timeout(dur),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(ssl_stream) => ssl_stream.set_read_timeout(dur),
        }
    }

    fn shutdown(&mut self, how: Shutdown) -> IoResult<()> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.shutdown(how),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(ssl_stream) => ssl_stream.shutdown(how),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.read(buf),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(ssl_stream) => ssl_stream.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.write(buf),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(ssl_stream) => ssl_stream.write(buf),
        }
    }

    fn flush(&mut self) -> IoResult<()> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.flush(),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            Stream::Https(ssl_stream) => ssl_stream.flush(),
        }
    }
}

pub(crate) struct RefinedTcpStream {
    stream: Stream,
    close_read: bool,
    close_write: bool,
}

impl RefinedTcpStream {
    /// Create `[RefinedTcpStream]`
    ///
    /// # Returns
    /// - tuple (closable Read, closable Write)
    ///
    pub(crate) fn new<S>(stream: S) -> (RefinedTcpStream, RefinedTcpStream)
    where
        S: Into<Stream>,
    {
        let stream: Stream = stream.into();

        let (read, mut write) = (stream.clone(), stream);

        // Although you can read at `TcpStream::try_clone()` comment:
        // The returned `TcpStream` is a reference to the same stream that this
        // object references. Both handles will read and write the same stream of
        // data, and options set on one stream will be propagated to the other
        // stream.
        //
        // The `read_timeout` seems not to be propagated. So we set no read_timeout
        // for writer or there is a problem in socket handling afterwards.
        let _ = write.set_read_timeout(None);

        let read = RefinedTcpStream {
            stream: read,
            close_read: true,
            close_write: false,
        };

        let write = RefinedTcpStream {
            stream: write,
            close_read: false,
            close_write: true,
        };

        (read, write)
    }

    /// Returns true if this struct wraps around a secure connection.
    #[inline]
    pub(crate) fn secure(&self) -> bool {
        self.stream.secure()
    }

    pub(crate) fn peer_addr(&mut self) -> IoResult<Option<SocketAddr>> {
        self.stream.peer_addr()
    }
}

impl Drop for RefinedTcpStream {
    fn drop(&mut self) {
        if self.close_read {
            let _ = self.stream.shutdown(Shutdown::Read);
        }

        if self.close_write {
            let _ = self.stream.shutdown(Shutdown::Write);
        }
    }
}

impl Read for RefinedTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.stream.read(buf)
    }
}

impl Write for RefinedTcpStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.stream.flush()
    }
}

impl ReadTimeout for RefinedTcpStream {
    fn read_timeout(&self) -> IoResult<Option<std::time::Duration>> {
        self.stream.read_timeout()
    }

    fn set_read_timeout(&mut self, dur: Option<std::time::Duration>) -> IoResult<()> {
        self.stream.set_read_timeout(dur)
    }
}
