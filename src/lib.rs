//! # Simple usage
//!
//! ## Creating the server
//!
//! The easiest way to create a server is to call `Server::http()`.
//!
//! The `http()` function returns an `IoResult<Server>` which will return an error
//! in the case where the server creation fails (for example if the listening port is already
//! occupied).
//!
//! ```no_run
//! let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//! ```
//!
//! A newly-created `Server` will immediately start listening for incoming connections and HTTP
//! requests.
//!
//! ## Receiving requests
//!
//! Calling `server.recv()` will block until the next request is available.
//! This function returns an `IoResult<Request>`, so you need to handle the possible errors.
//!
//! ```no_run
//! # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//!
//! loop {
//!     // blocks until the next request is received
//!     let request = match server.recv() {
//!         Ok(rq) => rq,
//!         Err(err) => { eprintln!("error: {err}"); break }
//!     };
//!
//!     // do something with the request
//!     // ...
//! }
//! ```
//!
//! In a real-case scenario, you will probably want to spawn multiple worker tasks and call
//! `server.recv()` on all of them. Like this:
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use std::thread;
//! # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//! let server = Arc::new(server);
//! let mut guards = Vec::with_capacity(4);
//!
//! for _ in (0 .. 4) {
//!     let server = server.clone();
//!
//!     let guard = thread::spawn(move || {
//!         loop {
//!             let rq = server.recv().unwrap();
//!
//!             // ...
//!         }
//!     });
//!
//!     guards.push(guard);
//! }
//! ```
//!
//! If you don't want to block, you can call `server.try_recv()` instead.
//!
//! ## Handling requests
//!
//! The `Request` object returned by `server.recv()` contains informations about the client's request.
//! The most useful methods are probably `request.method()` and `request.url()` which return
//! the requested method (`GET`, `POST`, etc.) and url.
//!
//! To handle a request, you need to create a `Response` object. See the docs of this object for
//! more infos. Here is an example of creating a `Response` from a file:
//!
//! ```no_run
//! # use std::fs::File;
//! # use std::path::Path;
//! let response = tiny_http::Response::from_file(File::open(&Path::new("image.png")).unwrap());
//! ```
//!
//! All that remains to do is call `request.respond()`:
//!
//! ```no_run
//! # use std::fs::File;
//! # use std::path::Path;
//! # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//! # let request = server.recv().unwrap();
//! # let response = tiny_http::Response::from_file(File::open(&Path::new("image.png")).unwrap());
//! let _ = request.respond(response);
//! ```

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::Duration;
use std::{
    io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult},
    net::{Shutdown, TcpStream, ToSocketAddrs},
};
use std::{
    sync::{mpsc, Arc},
    thread,
};

use client::ClientConnection;
pub use common::{HTTPVersion, Header, HeaderField, Method, StatusCode};
use connection::Connection;
#[cfg(feature = "socket2")]
pub use connection::SocketConfig;
pub use connection::{ConfigListenAddr, ListenAddr, Listener};
pub use request::{ReadWrite, Request};
pub use response::{Response, ResponseBox};
#[cfg(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
))]
pub use ssl::SslConfig;
pub use test::TestRequest;
use util::{MessagesQueue, RefinedTcpStream, TaskPool};

#[cfg(test)]
use fdlimit as _;
#[cfg(test)]
use rlimit as _;
#[cfg(test)]
use rustc_serialize as _;
#[cfg(test)]
use sha1_smol as _;

mod client;
mod common;
mod connection;
mod log;
mod request;
mod response;
pub mod ssl;
mod test;
mod util;

/// The main class of this library.
///
/// Destroying this object will immediately close the listening socket and the reading
///  part of all the client's connections. Requests that have already been returned by
///  the `recv()` function will not close and the responses will be transferred to the client.
#[allow(missing_debug_implementations)]
pub struct Server {
    // should be false as long as the server exists
    // when set to true, all the subtasks will close within a few hundreds ms
    close: Arc<AtomicBool>,

    // queue for messages received by child threads
    messages: Arc<MessagesQueue<Message>>,

    // result of TcpListener::local_addr()
    listening_addr: ListenAddr,
}

enum Message {
    Error(IoError),
    NewRequest(Request),
}

impl From<IoError> for Message {
    fn from(e: IoError) -> Message {
        Message::Error(e)
    }
}

impl From<Request> for Message {
    fn from(rq: Request) -> Message {
        Message::NewRequest(rq)
    }
}

// this trait is to make sure that Server implements Share and Send
#[doc(hidden)]
trait SyncSendT: Sync + Send {}
#[doc(hidden)]
impl SyncSendT for Server {}

/// Iterator over received [Request] from [Server]
#[allow(missing_debug_implementations)]
pub struct IncomingRequests<'a> {
    server: &'a Server,
}

impl Iterator for IncomingRequests<'_> {
    type Item = Request;

    fn next(&mut self) -> Option<Request> {
        self.server.recv().ok()
    }
}

/// Represents the parameters required to create a server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The addresses to try to listen to.
    pub addr: ConfigListenAddr,

    /// Socket configuration with _socket2_ feature  
    /// See [SocketConfig]
    #[cfg(feature = "socket2")]
    pub socket_config: SocketConfig,

    /// If `Some`, then the server will use SSL to encode the communications.
    #[cfg(any(
        feature = "ssl-openssl",
        feature = "ssl-rustls",
        feature = "ssl-native-tls"
    ))]
    pub ssl: Option<SslConfig>,
}

impl Server {
    /// Builds a new server that listens on the specified address.
    ///
    /// # Errors
    ///
    /// `std::io::Error` when socket binding failed
    ///
    pub fn new(config: &ServerConfig) -> Result<Server, Box<dyn Error + Send + Sync + 'static>> {
        #[cfg(feature = "socket2")]
        let listener = config.addr.bind(&config.socket_config)?;
        #[cfg(not(feature = "socket2"))]
        let listener = config.addr.bind()?;

        Self::from_listener(
            listener,
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            config.ssl.as_ref(),
        )
    }

    /// Shortcut for a simple server on a specific address.
    ///
    /// # Errors
    ///
    /// `std::io::Error` when `addr` is no socket address
    #[inline]
    pub fn http<A>(addr: A) -> Result<Server, Box<dyn Error + Send + Sync + 'static>>
    where
        A: ToSocketAddrs,
    {
        Server::new(&ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs(addr)?,
            #[cfg(feature = "socket2")]
            socket_config: connection::SocketConfig::default(),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            ssl: None,
        })
    }

    /// Shortcut for a UNIX socket server at a specific path
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
    ) -> Result<Server, Box<dyn Error + Send + Sync + 'static>> {
        Server::new(&ServerConfig {
            addr: ConfigListenAddr::unix_from_path(path),
            #[cfg(feature = "socket2")]
            socket_config: connection::SocketConfig::default(),
            #[cfg(any(
                feature = "ssl-openssl",
                feature = "ssl-rustls",
                feature = "ssl-native-tls"
            ))]
            ssl: None,
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
        config: SslConfig,
    ) -> Result<Server, Box<dyn Error + Send + Sync + 'static>>
    where
        A: ToSocketAddrs,
    {
        Server::new(&ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs(addr)?,
            #[cfg(feature = "socket2")]
            socket_config: connection::SocketConfig::default(),
            ssl: Some(config),
        })
    }

    /// Builds a new server using the specified TCP listener.
    ///
    /// This is useful if you've constructed `TcpListener` using some less usual method
    /// such as from systemd. For other cases, you probably want the `new()` function.
    ///
    /// # Errors
    ///
    /// - `std::io::Error` when socket problem
    ///
    pub fn from_listener<L: Into<Listener>>(
        listener: L,
        #[cfg(any(
            feature = "ssl-openssl",
            feature = "ssl-rustls",
            feature = "ssl-native-tls"
        ))]
        ssl_config: Option<&SslConfig>,
    ) -> Result<Server, Box<dyn Error + Send + Sync + 'static>> {
        // building the TcpListener
        let listener = listener.into();
        let (server, local_addr) = {
            let local_addr = listener.local_addr()?;
            log::debug!("server listening on {}", local_addr);
            (listener, local_addr)
        };

        // creating a task where server.accept() is continuously called
        // and ClientConnection objects are pushed in the messages queue
        let messages = MessagesQueue::with_capacity(8);
        let inside_messages = Arc::clone(&messages);

        // building the "close" variable
        let close_trigger = Arc::new(AtomicBool::new(false));
        let inside_close_trigger = Arc::clone(&close_trigger);

        // a task pool is used to dispatch the connections into threads
        let task_pool = util::TaskPool::new();

        #[cfg(any(
            feature = "ssl-openssl",
            feature = "ssl-rustls",
            feature = "ssl-native-tls"
        ))]
        {
            if let Some(ssl_config) = ssl_config {
                Self::start_https_listener_thread(
                    server,
                    task_pool,
                    ssl_config,
                    inside_messages,
                    inside_close_trigger,
                )?;
            } else {
                Self::start_http_listener_thread(
                    server,
                    task_pool,
                    inside_messages,
                    inside_close_trigger,
                );
            }
        }

        #[cfg(not(any(
            feature = "ssl-openssl",
            feature = "ssl-rustls",
            feature = "ssl-native-tls"
        )))]
        Self::start_http_listener_thread(server, task_pool, inside_messages, inside_close_trigger);

        // result
        Ok(Server {
            messages,
            close: close_trigger,
            listening_addr: local_addr,
        })
    }

    /// Returns an iterator for all the incoming requests.
    ///
    /// The iterator will return `None` if the server socket is shutdown.
    #[must_use]
    #[inline]
    pub fn incoming_requests(&self) -> IncomingRequests<'_> {
        IncomingRequests { server: self }
    }

    /// Returns the number of clients currently connected to the server.
    #[must_use]
    pub fn num_connections(&self) -> usize {
        unimplemented!()
        //self.requests_receiver.lock().len()
    }

    /// Blocks until an HTTP request has been submitted and returns it.
    ///
    /// # Errors
    ///
    /// - `[Message::Error]`
    ///
    pub fn recv(&self) -> IoResult<Request> {
        match self.messages.pop() {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(rq),
            None => Err(IoError::new(IoErrorKind::Other, "thread unblocked")),
        }
    }

    /// Same as `recv()` but doesn't block longer than timeout
    ///    
    /// # Errors
    ///
    /// - `[Message::Error]`
    ///
    pub fn recv_timeout(&self, timeout: Duration) -> IoResult<Option<Request>> {
        match self.messages.pop_timeout(timeout) {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(Some(rq)),
            None => Ok(None),
        }
    }

    /// Returns the address the server is listening to.
    #[must_use]
    #[inline]
    pub fn server_addr(&self) -> ListenAddr {
        self.listening_addr.clone()
    }

    /// Same as `recv()` but doesn't block.
    ///
    /// # Errors
    ///
    /// - `[Message::Error]`
    ///
    pub fn try_recv(&self) -> IoResult<Option<Request>> {
        match self.messages.try_pop() {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(Some(rq)),
            None => Ok(None),
        }
    }

    /// Unblock thread stuck in `recv()` or `incoming_requests()`.
    /// If there are several such threads, only one is unblocked.
    /// This method allows graceful shutdown of server.
    pub fn unblock(&self) {
        self.messages.unblock();
    }

    #[inline]
    fn handle_client_connection(
        task_pool: &TaskPool,
        client_connection: ClientConnection,
        inside_messages: &Arc<MessagesQueue<Message>>,
    ) {
        let mut client = Some(client_connection);

        let messages = Arc::clone(inside_messages);

        task_pool.spawn_task(Box::new(move || {
            let client = client.take().unwrap(); // safe: checked at the beginning

            // Synchronization is needed for HTTPS requests to avoid a deadlock
            if client.secure() {
                let (sender, receiver) = mpsc::channel();
                for rq in client {
                    messages.push(rq.with_notify_sender(sender.clone()).into());
                    if let Err(err) = receiver.recv() {
                        log::error!("receiver channel hangup: {err:?}");
                        let _ = err;
                    }
                }
            } else {
                for rq in client {
                    messages.push(rq.into());
                }
            }
        }));
    }

    #[inline]
    fn start_http_listener_thread(
        server: Listener,
        task_pool: TaskPool,
        inside_messages: Arc<MessagesQueue<Message>>,
        inside_close_trigger: Arc<AtomicBool>,
    ) {
        let _ = thread::spawn(move || {
            log::debug!("running accept thread");
            while !inside_close_trigger.load(Relaxed) {
                match server.accept() {
                    Ok((sock, _)) => {
                        let (read_closable, write_closable) = RefinedTcpStream::new(sock);
                        let connection = ClientConnection::new(write_closable, read_closable);
                        Self::handle_client_connection(&task_pool, connection, &inside_messages);
                    }
                    Err(err) => {
                        log::debug!("error on connection accept: {err:?}");
                        inside_messages.push(err.into());
                    }
                };
            }
            log::debug!("terminating accept thread");
        });
    }

    #[cfg(any(
        feature = "ssl-openssl",
        feature = "ssl-rustls",
        feature = "ssl-native-tls"
    ))]
    #[inline]
    fn start_https_listener_thread(
        server: Listener,
        task_pool: TaskPool,
        ssl_config: &SslConfig,
        inside_messages: Arc<MessagesQueue<Message>>,
        inside_close_trigger: Arc<AtomicBool>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // compile check
        #[cfg(any(
            all(feature = "ssl-openssl", feature = "ssl-rustls"),
            all(feature = "ssl-openssl", feature = "ssl-native-tls"),
            all(feature = "ssl-native-tls", feature = "ssl-rustls"),
        ))]
        compile_error!("Only one feature from 'ssl-openssl', 'ssl-rustls', 'ssl-native-tls' can be enabled at the same time");

        // types
        type SslContext = crate::ssl::SslContextImpl;

        // building the SSL capabilities
        let ssl_ctx: SslContext =
            SslContext::from_pem(&ssl_config.certificate, &ssl_config.private_key)?;

        let _ = thread::spawn(move || {
            log::debug!("running accept thread");
            while !inside_close_trigger.load(Relaxed) {
                match server.accept() {
                    Ok((sock, _)) => {
                        let (read_closable, write_closable) = {
                            // trying to apply SSL over the connection
                            // if an error occurs, we just close the socket and resume listening
                            let sock = match ssl_ctx.accept(sock) {
                                Ok(s) => s,
                                Err(err) => {
                                    log::debug!("ssl handshake failed: {}", err);
                                    inside_messages.push(std::io::Error::other(err).into());
                                    continue;
                                }
                            };

                            RefinedTcpStream::new(sock)
                        };

                        let connection = ClientConnection::new(write_closable, read_closable);
                        Self::handle_client_connection(&task_pool, connection, &inside_messages);
                    }
                    Err(err) => {
                        log::debug!("error on connection accept: {err:?}");
                        inside_messages.push(err.into());
                    }
                };
            }
            log::debug!("terminating accept thread");
        });

        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // close trigger
        self.close.store(true, Relaxed);
        // Connect briefly to ourselves to unblock the accept thread
        let maybe_stream = match &self.listening_addr {
            ListenAddr::IP(addr) => TcpStream::connect(addr).map(Connection::from),
            #[cfg(unix)]
            ListenAddr::Unix(addr) => {
                // TODO: use connect_addr when its stabilized.
                let path = addr.as_pathname().unwrap();
                std::os::unix::net::UnixStream::connect(path).map(Connection::from)
            }
        };
        if let Ok(stream) = maybe_stream {
            let _ = stream.shutdown(Shutdown::Both);
        }

        #[cfg(unix)]
        if let ListenAddr::Unix(addr) = &self.listening_addr {
            if let Some(path) = addr.as_pathname() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}
