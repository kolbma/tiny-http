//! # Simple usage
//!
//! ## Creating the server
//!
//! The easiest way to create a server is to call __single-worker-thread__ [`Server::http()`].
//!
//! For __multi-worker-thread__ configuration use [`Server<state::MultiThreaded>`].
//!
//! The `http()` function returns an `IoResult<Server>` which will return an error
//! in the case where the server creation fails (for example if the listening port is already
//! occupied).
//!
//! ```no_run
//! let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//! ```
//!
//! A newly-created [`Server<state::SingleThreaded>`] will immediately start listening for
//! incoming connections and HTTP requests.
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
//! let mut join_handles = Vec::with_capacity(4);
//!
//! for _ in (0 .. 4) {
//!     let server = server.clone();
//!
//!     let join_handle = thread::spawn(move || {
//!         loop {
//!             let rq = server.recv().unwrap();
//!
//!             // ...
//!         }
//!     });
//!
//!     join_handles.push(join_handle);
//! }
//! ```
//!
//! There is also [`Server<state::MultiThreaded>`] to handle this scenario.
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
//!
//! # Multiple worker threads for handling requests and connections
//!
//! In a productive scenario you can use the [`Server<state::MultiThreaded>`].
//!
//! Have a look at its documentation to see how to configure and use it.
//!
//! [`Server::http()`]: ./server/struct.Server.html#method.http-1
//! [`Server<state::SingleThreaded>`]: ./server/struct.Server.html#impl-Server%3CSingleThreaded%3E
//! [`Server<state::MultiThreaded>`]: ./server/struct.Server.html#impl-Server%3CMultiThreaded%3E
//!

#[cfg(feature = "content-type")]
#[doc(inline)]
pub use common::ContentType;
pub use common::{
    connection_header, ConnectionHeader, ConnectionValue, Header, HeaderError, HeaderField,
    HeaderFieldValue, HttpVersion, HttpVersionError, Method, StatusCode,
};
pub use common::{limits, LimitsConfig};
#[cfg(feature = "range-support")]
#[doc(inline)]
pub use common::{ByteRange, RangeHeader, RangeUnit};
use connection_stream::ConnectionStream;
pub use listener_thread::{IncomingRequests, ListenerThread};
pub use request::Request;
pub use request_handler::{FnRequestHandler, RequestHandler};
#[doc(inline)]
pub use response::{Response, ResponseBox};
pub use server::State;
pub use server_config::ServerConfig;
pub use socket_config::SocketConfig;
pub use socket_listener::{ConfigListenAddr, ListenAddr, Listener};
#[cfg(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
))]
#[doc(inline)]
pub use ssl::SslConfig;
pub use test::TestRequest;

mod client;
mod common;
mod connection_stream;
mod listener_thread;
mod log;
mod request;
mod request_handler;
pub mod response;
pub mod server;
mod server_config;
mod socket_config;
mod socket_listener;
pub mod ssl;
pub mod stream_traits;
mod test;
mod util;

/// Single worker-thread server
pub type Server = server::Server<server::state::SingleThreaded>;
/// Single worker-thread server
pub type STServer = server::Server<server::state::SingleThreaded>;
/// Multiple worker-threads server, configured by [`ServerConfig`]
pub type MTServer = server::Server<server::state::MultiThreaded>;

#[cfg(test)]
mod tests {
    use base64ct as _;
    use fdlimit as _;
    use rlimit as _;
    use sha1_smol as _;
}
