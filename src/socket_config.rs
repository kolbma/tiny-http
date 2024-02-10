use std::net::TcpStream;

/// Config for TCP socket
///
/// With enabled _socket2_ feature exists the possibility to configure more settings.
///
/// # Defaults
///
/// `keep_alive`: true  
/// `linger`: None  
/// `no_delay`: true  
/// `read_timeout`: 10s  
/// `tcp_keepalive_interval`: None  
/// `tcp_keepalive_time`: 5s  
/// `write_timeout`: 10s
///
#[derive(Clone, Debug)]
#[allow(missing_docs)]
pub struct SocketConfig {
    /// `SO_LINGER` accuracy is in seconds (below 1s is 0)
    #[cfg(feature = "socket2")]
    pub linger: Option<std::time::Duration>,
    pub no_delay: bool,
    /// `read_timeout` accuracy is with possible __sub__-seconds
    pub read_timeout: std::time::Duration,
    /// `keep_alive` needs to be true for other `keep_alive` fields to have any effect
    #[cfg(feature = "socket2")]
    pub tcp_keep_alive: bool,
    /// `tcp_keepalive_interval` accuracy is in seconds
    #[cfg(feature = "socket2")]
    pub tcp_keepalive_interval: Option<std::time::Duration>,
    /// `tcp_keepalive_time` accuracy is in seconds
    #[cfg(feature = "socket2")]
    pub tcp_keepalive_time: std::time::Duration,
    /// `write_timeout` accuracy is with possible __sub__-seconds
    pub write_timeout: std::time::Duration,
}

impl SocketConfig {
    #[inline]
    pub(super) fn set_socket_cfg(
        socket: &mut TcpStream,
        config: &SocketConfig,
    ) -> Result<(), std::io::Error> {
        socket.set_nodelay(config.no_delay)?;
        if !config.read_timeout.is_zero() {
            socket.set_read_timeout(Some(config.read_timeout))?;
        }
        if !config.write_timeout.is_zero() {
            socket.set_write_timeout(Some(config.write_timeout))?;
        }
        Ok(())
    }
}

impl Default for SocketConfig {
    fn default() -> Self {
        Self {
            #[cfg(feature = "socket2")]
            linger: None,
            no_delay: true,
            read_timeout: std::time::Duration::from_secs(10),
            #[cfg(feature = "socket2")]
            tcp_keep_alive: true,
            #[cfg(feature = "socket2")]
            tcp_keepalive_interval: None,
            #[cfg(feature = "socket2")]
            tcp_keepalive_time: std::time::Duration::from_secs(5),
            write_timeout: std::time::Duration::from_secs(10),
        }
    }
}
