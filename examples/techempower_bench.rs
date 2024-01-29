#![allow(missing_docs, unused_crate_dependencies)]

use std::sync::Arc;
use std::thread;

use serde::Serialize;
use tiny_http::{ConfigListenAddr, Header, HeaderField, LimitsConfig, ServerConfig};

#[derive(Serialize)]
struct HelloWorldMsg {
    message: &'static str,
}

fn main() {
    let server = Arc::new(
        tiny_http::Server::new(&ServerConfig {
            addr: ConfigListenAddr::IP(vec!["0.0.0.0:8082".parse().unwrap()]),
            limits: LimitsConfig {
                connection_limit: 500,
                header_line_len: 128,
                ..LimitsConfig::default()
            },
            ..ServerConfig::default()
        })
        .unwrap(),
    );

    let mut handles = Vec::new();
    let mut response_json = tiny_http::Response::empty(200);
    let _ = response_json.filter_header("Connection".parse::<HeaderField>().unwrap());
    let _ = response_json.add_header("Content-Type: application/json".parse::<Header>().unwrap());
    let _ = response_json.add_header("Server: t".parse::<Header>().unwrap());

    let mut response_text = response_json.clone();
    let _ = response_text.filter_header("Connection".parse::<HeaderField>().unwrap());
    let _ = response_text.add_header("Content-Type: plain/text".parse::<Header>().unwrap());

    for _ in 0..num_cpus::get() {
        let server = server.clone();
        let response_json = response_json.clone();
        let response_text = response_text.clone();

        handles.push(thread::spawn(move || {
            for req in server.incoming_requests() {
                match req.url() {
                    "/json" => {
                        let json: &[u8] = &serde_json::to_vec(&HelloWorldMsg {
                            message: "Hello, World!",
                        })
                        .expect("json ser fail");
                        let _ =
                            req.respond(response_json.clone().with_data(json, Some(json.len())));
                    }
                    "/plaintext" => {
                        let text: &[u8] = b"Hello, World!";
                        let _ =
                            req.respond(response_text.clone().with_data(text, Some(text.len())));
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
