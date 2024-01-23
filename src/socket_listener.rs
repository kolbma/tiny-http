//! Abstractions of Tcp and Unix socket types

#[cfg(unix)]
use std::os::unix::net as unix_net;
use std::{
    net::{SocketAddr, TcpListener, ToSocketAddrs},
    path::PathBuf,
};

use crate::log;

use super::ConnectionStream;
use super::SocketConfig;

/// Unified listener. Either a [`TcpListener`] or [`std::os::unix::net::UnixListener`]
#[allow(missing_debug_implementations)]
pub enum Listener {
    /// [TcpListener] socket with [SocketConfig]
    Tcp(TcpListener, SocketConfig),
    /// [unix_net::UnixListener] socket
    #[cfg(unix)]
    Unix(unix_net::UnixListener),
}

impl Listener {
    pub(crate) fn local_addr(&self) -> std::io::Result<ListenAddr> {
        match self {
            Self::Tcp(l, _cfg) => l.local_addr().map(ListenAddr::from),
            #[cfg(unix)]
            Self::Unix(l) => l.local_addr().map(ListenAddr::from),
        }
    }

    pub(crate) fn accept(&self) -> std::io::Result<(ConnectionStream, Option<SocketAddr>)> {
        match self {
            Self::Tcp(l, cfg) => l.accept().map(|(mut stream, addr)| {
                if let Err(err) = SocketConfig::set_socket_cfg(&mut stream, cfg) {
                    log::error!("socket config fail: {err:?}");
                    let _ = err;
                }
                (ConnectionStream::from(stream), Some(addr))
            }),
            #[cfg(unix)]
            Self::Unix(l) => l
                .accept()
                .map(|(stream, _)| (ConnectionStream::from(stream), None)),
        }
    }
}

impl From<(TcpListener, SocketConfig)> for Listener {
    fn from((s, cfg): (TcpListener, SocketConfig)) -> Self {
        Self::Tcp(s, cfg)
    }
}

#[cfg(unix)]
impl From<unix_net::UnixListener> for Listener {
    fn from(s: unix_net::UnixListener) -> Self {
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
            Self::IP(addresses) => {
                log::debug!("addresses: {addresses:?}");

                let mut found_socket = Err(None);

                for address in addresses {
                    let socket = socket2::Socket::new(
                        socket2::Domain::for_address(*address),
                        socket2::Type::STREAM,
                        None,
                    )?;
                    socket.set_reuse_address(true)?;

                    if let Err(e) = socket.bind(&(*address).into()) {
                        found_socket = Err(Some(e));
                        continue;
                    }
                    if let Err(e) = socket.listen(1024) {
                        found_socket = Err(Some(e));
                        continue;
                    }

                    found_socket = Ok(socket);
                    break;
                }

                let socket = if let Ok(socket) = found_socket {
                    socket
                } else {
                    let err = found_socket.unwrap_err().unwrap();
                    log::error!("socket bind fail: {err:?}");
                    return Err(err);
                };

                socket.set_linger(config.linger)?;
                if config.tcp_keep_alive {
                    if config.tcp_keepalive_time.is_zero() {
                        socket.set_keepalive(config.tcp_keep_alive)?;
                    } else {
                        socket.set_tcp_keepalive(&if let Some(tcp_keepalive_interval) =
                            config.tcp_keepalive_interval
                        {
                            socket2::TcpKeepalive::new()
                                .with_interval(tcp_keepalive_interval)
                                .with_time(config.tcp_keepalive_time)
                        } else {
                            socket2::TcpKeepalive::new().with_time(config.tcp_keepalive_time)
                        })?;
                    }
                }
                socket.set_nodelay(config.no_delay)?;

                Ok(Listener::Tcp(socket.into(), config.clone()))
            }
            #[cfg(unix)]
            Self::Unix(path) => unix_net::UnixListener::bind(path).map(Listener::from),
        }
    }

    #[cfg(not(feature = "socket2"))]
    pub(crate) fn bind(&self, config: &SocketConfig) -> std::io::Result<Listener> {
        match self {
            Self::IP(addresses) => {
                log::debug!("addresses: {addresses:?}");
                TcpListener::bind(addresses.as_slice()).map(|l| Listener::Tcp(l, config.clone()))
            }
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
