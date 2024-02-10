//! Abstractions of Tcp and Unix socket types

use std::net::IpAddr;
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
#[cfg(unix)]
use std::os::unix::net as unix_net;
use std::path::PathBuf;
use std::sync::Arc;

use crate::log;

use super::ConnectionStream;
use super::SocketConfig;

/// Unified listener. Either a [`TcpListener`] or [`std::os::unix::net::UnixListener`]
#[allow(missing_debug_implementations)]
pub enum Listener {
    /// [TcpListener] socket with [SocketConfig]
    Tcp(TcpListener, Arc<SocketConfig>),
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

impl From<(TcpListener, Arc<SocketConfig>)> for Listener {
    fn from((s, cfg): (TcpListener, Arc<SocketConfig>)) -> Self {
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
    // TODO: use SocketAddr when bind_addr is stabilized (since 1.70)
    Unix(PathBuf),
}

impl ConfigListenAddr {
    /// Create [`ConfigListenAddr`] from `IP` addresses
    ///
    /// # Errors
    ///
    /// - `std::io::Error` when `addrs` are no socket addresses
    ///
    pub fn from_socket_addrs<A: ToSocketAddrs>(addrs: A) -> std::io::Result<Self> {
        addrs.to_socket_addrs().map(|it| Self::IP(it.collect()))
    }

    /// Create [`ConfigListenAddr`] from `path`
    #[cfg(unix)]
    pub fn unix_from_path<P: Into<PathBuf>>(path: P) -> Self {
        Self::Unix(path.into())
    }

    #[cfg(feature = "socket2")]
    pub(crate) fn bind(&self, config: &Arc<SocketConfig>) -> std::io::Result<Listener> {
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

                Ok(Listener::Tcp(socket.into(), Arc::clone(config)))
            }
            #[cfg(unix)]
            Self::Unix(path) => unix_net::UnixListener::bind(path).map(Listener::from),
        }
    }

    #[cfg(not(feature = "socket2"))]
    pub(crate) fn bind(&self, config: &Arc<SocketConfig>) -> std::io::Result<Listener> {
        match self {
            Self::IP(addresses) => {
                log::debug!("addresses: {addresses:?}");
                TcpListener::bind(&addresses[..]).map(|l| Listener::Tcp(l, Arc::clone(config)))
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
    /// Returns the IP address associated with this socket address.
    #[must_use]
    pub fn ip(&self) -> Option<IpAddr> {
        match self {
            ListenAddr::IP(s) => Some(s.ip()),
            #[cfg(unix)]
            ListenAddr::Unix(_) => None,
        }
    }

    /// Returns the port number associated with this socket address.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        match self {
            ListenAddr::IP(s) => Some(s.port()),
            #[cfg(unix)]
            ListenAddr::Unix(_) => None,
        }
    }

    /// Convert to [`SocketAddr`] if it is an `IP` else `None`
    #[must_use]
    pub fn to_ip(self) -> Option<SocketAddr> {
        match self {
            Self::IP(s) => Some(s),
            #[cfg(unix)]
            Self::Unix(_) => None,
        }
    }

    /// Get [`SocketAddr`] if it is an `IP` else `None`
    #[must_use]
    pub fn socket_addrs(&self) -> Option<&SocketAddr> {
        match self {
            ListenAddr::IP(s) => Some(s),
            #[cfg(unix)]
            ListenAddr::Unix(_) => None,
        }
    }

    /// Get the Unix socket address.
    ///
    /// Or `None` for `IP`
    #[cfg(unix)]
    #[must_use]
    pub fn to_unix(self) -> Option<unix_net::SocketAddr> {
        match self {
            Self::IP(_) => None,
            Self::Unix(s) => Some(s),
        }
    }
    /// Returns `None`, for ease of use available on non-Unix platforms
    #[cfg(not(unix))]
    #[must_use]
    pub fn to_unix(self) -> Option<SocketAddr> {
        None
    }

    /// Get the Unix socket address.
    ///
    /// Or `None` for `IP`
    #[cfg(unix)]
    #[must_use]
    pub fn unix_socket_addrs(&self) -> Option<&unix_net::SocketAddr> {
        match self {
            ListenAddr::IP(_) => None,
            ListenAddr::Unix(s) => Some(s),
        }
    }
    /// Returns `None`, for ease of use available on non-Unix platforms
    #[cfg(not(unix))]
    #[must_use]
    pub fn unix_socket_addrs(&self) -> Option<&unix_net::SocketAddr> {
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
