#![allow(missing_docs, unused_crate_dependencies)]

use tiny_http::{response, ListenerThread, MTServer as Server, RequestHandler, ServerConfig};

#[derive(Clone)]
struct RqHandler;

// exits after each thread responded 5 requests (with browser exists favicon request, so 2)
impl RequestHandler for RqHandler {
    fn handle_requests(&self, listener: &ListenerThread) {
        let stop_handling = 5;
        for (count, rq) in listener.incoming_requests().enumerate() {
            let mut response = None;
            if let Some(ct) = rq.header_first(b"Accept") {
                if ct.value.as_str().contains("text/html") {
                    if count < stop_handling {
                        response = Some(tiny_http::Response::from_str("hello world"));
                    } else {
                        response = Some(tiny_http::Response::from_str("exiting thread..."));
                    }
                }
            }
            if let Some(response) = response {
                let _ = rq.respond(response);
            } else {
                let _ = rq.respond(
                    <&response::StandardResponse>::from(response::Standard::NotFound404).clone(),
                );
            }

            if count >= stop_handling {
                break;
            }
        }
    }
}

fn main() -> Result<(), std::io::Error> {
    let mut server = Server::new(&ServerConfig {
        addr: tiny_http::ConfigListenAddr::from_socket_addrs("127.0.0.1:9975")?,
        worker_thread_nr: 2,
        ..ServerConfig::default()
    })?
    .add_request_handler(RqHandler);

    let port = server.server_addr().port().unwrap();

    println!("Now listening on http://localhost:{port}/");

    server.wait_for_exit(None);

    Ok(())
}
