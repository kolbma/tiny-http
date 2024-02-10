//! [`RequestHandler`] required for multi-threading [`MTServer`](crate::MTServer) variant
use crate::listener_thread::ListenerThread;

/// A `RequestHandler` needs to implement the trait method [`handle_requests`](RequestHandler::handle_requests)
///
/// For simple handling exists the implementation [`FnRequestHandler`].  
///
pub trait RequestHandler: Send + Sync {
    /// `handle_requests` is called and provides [`ListenerThread`] reference
    ///
    /// # Example
    ///
    /// An example `RequestHandler` implementation:
    /// ```
    /// # use tiny_http::{response, ListenerThread, RequestHandler};
    /// struct NothingFoundHandler;
    /// impl RequestHandler for NothingFoundHandler {
    ///     fn handle_requests(&self, listener: &ListenerThread) {
    ///         for rq in listener.incoming_requests() {
    ///             let _ = rq.respond(
    ///                 <&response::StandardResponse>::from(response::Standard::NotFound404).clone(),
    ///             );
    ///         }
    ///     }
    /// }
    /// ```
    fn handle_requests(&self, listener: &ListenerThread);
}

/// `FnRequestHandler` implements [`RequestHandler`]
///
/// It can be used to make an [`RequestHandler`] out of function or closure.
///
/// # Example
///
/// ```
/// # use tiny_http::{response, ListenerThread, FnRequestHandler};
/// let handler = FnRequestHandler(|listener: &ListenerThread| {
///     for rq in listener.incoming_requests() {
///         let _ = rq.respond(
///             <&response::StandardResponse>::from(response::Standard::NotFound404).clone(),
///         );
///     }
/// });
/// ```
#[allow(missing_debug_implementations)]
pub struct FnRequestHandler<T>(pub T)
where
    T: Fn(&ListenerThread);

impl<T> FnRequestHandler<T>
where
    T: Fn(&ListenerThread),
{
    #[inline]
    fn call(&self, listener: &ListenerThread) {
        (self.0)(listener);
    }
}

impl<T> RequestHandler for FnRequestHandler<T>
where
    T: Fn(&ListenerThread) + Clone + Send + Sync,
{
    #[inline]
    fn handle_requests(&self, listener: &ListenerThread) {
        self.call(listener);
    }
}

impl<T> From<T> for FnRequestHandler<T>
where
    T: Fn(&ListenerThread) + Clone + Send + Sync,
{
    fn from(f: T) -> Self {
        FnRequestHandler(f)
    }
}
