use std::sync::Arc;
use std::{net::Ipv4Addr, net::Ipv6Addr, net::SocketAddr, time::Duration};

use crate::ConfigListenAddr;
use crate::LimitsConfig;
use crate::SocketConfig;

/// Duration of sleep to check for concurrent connections
pub(crate) const CONNECTION_LIMIT_SLEEP_DURATION: Duration = Duration::from_millis(25);

/// Represents the config parameters required to create a server.
///
/// # Example
///
/// ```
/// # use tiny_http::{LimitsConfig, ServerConfig};
/// let cfg = ServerConfig {
///     limits: LimitsConfig {
///         connection_limit: 50,
///         ..LimitsConfig::default()
///     },
///     ..ServerConfig::default()
/// };
/// ```
///
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The addresses to try to listen to.
    pub addr: ConfigListenAddr,

    /// Timeout to wait for graceful server exit (default 30s)
    pub exit_graceful_timeout: Duration,

    /// Timeout to wait on exit condition in `wait_for_exit()` (default 1000ms)
    pub exit_wait_timeout: Duration,

    /// Configuration of limits [`LimitsConfig`]
    pub limits: LimitsConfig,

    /// Socket configuration
    /// See [SocketConfig]
    pub socket_config: Arc<SocketConfig>,

    /// If `Some`, then the server will use SSL to encode the communications.
    #[cfg(any(
        feature = "ssl-openssl",
        feature = "ssl-rustls",
        feature = "ssl-native-tls"
    ))]
    pub ssl: Option<crate::SslConfig>,

    /// Number of worker threads the server will start (default 1).
    ///
    /// For productive servers the number should be higher.  
    /// A responsive server might have as much worker threads as CPU cores.  
    /// But here you can experiment a little bit with the requirements of your
    /// application.
    ///
    /// A worker thread runs your custom [`RequestHandler`] code.
    ///
    /// [`RequestHandler`]: crate::RequestHandler
    ///
    pub worker_thread_nr: u8,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: ConfigListenAddr::IP(vec![
                SocketAddr::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1).into(), 0),
                SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            ]),
            exit_graceful_timeout: Duration::from_secs(30),
            exit_wait_timeout: Duration::from_millis(1000),
            limits: LimitsConfig::default(),
            socket_config: Arc::new(SocketConfig::default()),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            ssl: None,
            worker_thread_nr: 1,
        }
    }
}
