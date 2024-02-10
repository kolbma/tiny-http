#![allow(missing_docs, unused_crate_dependencies)]

#[cfg(feature = "socket2")]
use std::sync::Arc;
#[cfg(feature = "socket2")]
use std::time::Duration;

#[cfg(feature = "socket2")]
use tiny_http::{Response, Server, ServerConfig};

#[cfg(feature = "socket2")]
fn main() -> Result<(), std::io::Error> {
    let server = Server::new(&ServerConfig {
        addr: tiny_http::ConfigListenAddr::from_socket_addrs("0.0.0.0:9975")?,
        socket_config: Arc::new(tiny_http::SocketConfig {
            read_timeout: Duration::from_millis(5000),
            write_timeout: Duration::from_millis(5000),
            ..tiny_http::SocketConfig::default()
        }),
        ..ServerConfig::default()
    })
    .unwrap();

    let port = server.server_addr().port().unwrap();
    println!("Now listening on http://localhost:{port}/");

    for request in server.incoming_requests() {
        println!(
            "received request! method: {:?}, url: {:?}, headers: {:?}",
            request.method(),
            request.url(),
            request.headers()
        );

        let response = Response::from_string("hello world");
        request.respond(response).expect("Responded");
    }

    println!("server exit");

    Ok(())
}

#[cfg(not(feature = "socket2"))]
fn main() {
    eprintln!("socket-config-server example needs feature \"socket2\"");
}
