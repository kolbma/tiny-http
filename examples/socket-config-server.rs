#![allow(missing_docs, unused_crate_dependencies)]

#[cfg(feature = "socket2")]
use std::time::Duration;

#[cfg(feature = "socket2")]
use tiny_http::{Response, Server, ServerConfig};

#[cfg(feature = "socket2")]
fn main() -> Result<(), std::io::Error> {
    let server = Server::new(ServerConfig {
        addr: tiny_http::ConfigListenAddr::from_socket_addrs("0.0.0.0:8000")?,
        socket_config: tiny_http::SocketConfig {
            read_timeout: Duration::from_millis(5000),
            write_timeout: Duration::from_millis(5000),
            ..tiny_http::SocketConfig::default()
        },
        ssl: None,
    })
    .unwrap();

    println!("server listening 0.0.0.0:8000");

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
fn main() {}
