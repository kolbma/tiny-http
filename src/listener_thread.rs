use std::error::Error;
use std::io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult};
use std::net::{Shutdown, TcpStream};
use std::sync::{
    atomic::{AtomicBool, AtomicU16, Ordering},
    mpsc, Arc, Condvar, Mutex,
};
use std::thread;
use std::time::Duration;

use crate::client::{ClientConnection, ReadError};
use crate::connection_stream::ConnectionStream;
use crate::log;
use crate::request::Request;
use crate::server_config::{ServerConfig, CONNECTION_LIMIT_SLEEP_DURATION};
use crate::socket_listener::{ListenAddr, Listener};
#[cfg(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
))]
use crate::ssl::SslConfig;
use crate::util::{Message, MessagesQueue, RefinedTcpStream, Registration, TaskPool};
use crate::RequestHandler;

/// Starts a listener thread to accept socket connections.
///
/// Dropping the `ListenerThread` instance closes the listening socket and the reading
/// part of all the client's connections.  
/// Requests that have already been returned by the `recv()`-like function will not close
/// and the responses will be transferred to the client.
#[allow(missing_debug_implementations)]
pub struct ListenerThread {
    // should be false as long as the listener exists
    // when set to true, all the subtasks will close within a few hundreds ms
    close: Arc<AtomicBool>,

    // result of TcpListener::local_addr()
    listening_addr: ListenAddr,

    // queue for messages received by child threads
    messages: Arc<MessagesQueue<Message>>,

    // number of currently open connections
    num_connections: Arc<AtomicU16>,

    // listener thread join handle
    thread_jh: Option<thread::JoinHandle<()>>,

    // number of worker threads for multi-thread variant
    thread_nr: u8,
}

impl ListenerThread {
    /// Creates a new listener using the specified TCP listener.
    ///
    /// This is useful if you've constructed `TcpListener` using some less usual method
    /// such as from systemd. For other cases, you probably want use one of the servers.
    ///
    /// There is [`Server`] which is [`STServer`] and the variant [`MTServer`].
    ///
    /// # Errors
    ///
    /// - `std::io::Error` when socket problem
    ///
    /// [`Server`]: crate::Server
    /// [`STServer`]: crate::STServer
    /// [`MTServer`]: crate::MTServer
    ///
    #[inline]
    pub fn new<L: Into<Listener>>(
        listener: L,
        config: &ServerConfig,
        num_connections: &Arc<AtomicU16>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // building the TcpListener
        let listener = listener.into();
        let (listener, local_addr) = {
            let local_addr = listener.local_addr()?;
            log::info!("listening on {}", local_addr);
            (listener, local_addr)
        };

        // creating a task where listener.accept() is continuously called
        // and ClientConnection objects are pushed in the messages queue
        let messages = MessagesQueue::with_capacity(8);
        let inside_messages = Arc::clone(&messages);

        // building the "close" variable
        let close_trigger = Arc::new(AtomicBool::new(false));
        let inside_close_trigger = Arc::clone(&close_trigger);

        // a task pool is used to dispatch the connections into threads
        let task_pool = TaskPool::new();

        // counting number of concurrent client connections
        let num_connections = Arc::clone(num_connections);

        let thread_jh;

        #[cfg(any(
            feature = "ssl-openssl",
            feature = "ssl-rustls",
            feature = "ssl-native-tls"
        ))]
        {
            thread_jh = if let Some(ssl_config) = &config.ssl {
                Self::start_https_listener_thread(
                    listener,
                    config,
                    task_pool,
                    &num_connections,
                    ssl_config,
                    inside_messages,
                    inside_close_trigger,
                )?
            } else {
                Self::start_http_listener_thread(
                    listener,
                    config,
                    task_pool,
                    &num_connections,
                    inside_messages,
                    inside_close_trigger,
                )
            };
        }

        #[cfg(not(any(
            feature = "ssl-openssl",
            feature = "ssl-rustls",
            feature = "ssl-native-tls"
        )))]
        {
            thread_jh = Self::start_http_listener_thread(
                listener,
                config,
                task_pool,
                &num_connections,
                inside_messages,
                inside_close_trigger,
            );
        }

        // result
        Ok(Self {
            close: close_trigger,
            listening_addr: local_addr,
            messages,
            num_connections,
            thread_jh: Some(thread_jh),
            thread_nr: config.worker_thread_nr,
        })
    }

    /// Returns the address the thread is listening to.
    #[must_use]
    pub fn listening_addr(&self) -> &ListenAddr {
        &self.listening_addr
    }

    /// Returns an iterator for all the incoming requests.
    ///
    /// The iterator will return `None` if the server socket is shutdown
    /// or `OsError` occurred.
    #[must_use]
    #[inline]
    pub fn incoming_requests(&self) -> IncomingRequests<'_> {
        IncomingRequests {
            listener_thread: self,
        }
    }

    /// Returns the number of clients currently connected.
    #[must_use]
    pub fn num_connections(&self) -> u16 {
        self.num_connections.load(Ordering::Acquire)
    }

    /// Blocks until an HTTP request has been submitted and returns it.
    ///
    /// # Errors
    ///
    /// - [`std::io::Error`]
    ///
    #[inline]
    pub fn recv(&self) -> IoResult<Request> {
        match self.messages.pop() {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(rq),
            None => Err(IoError::new(IoErrorKind::Other, "thread unblocked")),
        }
    }

    /// Same as `recv()` but doesn't block longer than timeout.
    ///    
    /// # Errors
    ///
    /// - [`std::io::Error`]
    ///
    #[inline]
    pub fn recv_timeout(&self, timeout: Duration) -> IoResult<Option<Request>> {
        match self.messages.pop_timeout(timeout) {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(Some(rq)),
            None => Ok(None),
        }
    }

    /// Spawns worker threads for [`Request`] handling and calls provided [`RequestHandler`].
    ///
    /// Takes responsibility/ownership of [`ListenerThread`] to give references to `RequestHandler`.
    ///
    /// # Return
    ///
    /// Returns an reference tuple with `ListenerThread` and a tuple pair of
    /// `Mutex` guard and `Condvar`.  
    /// The guard contains the active number of threads and needs to be locked
    /// when waiting on the `Condvar` for notification of thread exits.
    ///
    /// ```no_run
    /// # let mut mtx = std::sync::Mutex::new(0);
    /// # let exit_cond = std::sync::Condvar::new();
    /// if let Ok(mut guard) = mtx.lock() {
    ///    while *guard > 0 {
    ///        guard = match exit_cond.wait(guard) {
    ///            Ok(guard) => guard,
    ///            Err(err) => unreachable!("{:?}", err),
    ///        }
    ///    }
    /// }
    /// # mtx.lock();
    /// ```
    ///
    #[inline]
    pub fn start_request_handling<T>(
        self,
        request_handler: T,
    ) -> (Arc<ListenerThread>, Arc<(Mutex<u8>, Condvar)>)
    where
        T: RequestHandler + 'static,
    {
        let exit_cond_var = Arc::new((Mutex::new(self.thread_nr), Condvar::new()));
        let lt = Arc::new(self);
        let request_handler = Arc::new(request_handler);

        for n in 1..=lt.thread_nr {
            let lt = Arc::clone(&lt);
            let inner_exit_cond = Arc::clone(&exit_cond_var);
            let request_handler = Arc::clone(&request_handler);

            if let Err(err) = thread::Builder::new().spawn(move || {
                let id = thread::current().id();
                log::debug!("enter start_request_handling [{:?}]", id);

                request_handler.handle_requests(&lt);

                {
                    let (mtx, cond) = inner_exit_cond.as_ref();
                    if let Ok(mut guard) = mtx.lock() {
                        *guard -= 1;
                        cond.notify_one();
                    } else {
                        log::error!("lock fail");
                    }
                }
                log::debug!("exit start_request_handling [{:?}]", id);
                let _ = id;
            }) {
                log::error!("aborted creating worker threads at thread {n}: {err:?}");
                let _ = n;
                let _ = err;
                break;
            }
        }

        log::info!("started worker thread(s) {}", lt.thread_nr);

        (lt, exit_cond_var)
    }

    /// Same as `recv()` but doesn't block.
    ///
    /// # Errors
    ///
    /// - [`std::io::Error`]
    ///
    #[inline]
    pub fn try_recv(&self) -> IoResult<Option<Request>> {
        match self.messages.try_pop() {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(Some(rq)),
            None => Ok(None),
        }
    }

    /// Unblock thread stuck in `recv()` or `incoming_requests()`.
    ///
    /// If there are several such threads, only one is unblocked.  
    /// This method allows graceful shutdown.
    pub fn unblock(&self) {
        self.messages.unblock();
    }

    pub(crate) fn shutdown(&self) {
        // close trigger
        self.close.store(true, Ordering::Relaxed);

        // Connect briefly to ourselves to unblock the accept thread(s)
        match &self.listening_addr {
            ListenAddr::IP(addr) => {
                // for _ in 0..self.thread_nr {
                let _ = TcpStream::connect(addr)
                    .map(ConnectionStream::from)
                    .and_then(|stream| stream.shutdown(Shutdown::Both));
                // }
            }
            #[cfg(unix)]
            ListenAddr::Unix(addr) => {
                // TODO: use connect_addr when its stabilized (since 1.70).
                if let Some(path) = addr.as_pathname() {
                    let _ = std::os::unix::net::UnixStream::connect(path)
                        .map(ConnectionStream::from)
                        .and_then(|stream| stream.shutdown(Shutdown::Both));
                    let _ = std::fs::remove_file(path);
                }
            }
        };

        for _ in 1..=self.thread_nr {
            self.unblock();
        }
    }

    #[inline]
    pub(crate) fn join_handle(&mut self) -> Option<thread::JoinHandle<()>> {
        self.thread_jh.take()
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
                    match rq {
                        Ok(rq) => {
                            messages.push(rq.with_notify_sender(sender.clone()).into());
                            if let Err(err) = receiver.recv() {
                                log::error!("receiver channel hangup: {err:?}");
                                let _ = err;
                            }
                        }
                        Err(ReadError::ReadIoError(err)) => {
                            log::debug!("message error: {err:?}");
                            messages.push(err.into());
                        }
                        _ => {}
                    }
                }
            } else {
                for rq in client {
                    match rq {
                        Ok(rq) => {
                            messages.push(rq.into());
                        }
                        Err(ReadError::ReadIoError(err)) => {
                            log::debug!("message error: {err:?}");
                            messages.push(err.into());
                        }
                        _ => {}
                    }
                }
            }
        }));
    }

    #[inline]
    fn start_http_listener_thread(
        listener: Listener,
        config: &ServerConfig,
        task_pool: TaskPool,
        num_connections: &Arc<AtomicU16>,
        inside_messages: Arc<MessagesQueue<Message>>,
        inside_close_trigger: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        let connection_limit = config.limits.connection_limit;
        let limits = Arc::new(config.limits);
        let num_connections = Arc::clone(num_connections);
        #[cfg(feature = "socket2")]
        let socket_config = Arc::clone(&config.socket_config);

        thread::spawn(move || {
            // TODO: change to thread::current().id().as_u64() when stable api
            let id = format!("{:?}", thread::current().id());

            log::debug!("running accept thread [{id}]");

            let mut cur_pool_thread_nr = 0;

            while !inside_close_trigger.load(Ordering::Relaxed) {
                while num_connections.load(Ordering::Acquire) >= connection_limit {
                    log::warn!("connection limit reached");
                    thread::sleep(CONNECTION_LIMIT_SLEEP_DURATION);
                }

                match listener.accept() {
                    Ok((sock, _)) => {
                        let client_counter = Registration::new(Arc::clone(&num_connections));

                        let (read_closable, write_closable) = RefinedTcpStream::new(sock);
                        let connection = ClientConnection::new(
                            write_closable,
                            read_closable,
                            client_counter,
                            &limits,
                            #[cfg(feature = "socket2")]
                            &socket_config,
                        );
                        Self::handle_client_connection(&task_pool, connection, &inside_messages);
                    }
                    Err(err) => {
                        log::error!("error on connection accept: {err:?}");
                        #[cfg(not(feature = "log"))]
                        eprintln!("error on connection accept: {err:?}");
                        inside_messages.push(err.into());
                        let _ = err;
                    }
                };

                if log::log_enabled!(log::Level::Info) {
                    let tt_nr = task_pool.threads_total();
                    if cur_pool_thread_nr != tt_nr {
                        log::info!("task pool thread count: {tt_nr}");
                        cur_pool_thread_nr = tt_nr;
                    }
                }
            }
            log::debug!("terminating accept thread [{id}]");
            let _ = id;
        })
    }

    #[cfg(any(
        feature = "ssl-openssl",
        feature = "ssl-rustls",
        feature = "ssl-native-tls"
    ))]
    #[inline]
    fn start_https_listener_thread(
        listener: Listener,
        config: &ServerConfig,
        task_pool: TaskPool,
        num_connections: &Arc<AtomicU16>,
        ssl_config: &SslConfig,
        inside_messages: Arc<MessagesQueue<Message>>,
        inside_close_trigger: Arc<AtomicBool>,
    ) -> Result<thread::JoinHandle<()>, Box<dyn Error + Send + Sync>> {
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

        let connection_limit = config.limits.connection_limit;
        let limits = Arc::new(config.limits);
        let num_connections = Arc::clone(num_connections);
        #[cfg(feature = "socket2")]
        let socket_config = Arc::new(config.socket_config.clone());

        let th = thread::spawn(move || {
            // TODO: change to thread::current().id().as_u64() when stable api
            let id = format!("{:?}", thread::current().id());

            log::debug!("running accept thread {id}");

            let mut cur_pool_thread_nr = 0;

            while !inside_close_trigger.load(Ordering::Relaxed) {
                while num_connections.load(Ordering::Acquire) >= connection_limit {
                    log::warn!("connection limit reached");
                    thread::sleep(CONNECTION_LIMIT_SLEEP_DURATION);
                }

                match listener.accept() {
                    Ok((sock, _)) => {
                        let client_counter = Registration::new(Arc::clone(&num_connections));

                        let (read_closable, write_closable) = {
                            // trying to apply SSL over the connection
                            // if an error occurs, we just close the socket and resume listening
                            let sock = match ssl_ctx.accept(sock) {
                                Ok(s) => s,
                                Err(err) => {
                                    log::warn!("ssl handshake failed: {}", err);
                                    inside_messages
                                        .push(IoError::new(IoErrorKind::Other, err).into());
                                    continue;
                                }
                            };

                            RefinedTcpStream::new(sock)
                        };

                        let connection = ClientConnection::new(
                            write_closable,
                            read_closable,
                            client_counter,
                            &limits,
                            #[cfg(feature = "socket2")]
                            &socket_config,
                        );
                        Self::handle_client_connection(&task_pool, connection, &inside_messages);
                    }
                    Err(err) => {
                        log::error!("error on connection accept: {err:?}");
                        #[cfg(not(feature = "log"))]
                        eprintln!("error on connection accept: {err:?}");
                        inside_messages.push(err.into());
                    }
                };

                if log::log_enabled!(log::Level::Info) {
                    let tt_nr = task_pool.threads_total();
                    if cur_pool_thread_nr != tt_nr {
                        log::info!("task pool thread count: {tt_nr}");
                        cur_pool_thread_nr = tt_nr;
                    }
                }
            }
            log::debug!("terminating accept thread {id}");
            let _ = id;
        });

        Ok(th)
    }
}

impl Drop for ListenerThread {
    fn drop(&mut self) {
        self.shutdown();

        if let Some(jh) = self.thread_jh.take() {
            let _ = jh.join();
        }
    }
}

// this trait is to make sure that ListenerThread implements Send and Sync
#[doc(hidden)]
#[allow(dead_code)]
trait SendSyncT: Send + Sync {}
#[doc(hidden)]
impl SendSyncT for ListenerThread {}

/// Iterator over received [`Request`] from [`ListenerThread`]
///
/// Returns `None` on any `Error`
///
#[allow(missing_debug_implementations)]
pub struct IncomingRequests<'a> {
    listener_thread: &'a ListenerThread,
}

impl Iterator for IncomingRequests<'_> {
    type Item = Request;

    fn next(&mut self) -> Option<Request> {
        self.listener_thread.recv().ok()
    }
}
