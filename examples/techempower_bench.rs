#![allow(missing_docs, unused_crate_dependencies)]

use std::sync::Arc;
use std::thread;

use serde::Serialize;
use tiny_http::Header;

#[derive(Serialize)]
struct HelloWorldMsg {
    message: &'static str,
}

fn main() {
    let server = Arc::new(tiny_http::Server::http("127.0.0.1:8082").unwrap());

    let mut handles = Vec::new();
    let mut response_json = tiny_http::Response::empty(200);
    let _ = response_json.add_header("Content-Type: application/json".parse::<Header>().unwrap());
    let _ = response_json.add_header("Server: t".parse::<Header>().unwrap());

    let mut response_text = response_json.clone();
    let _ = response_text.add_header("Content-Type: plain/text".parse::<Header>().unwrap());

    for _ in 0..num_cpus::get() {
        let server = server.clone();
        let response_json = response_json.clone();
        let response_text = response_text.clone();

        handles.push(thread::spawn(move || {
            for req in server.incoming_requests() {
                match req.url() {
                    "/json" => {
                        let json = serde_json::to_vec(&HelloWorldMsg {
                            message: "Hello, World!",
                        })
                        .expect("json ser fail");
                        let _ = req
                            .respond(response_json.clone().with_data(&json[..], Some(json.len())));
                    }
                    "/plaintext" => {
                        let text = b"Hello, World!";
                        let _ = req
                            .respond(response_text.clone().with_data(&text[..], Some(text.len())));
                    }
                    _ => {
                        let _ = req.respond(tiny_http::Response::empty(404));
                    }
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}
