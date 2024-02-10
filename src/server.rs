//! [`Server<state::MultiThreaded>`] and [`Server<state::SingleThreaded>`] implementations
//!
//! [`Server<state::SingleThreaded>`]: ./struct.Server.html#impl-Server%3CSingleThreaded%3E
//! [`Server<state::MultiThreaded>`]: ./struct.Server.html#impl-Server%3CMultiThreaded%3E

use std::error::Error;
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};
use std::marker::PhantomData;
use std::net::ToSocketAddrs;
use std::sync::atomic::AtomicBool;
use std::sync::{
    atomic::{AtomicU16, Ordering},
    Arc,
};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::listener_thread::{self, ListenerThread};
use crate::log;
use crate::request::Request;
use crate::request_handler::RequestHandler;
use crate::server_config::ServerConfig;
use crate::socket_listener::{ConfigListenAddr, ListenAddr};
#[cfg(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
))]
use crate::ssl::SslConfig;

/// Implementations of `tiny_http` `Server`.
///
#[allow(missing_debug_implementations)]
pub struct Server<S: State> {
    /// wait for exit
    exit: Option<Arc<(Mutex<u8>, Condvar)>>,

    /// timeout to wait for graceful server exit
    exit_graceful_timeout: Duration,

    /// timeout to wait on exit condition in `wait_for_exit()`
    exit_wait_timeout: Duration,

    /// the [`ListenAddr`]
    listen_addr: Option<ListenAddr>,

    /// started listener threads, this becomes none in `Running` state,
    /// because the `RequestHandler` takes responsibility/ownerships
    listener_thread: Option<ListenerThread>,
    /// `listener_thread` is bound as ref for shutdown possibility
    listener_thread_ref: Option<Arc<ListenerThread>>,

    /// `JoinHandle` for listener thread
    listener_thread_jh: Option<thread::JoinHandle<()>>,

    /// number of currently open connections
    num_connections: Arc<AtomicU16>,

    _state: PhantomData<S>,
}

impl<S: State> Server<S> {
    /// Returns the number of clients currently connected to the server.
    #[must_use]
    pub fn num_connections(&self) -> u16 {
        self.num_connections.load(Ordering::Acquire)
    }
}

/// Implementation of `Server` creating multiple worker threads.
///
/// This server starts up the configured number of worker threads.
///
/// In the default configuration used by the shortcut constructors
/// `http()`, `https()`, `http_unix()` there are 2 worker threads.
///
/// ```
/// let _ = tiny_http::ServerConfig {
///     worker_thread_nr: 2,
///     ..tiny_http::ServerConfig::default()
/// };
/// ```
///
/// You could use here the `num_cpus`-Crate:
///
/// ```
/// # // use std::convert::TryFrom;
/// # // extern crate num_cpus;
/// let _ = tiny_http::ServerConfig {
///     // worker_thread_nr: u8::try_from(num_cpus::get()).unwrap_or(u8::MAX),
///     ..tiny_http::ServerConfig::default()
/// };
/// ```
///
/// Next to the worker threads exists an internal `TaskPool`.  
/// For connections and request construction the pool provides a dynamically
/// growing, reused and shrinking thread count based on the server load.  
/// After finishing a task the created request instance is queued to be handled
/// by a worker thread with [`RequestHandler`].
///
/// Depending on your server load you can experiment to provide a good number
/// of worker threads responding the queued requests from `TaskPool` fast.
///
/// # Usage
///
/// For multi-thread-worker there is the need to provide a [`RequestHandler`]
/// implementation.
///
/// A straight function or closure can be used with the provided implementation
/// [`FnRequestHandler`].
///
/// # Example
///
/// ```no_run
/// use tiny_http::{FnRequestHandler, ListenerThread, MTServer as Server};
///
/// # fn main() -> std::io::Result<()> {
/// let mut server = Server::http("0.0.0.0:0")?
///     .add_request_handler(FnRequestHandler(|listener: &ListenerThread| {
///         while let Some(request) = listener.incoming_requests().next() {
///             let _ = request;
///             todo!()
///         }
///     }));
/// server.wait_for_exit(None);
/// # Ok(())
/// # }
/// ```
///
/// [`FnRequestHandler`]: crate::request_handler::FnRequestHandler
///
impl Server<state::MultiThreaded> {
    /// Builds a new server that listens on the specified address.
    ///
    /// The [`state::MultiThreaded`] server uses at least 2 worker threads for
    /// handling incoming requests.  
    /// It can be configured in the provided [`ServerConfig`].
    ///
    /// # Errors
    ///
    /// `std::io::Error` when socket binding failed
    ///
    pub fn new(server_config: &ServerConfig) -> IoResult<Server<state::NeedRequestHandler>> {
        let server_config = if server_config.worker_thread_nr <= 1 {
            let mut cfg = server_config.clone();
            cfg.worker_thread_nr = 2;
            cfg
        } else {
            server_config.clone()
        };

        let listener = server_config.addr.bind(&server_config.socket_config)?;

        let listen_addr = listener.local_addr().ok();

        let num_connections = Arc::new(AtomicU16::default());

        let mut listener_thread = ListenerThread::new(listener, &server_config, &num_connections)
            .map_err(|err| IoError::new(IoErrorKind::Other, err))?;

        Ok(Server::<state::NeedRequestHandler> {
            exit: None,
            exit_graceful_timeout: server_config.exit_graceful_timeout,
            exit_wait_timeout: server_config.exit_wait_timeout,
            listen_addr,
            listener_thread_jh: listener_thread.join_handle(),
            listener_thread: Some(listener_thread),
            listener_thread_ref: None,
            num_connections,
            _state: PhantomData,
        })
    }

    /// Shortcut for a simple server on a specific address.
    ///
    /// # Errors
    ///
    /// `std::io::Error` when `addr` is no socket address
    #[inline]
    pub fn http<A>(addr: A) -> IoResult<Server<state::NeedRequestHandler>>
    where
        A: ToSocketAddrs,
    {
        Self::new(&ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs(addr)?,
            ..ServerConfig::default()
        })
    }

    /// Shortcut for a UNIX socket server at a specific path.
    ///
    /// # Errors
    ///
    /// - `std::io::Error` when `addr` is no socket address
    /// - `std::io::Error` when socket binding failed
    ///
    #[cfg(unix)]
    #[inline]
    pub fn http_unix(path: &std::path::Path) -> IoResult<Server<state::NeedRequestHandler>> {
        Self::new(&ServerConfig {
            addr: ConfigListenAddr::unix_from_path(path),
            ..ServerConfig::default()
        })
    }

    /// Shortcut for an HTTPS server on a specific address.
    ///
    /// # Errors
    ///
    /// - `std::io::Error` when `addr` is no socket address
    /// - `std::io::Error` when socket binding failed
    ///
    #[cfg(any(
        feature = "ssl-openssl",
        feature = "ssl-rustls",
        feature = "ssl-native-tls"
    ))]
    #[inline]
    pub fn https<A>(addr: A, ssl_config: SslConfig) -> IoResult<Server<state::NeedRequestHandler>>
    where
        A: ToSocketAddrs,
    {
        Self::new(&ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs(addr)?,
            ssl: Some(ssl_config),
            ..ServerConfig::default()
        })
    }
}

/// Classical server for handling requests completely in the application code.
impl Server<state::SingleThreaded> {
    /// Builds a new server that listens on the specified address.
    ///
    /// # Errors
    ///
    /// `std::io::Error` when socket binding failed  
    /// `SingleThreaded` when `ServerConfig` requests more than 1 thread
    ///
    pub fn new(
        server_config: &ServerConfig,
    ) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
        if server_config.worker_thread_nr > 1 {
            return Err(Box::new(state::SingleThreaded));
        }

        let num_connections = Arc::new(AtomicU16::default());

        let listener = server_config.addr.bind(&server_config.socket_config)?;

        let listen_addr = listener.local_addr().ok();

        let mut listener_thread = ListenerThread::new(listener, server_config, &num_connections)?;

        Ok(Self {
            exit: None,
            exit_graceful_timeout: server_config.exit_graceful_timeout,
            exit_wait_timeout: server_config.exit_wait_timeout,
            listen_addr,
            listener_thread_jh: listener_thread.join_handle(),
            listener_thread: Some(listener_thread),
            listener_thread_ref: None,
            num_connections,
            _state: PhantomData,
        })
    }

    /// Shortcut for a simple server on a specific address.
    ///
    /// # Errors
    ///
    /// `std::io::Error` when `addr` is no socket address
    #[inline]
    pub fn http<A>(addr: A) -> Result<Self, Box<dyn Error + Send + Sync + 'static>>
    where
        A: ToSocketAddrs,
    {
        Self::new(&ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs(addr)?,
            ..ServerConfig::default()
        })
    }

    /// Shortcut for a UNIX socket server at a specific path.
    ///
    /// # Errors
    ///
    /// - `std::io::Error` when `addr` is no socket address
    /// - `std::io::Error` when socket binding failed
    ///
    #[cfg(unix)]
    #[inline]
    pub fn http_unix(
        path: &std::path::Path,
    ) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
        Self::new(&ServerConfig {
            addr: ConfigListenAddr::unix_from_path(path),
            ..ServerConfig::default()
        })
    }

    /// Shortcut for an HTTPS server on a specific address.
    ///
    /// # Errors
    ///
    /// - `std::io::Error` when `addr` is no socket address
    /// - `std::io::Error` when socket binding failed
    ///
    #[cfg(any(
        feature = "ssl-openssl",
        feature = "ssl-rustls",
        feature = "ssl-native-tls"
    ))]
    #[inline]
    pub fn https<A>(
        addr: A,
        ssl_config: SslConfig,
    ) -> Result<Self, Box<dyn Error + Send + Sync + 'static>>
    where
        A: ToSocketAddrs,
    {
        Self::new(&ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs(addr)?,
            ssl: Some(ssl_config),
            ..ServerConfig::default()
        })
    }

    /// Returns an iterator for all the incoming requests.
    ///
    /// The iterator will return `None` if the server socket is shutdown
    /// or `OsError` occurred.
    #[must_use]
    #[inline]
    #[allow(clippy::missing_panics_doc)]
    pub fn incoming_requests(&self) -> listener_thread::IncomingRequests<'_> {
        self.listener_thread.as_ref().unwrap().incoming_requests()
    }

    /// Blocks until an HTTP request has been submitted and returns it.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`]
    ///
    #[inline]
    #[allow(clippy::missing_panics_doc)]
    pub fn recv(&self) -> IoResult<Request> {
        self.listener_thread.as_ref().unwrap().recv()
    }

    /// Same as `recv()` but doesn't block longer than timeout.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`]
    ///
    #[inline]
    #[allow(clippy::missing_panics_doc)]
    pub fn recv_timeout(&self, timeout: Duration) -> IoResult<Option<Request>> {
        self.listener_thread.as_ref().unwrap().recv_timeout(timeout)
    }

    /// Returns the address the server is listening to.
    #[must_use]
    #[inline]
    #[allow(clippy::missing_panics_doc)]
    pub fn server_addr(&self) -> &ListenAddr {
        self.listen_addr.as_ref().unwrap()
    }

    /// Same as `recv()` but doesn't block.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`]
    ///
    #[inline]
    #[allow(clippy::missing_panics_doc)]
    pub fn try_recv(&self) -> IoResult<Option<Request>> {
        self.listener_thread.as_ref().unwrap().try_recv()
    }

    /// Unblock thread stuck in `recv()` or `incoming_requests()`.
    ///
    /// If there are several such threads, only one is unblocked.
    /// This method allows graceful shutdown of server.
    #[allow(clippy::missing_panics_doc)]
    pub fn unblock(&self) {
        self.listener_thread.as_ref().unwrap().unblock();
    }
}

impl Server<state::NeedRequestHandler> {
    /// Add required [`RequestHandler`] to handle incoming requests.
    #[inline]
    #[allow(clippy::missing_panics_doc)]
    pub fn add_request_handler(
        mut self,
        request_handler: impl RequestHandler + 'static,
    ) -> Server<state::Running> {
        let listener_thread = self.listener_thread.take().unwrap();
        let (listener_thread, exit_cond_var) =
            listener_thread.start_request_handling(request_handler);

        Server::<state::Running> {
            exit: Some(exit_cond_var),
            exit_graceful_timeout: self.exit_graceful_timeout,
            exit_wait_timeout: self.exit_wait_timeout,
            listen_addr: self.listen_addr.take(),
            listener_thread: None,
            listener_thread_ref: Some(listener_thread),
            listener_thread_jh: self.listener_thread_jh.take(),
            num_connections: Arc::clone(&self.num_connections),
            _state: PhantomData,
        }
    }
}

impl Server<state::Running> {
    /// Returns the address the server is listening to.
    #[must_use]
    #[inline]
    #[allow(clippy::missing_panics_doc)]
    pub fn server_addr(&self) -> &ListenAddr {
        self.listen_addr.as_ref().unwrap()
    }

    /// Call `wait_for_exit()` to block your application until all threads are dropped.
    ///
    /// The optional `force_exit` [`AtomicBool`], stops the server on `true` value.  
    /// The method returns after graceful server stop.
    ///
    /// `force_exit` is set back to false.  
    /// If `force_exit` is set a 2nd time to `true` the server exits ungracefully.
    ///
    /// Most likely you will provide a reference to an `Arc<AtomicBool>` to have the
    /// possibility to set `force_exit` while `wait_for_exit`.
    pub fn wait_for_exit(&mut self, force_exit: Option<&AtomicBool>) {
        let mut is_graceful = false;

        if let Some(exit) = &self.exit {
            let (mtx, exit_cond) = exit.as_ref();
            if let Ok(mut guard) = mtx.lock() {
                if let Some(force_exit) = force_exit {
                    while *guard > 0 {
                        if force_exit.load(Ordering::Relaxed) {
                            self.stop_server();
                            if is_graceful {
                                is_graceful = false;
                                break;
                            }
                            is_graceful = true;
                            force_exit.store(false, Ordering::Release);
                        }
                        guard = match exit_cond.wait_timeout(guard, self.exit_wait_timeout) {
                            Ok((guard, _)) => guard,
                            Err(err) => {
                                log::error!("{err:?}");
                                drop(err);
                                break;
                            }
                        }
                    }
                } else {
                    while *guard > 0 {
                        guard = match exit_cond.wait(guard) {
                            Ok(guard) => guard,
                            Err(err) => {
                                log::error!("{err:?}");
                                drop(err);
                                break;
                            }
                        }
                    }
                }
            } else {
                log::error!("lock fail");
            }
        }

        if let Some(jh) = self.listener_thread_jh.take() {
            if !jh.is_finished() {
                let now = Instant::now();
                let force_exit_at = if is_graceful {
                    now + Duration::from_secs(5)
                } else {
                    now + self.exit_graceful_timeout
                };

                while !jh.is_finished() {
                    thread::sleep(Duration::from_millis(100));
                    if Instant::now() >= force_exit_at {
                        break;
                    }
                }
            }
        }

        #[allow(clippy::if_same_then_else)]
        if log::log_enabled!(log::Level::Info) {
            if is_graceful {
                log::info!("server exited gracefully");
            } else {
                log::info!("server exits ungracefully");
            }
        }
    }

    /// Stops the server gracefully.
    pub fn stop_server(&self) {
        if let Some(listener_thread) = self.listener_thread_ref.as_ref() {
            listener_thread.shutdown();
        }
    }
}

impl<S: State> Drop for Server<S> {
    fn drop(&mut self) {
        if let Some(listener_thread) = self.listener_thread_ref.as_ref() {
            listener_thread.shutdown();
        }
    }
}

/// States of [`Server`] implement this trait
pub trait State {}

/// Available [`Server`](super::Server) [`States`](super::State)
pub mod state {
    /// For convenience exists the type [`MTServer`](crate::MTServer)
    #[derive(Debug)]
    pub struct MultiThreaded;
    impl super::State for MultiThreaded {}

    /// State requesting a [`RequestHandler`](crate::RequestHandler)
    #[derive(Debug)]
    pub struct NeedRequestHandler;
    impl super::State for NeedRequestHandler {}

    /// [`ListenerThread`](crate::ListenerThread)(s) are handling [`Request`](crate::Request)
    #[derive(Debug)]
    pub struct Running;
    impl super::State for Running {}

    /// For convenience exists the type [`STServer`](crate::STServer)
    #[derive(Debug)]
    pub struct SingleThreaded;
    impl super::State for SingleThreaded {}

    impl std::error::Error for SingleThreaded {}

    impl std::fmt::Display for SingleThreaded {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("configuration not single threaded")
        }
    }
}
