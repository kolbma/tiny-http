#![cfg(feature = "socket2")]
extern crate tiny_http;

use std::time::Duration;

use tiny_http::{Response, Server, ServerConfig};

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
