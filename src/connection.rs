//! Abstractions of Tcp and Unix socket types

#[cfg(unix)]
use std::os::unix::net as unix_net;
use std::{
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    path::PathBuf,
};

/// Unified listener. Either a [`TcpListener`] or [`std::os::unix::net::UnixListener`]
#[allow(missing_debug_implementations)]
pub enum Listener {
    /// [TcpListener] socket with [SocketConfig]
    #[cfg(feature = "socket2")]
    Tcp(TcpListener, SocketConfig),
    /// [TcpListener] socket
    #[cfg(not(feature = "socket2"))]
    Tcp(TcpListener),
    /// [unix_net::UnixListener] socket
    #[cfg(unix)]
    Unix(unix_net::UnixListener),
}
impl Listener {
    pub(crate) fn local_addr(&self) -> std::io::Result<ListenAddr> {
        match self {
            #[cfg(feature = "socket2")]
            Self::Tcp(l, _cfg) => l.local_addr().map(ListenAddr::from),
            #[cfg(not(feature = "socket2"))]
            Self::Tcp(l) => l.local_addr().map(ListenAddr::from),
            #[cfg(unix)]
            Self::Unix(l) => l.local_addr().map(ListenAddr::from),
        }
    }

    #[cfg(feature = "socket2")]
    pub(crate) fn accept(&self) -> std::io::Result<(Connection, Option<SocketAddr>)> {
        use crate::log;

        match self {
            Self::Tcp(l, cfg) => l.accept().map(|(mut conn, addr)| {
                if let Err(err) = set_socket_cfg(&mut conn, cfg) {
                    log::error!("socket config fail: {err:?}");
                    let _ = err;
                }
                (Connection::from(conn), Some(addr))
            }),
            #[cfg(unix)]
            Self::Unix(l) => l.accept().map(|(conn, _)| (Connection::from(conn), None)),
        }
    }

    #[cfg(not(feature = "socket2"))]
    pub(crate) fn accept(&self) -> std::io::Result<(Connection, Option<SocketAddr>)> {
        match self {
            Self::Tcp(l) => l
                .accept()
                .map(|(conn, addr)| (Connection::from(conn), Some(addr))),
            #[cfg(unix)]
            Self::Unix(l) => l.accept().map(|(conn, _)| (Connection::from(conn), None)),
        }
    }
}
#[cfg(feature = "socket2")]
impl From<(TcpListener, SocketConfig)> for Listener {
    fn from((s, cfg): (TcpListener, SocketConfig)) -> Self {
        Self::Tcp(s, cfg)
    }
}
#[cfg(not(feature = "socket2"))]
impl From<TcpListener> for Listener {
    fn from(s: TcpListener) -> Self {
        Self::Tcp(s)
    }
}
#[cfg(unix)]
impl From<unix_net::UnixListener> for Listener {
    fn from(s: unix_net::UnixListener) -> Self {
        Self::Unix(s)
    }
}

#[cfg(feature = "socket2")]
#[inline]
fn set_socket_cfg(socket: &mut TcpStream, config: &SocketConfig) -> Result<(), std::io::Error> {
    socket.set_nodelay(config.no_delay)?;
    if !config.read_timeout.is_zero() {
        socket.set_read_timeout(Some(config.read_timeout))?;
    }
    if !config.write_timeout.is_zero() {
        socket.set_write_timeout(Some(config.write_timeout))?;
    }
    Ok(())
}

/// Unified connection. Either a [`TcpStream`] or [`std::os::unix::net::UnixStream`].
#[derive(Debug)]
pub(crate) enum Connection {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(unix_net::UnixStream),
}
impl std::io::Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            Self::Unix(s) => s.read(buf),
        }
    }
}
impl std::io::Write for Connection {
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
impl Connection {
    /// Gets the peer's address. Some for TCP, None for Unix sockets.
    pub(crate) fn peer_addr(&mut self) -> std::io::Result<Option<SocketAddr>> {
        match self {
            Self::Tcp(s) => s.peer_addr().map(Some),
            #[cfg(unix)]
            Self::Unix(_) => Ok(None),
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
impl From<TcpStream> for Connection {
    fn from(s: TcpStream) -> Self {
        Self::Tcp(s)
    }
}
#[cfg(unix)]
impl From<unix_net::UnixStream> for Connection {
    fn from(s: unix_net::UnixStream) -> Self {
        Self::Unix(s)
    }
}

/// Address of configuration  
/// Unified listen socket address. Either a `Vec` of [`SocketAddr`] or [`std::os::unix::net::SocketAddr`].
#[derive(Debug, Clone)]
pub enum ConfigListenAddr {
    /// [SocketAddr] for IP net
    IP(Vec<SocketAddr>),
    /// [PathBuf] for `Unix`socket
    #[cfg(unix)]
    // TODO: use SocketAddr when bind_addr is stabilized
    Unix(PathBuf),
}
impl ConfigListenAddr {
    /// Create `[ConfigListenAddr]` from `IP` addresses
    ///
    /// # Errors
    ///
    /// - `std::io::Error` when `addrs` are no socket addresses
    ///
    pub fn from_socket_addrs<A: ToSocketAddrs>(addrs: A) -> std::io::Result<Self> {
        addrs.to_socket_addrs().map(|it| Self::IP(it.collect()))
    }

    /// Create `[ConfigListenAddr]` from `path`
    #[cfg(unix)]
    pub fn unix_from_path<P: Into<PathBuf>>(path: P) -> Self {
        Self::Unix(path.into())
    }

    #[cfg(feature = "socket2")]
    pub(crate) fn bind(&self, config: &SocketConfig) -> std::io::Result<Listener> {
        match self {
            Self::IP(ip) => {
                let addresses = ip;
                let mut err = None;
                let mut socket =
                    socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None)?;

                for address in addresses {
                    socket = socket2::Socket::new(
                        socket2::Domain::for_address(*address),
                        socket2::Type::STREAM,
                        None,
                    )?;

                    if let Err(e) = socket.bind(&(*address).into()) {
                        err = Some(e);
                        continue;
                    }
                    if let Err(e) = socket.listen(128) {
                        err = Some(e);
                        continue;
                    }
                    err = None;
                    break;
                }

                if let Some(err) = err {
                    return Err(err);
                }

                socket.set_keepalive(config.keep_alive)?;
                socket.set_tcp_keepalive(&if let Some(tcp_keepalive_interval) =
                    config.tcp_keepalive_interval
                {
                    socket2::TcpKeepalive::new()
                        .with_interval(tcp_keepalive_interval)
                        .with_time(config.tcp_keepalive_time)
                } else {
                    socket2::TcpKeepalive::new().with_time(config.tcp_keepalive_time)
                })?;

                Ok(Listener::Tcp(socket.into(), config.clone()))
            }
            #[cfg(unix)]
            Self::Unix(path) => unix_net::UnixListener::bind(path).map(Listener::from),
        }
    }

    #[cfg(not(feature = "socket2"))]
    pub(crate) fn bind(&self) -> std::io::Result<Listener> {
        match self {
            Self::IP(addresses) => TcpListener::bind(addresses.as_slice()).map(Listener::from),
            #[cfg(unix)]
            Self::Unix(path) => unix_net::UnixListener::bind(path).map(Listener::from),
        }
    }
}

/// Unified listen socket address. Either a [`SocketAddr`] or [`std::os::unix::net::SocketAddr`].
#[derive(Debug, Clone)]
pub enum ListenAddr {
    /// [SocketAddr] for IP net
    IP(SocketAddr),
    /// Unix [unix_net::SocketAddr]
    #[cfg(unix)]
    Unix(unix_net::SocketAddr),
}
impl ListenAddr {
    /// Get `[SocketAddr]` if it is an `IP` else `None`
    #[must_use]
    pub fn to_ip(self) -> Option<SocketAddr> {
        match self {
            Self::IP(s) => Some(s),
            #[cfg(unix)]
            Self::Unix(_) => None,
        }
    }

    /// Gets the Unix socket address.
    ///
    /// This is also available on non-Unix platforms, for ease of use, but always returns `None`.
    #[must_use]
    #[cfg(unix)]
    pub fn to_unix(self) -> Option<unix_net::SocketAddr> {
        match self {
            Self::IP(_) => None,
            Self::Unix(s) => Some(s),
        }
    }
    #[cfg(not(unix))]
    pub fn to_unix(self) -> Option<SocketAddr> {
        None
    }
}
impl From<SocketAddr> for ListenAddr {
    fn from(s: SocketAddr) -> Self {
        Self::IP(s)
    }
}
#[cfg(unix)]
impl From<unix_net::SocketAddr> for ListenAddr {
    fn from(s: unix_net::SocketAddr) -> Self {
        Self::Unix(s)
    }
}
impl std::fmt::Display for ListenAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IP(s) => s.fmt(f),
            #[cfg(unix)]
            Self::Unix(s) => std::fmt::Debug::fmt(s, f),
        }
    }
}

/// Config for TCP socket with enabled _socket2_ feature
///
/// # Defaults
///
/// `keep_alive`: true  
/// `no_delay`: true  
/// `read_timeout`: 10s  
/// `tcp_keepalive_interval`: None  
/// `tcp_keepalive_time`: 5s  
/// `write_timeout`: 10s
///
#[cfg(feature = "socket2")]
#[derive(Clone, Debug)]
#[allow(missing_docs)]
pub struct SocketConfig {
    pub keep_alive: bool,
    pub no_delay: bool,
    pub read_timeout: std::time::Duration,
    pub tcp_keepalive_interval: Option<std::time::Duration>,
    pub tcp_keepalive_time: std::time::Duration,
    pub write_timeout: std::time::Duration,
}

#[cfg(feature = "socket2")]
impl Default for SocketConfig {
    fn default() -> Self {
        Self {
            keep_alive: true,
            no_delay: true,
            read_timeout: std::time::Duration::from_secs(10),
            tcp_keepalive_interval: None,
            tcp_keepalive_time: std::time::Duration::from_secs(5),
            write_timeout: std::time::Duration::from_secs(10),
        }
    }
}
