#![allow(missing_docs, unused_crate_dependencies)]

use std::sync::Arc;
use std::thread;

fn main() {
    let server = Arc::new(tiny_http::Server::http("0.0.0.0:9975").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    println!("Now listening on http://localhost:{port}/");

    let mut handles = Vec::new();

    for _ in 0..4 {
        let server = server.clone();

        handles.push(thread::spawn(move || {
            for rq in server.incoming_requests() {
                let response = tiny_http::Response::from_string("hello world".to_string());
                let _ = rq.respond(response);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}
