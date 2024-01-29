use std::{
    net::{Shutdown, SocketAddr, TcpStream},
    os::unix::net as unix_net,
};

/// Unified stream. Either a [`TcpStream`] or [`std::os::unix::net::UnixStream`].
#[derive(Debug)]
pub(crate) enum ConnectionStream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(unix_net::UnixStream),
}

impl ConnectionStream {
    /// Gets the peer's address. Some for TCP, None for Unix sockets.
    pub(crate) fn peer_addr(&mut self) -> std::io::Result<Option<SocketAddr>> {
        match self {
            Self::Tcp(s) => s.peer_addr().map(Some),
            #[cfg(unix)]
            Self::Unix(_) => Ok(None),
        }
    }

    pub(crate) fn read_timeout(&self) -> std::io::Result<Option<std::time::Duration>> {
        match self {
            ConnectionStream::Tcp(s) => s.read_timeout(),
            ConnectionStream::Unix(s) => s.read_timeout(),
        }
    }

    pub(crate) fn set_read_timeout(
        &mut self,
        dur: Option<std::time::Duration>,
    ) -> std::io::Result<()> {
        match self {
            ConnectionStream::Tcp(s) => s.set_read_timeout(dur),
            ConnectionStream::Unix(s) => s.set_read_timeout(dur),
        }
    }

    pub(crate) fn shutdown(&self, how: Shutdown) -> std::io::Result<()> {
        match self {
            Self::Tcp(s) => s.shutdown(how),
            #[cfg(unix)]
            Self::Unix(s) => s.shutdown(how),
        }
    }

    pub(crate) fn try_clone(&self) -> std::io::Result<Self> {
        match self {
            Self::Tcp(s) => s.try_clone().map(Self::from),
            #[cfg(unix)]
            Self::Unix(s) => s.try_clone().map(Self::from),
        }
    }
}

impl std::io::Read for ConnectionStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            Self::Unix(s) => s.read(buf),
        }
    }
}

impl std::io::Write for ConnectionStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            Self::Unix(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(s) => s.flush(),
            #[cfg(unix)]
            Self::Unix(s) => s.flush(),
        }
    }
}

impl From<TcpStream> for ConnectionStream {
    fn from(s: TcpStream) -> Self {
        Self::Tcp(s)
    }
}

#[cfg(unix)]
impl From<unix_net::UnixStream> for ConnectionStream {
    fn from(s: unix_net::UnixStream) -> Self {
        Self::Unix(s)
    }
}
