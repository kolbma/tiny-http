#![allow(unused_crate_dependencies)]

use std::io::{Read, Write};

#[cfg(feature = "http-0-9")]
use tiny_http::HttpVersion;

#[allow(dead_code)]
mod support;

#[cfg(feature = "http-0-9")]
#[test]
fn headers_are_not_in_simple_request_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(client, "GET / HTTP/0.9\r\nHost: localhost\r\n");

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    // correct would be a bad request, but the streams are designed to not support this
    // and so we simply ignore any headers

    assert_eq!(&content, "hello world");
}

#[cfg(not(feature = "http-0-9"))]
#[test]
fn http_0_9_not_supported_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(client, "GET /\r\n");

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/"), "content: {}", content);
    assert!(
        content.contains("HTTP Version Not Supported"),
        "content: {}",
        content
    );

    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(client, "GET / HTTP/0.9\r\n");

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/"), "content: {}", content);
    assert!(
        content.contains("HTTP Version Not Supported"),
        "content: {}",
        content
    );
}

#[cfg(feature = "http-0-9")]
#[test]
fn only_get_request_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(client, "GET /\r\n");

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert_eq!(&content, "hello world");

    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(client, "HEAD /\r\n");

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert_eq!(&content, "Bad Request");

    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(client, "POST /\r\ndata\r\n");

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert_eq!(&content, "Bad Request");
}

#[cfg(feature = "http-0-9")]
#[test]
fn timeout_invalid_request_test() {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    let (server, mut client) = support::new_one_server_one_client();

    let _ = thread::spawn(move || {
        let _ = write!(client, "GET /\r\n");
        loop {
            let _ = write!(client, "Host: localhost\r\n");
            thread::sleep(Duration::from_millis(10));
        }
    });

    let now = Instant::now();

    let rq = server.recv_timeout(Duration::from_millis(200));
    let elaps = now.elapsed();

    // doesn't timeout in recv, but closes the client after request
    assert!(
        elaps < Duration::from_millis(190),
        "elaps: {}",
        elaps.as_millis()
    );

    assert!(rq.is_ok());
    let rq = rq.unwrap().unwrap();
    assert_eq!(rq.http_version(), HttpVersion::Version0_9);
}
