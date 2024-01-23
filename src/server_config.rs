use std::{net::SocketAddr, str::FromStr, time::Duration};

use crate::ConfigListenAddr;
use crate::SocketConfig;

/// Default connection limit for concurrent connections
pub(crate) const CONNECTION_LIMIT_DEFAULT: usize = 200;

/// Duration of sleep to check for concurrent connections
pub(crate) const CONNECTION_LIMIT_SLEEP_DURATION: Duration = Duration::from_millis(25);

/// Represents the config parameters required to create a server.
///
/// # Example
///
/// ```
/// # use tiny_http::ServerConfig;
/// let cfg = ServerConfig { connection_limit: 50, ..ServerConfig::default() };
/// ```
///
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The addresses to try to listen to.
    pub addr: ConfigListenAddr,

    /// Connections are limited to `connection_limit`
    pub connection_limit: usize,

    /// Socket configuration
    /// See [SocketConfig]
    pub socket_config: SocketConfig,

    /// If `Some`, then the server will use SSL to encode the communications.
    #[cfg(any(
        feature = "ssl-openssl",
        feature = "ssl-rustls",
        feature = "ssl-native-tls"
    ))]
    pub ssl: Option<crate::SslConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: ConfigListenAddr::IP(vec![SocketAddr::from_str("127.0.0.1:0").unwrap()]),
            connection_limit: CONNECTION_LIMIT_DEFAULT,
            socket_config: SocketConfig::default(),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            ssl: None,
        }
    }
}
