#![allow(unused_crate_dependencies)]
#![cfg(feature = "range-support")]

use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};

#[allow(dead_code)]
mod support;

#[test]
fn range_head_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "HEAD / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1"), "content: {}", content);
    assert!(
        content.contains("Accept-Ranges: bytes"),
        "content: {}",
        content
    );
    assert!(!content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_http_1_0_not_supported_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.0\r\nHost: localhost\r\nRange: bytes=0-0\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.0"), "content: {}", content);
    assert!(!content.contains("Accept-Ranges:"), "content: {}", content);
    assert!(!content.contains("Content-Range:"), "content: {}", content);
}

#[test]
fn range_http_1_1_h_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=0-0\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 206 "), "content: {}", content);
    assert!(content.contains("Content-Range:"), "content: {}", content);
    assert!(
        content.contains("Content-Length: 1\r"),
        "content: {}",
        content
    );
    assert!(!content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_http_1_1_ello_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=1-4\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 206 "), "content: {}", content);
    assert!(content.contains("Content-Range:"), "content: {}", content);
    assert!(
        content.contains("Content-Length: 4\r"),
        "content: {}",
        content
    );
    assert!(!content.contains("hello world"), "content: {}", content);
    assert!(content.contains("ello"), "content: {}", content);
}

#[test]
fn range_http_1_1_world1_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=-5\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 206 "), "content: {}", content);
    assert!(content.contains("Content-Range:"), "content: {}", content);
    assert!(
        content.contains("Content-Length: 5\r"),
        "content: {}",
        content
    );
    assert!(!content.contains("hello world"), "content: {}", content);
    assert!(content.contains("world"), "content: {}", content);
}

#[test]
fn range_http_1_1_world2_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=6-10\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 206 "), "content: {}", content);
    assert!(content.contains("Content-Range:"), "content: {}", content);
    assert!(
        content.contains("Content-Length: 5\r"),
        "content: {}",
        content
    );
    assert!(!content.contains("hello world"), "content: {}", content);
    assert!(content.contains("world"), "content: {}", content);
}

#[test]
fn range_http_1_1_world3_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=6-\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 206 "), "content: {}", content);
    assert!(content.contains("Content-Range:"), "content: {}", content);
    assert!(
        content.contains("Content-Length: 5\r"),
        "content: {}",
        content
    );
    assert!(!content.contains("hello world"), "content: {}", content);
    assert!(content.contains("world"), "content: {}", content);
}

#[test]
fn range_http_1_1_world4_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=-11\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 206 "), "content: {}", content);
    assert!(content.contains("Content-Range:"), "content: {}", content);
    assert!(
        content.contains("Content-Length: 11\r"),
        "content: {}",
        content
    );
    assert!(content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_unset_test() {
    let (server, mut client) = support::new_one_server_one_client();

    let _ = thread::spawn(move || {
        let mut cycles = 3000 / 20;

        loop {
            if let Ok(Some(rq)) = server.try_recv() {
                rq.range_unset();
                let response = tiny_http::Response::from_string("hello world".to_string());
                let _ = rq.respond(response).unwrap();
            }

            thread::sleep(Duration::from_millis(20));

            cycles -= 1;
            if cycles == 0 {
                break;
            }
        }
    });

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=0-0\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 200 "), "content: {}", content);
    assert!(!content.contains("Content-Range:"), "content: {}", content);
    assert!(
        content.contains("Content-Length: 11\r"),
        "content: {}",
        content
    );
    assert!(content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_unsatified_first_pos_overflow_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=11-\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 416 "), "content: {}", content);
    assert!(!content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_unsatified_last_pos_overflow_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=0-11\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 416 "), "content: {}", content);
    assert!(!content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_unsatified_last_pos_underflow_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=-12\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 416 "), "content: {}", content);
    assert!(!content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_unsatified_overflow1_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=0-{}\r\n\r\n",
        usize::MAX
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 416 "), "content: {}", content);
    assert!(!content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_unsatified_overflow2_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes={}\r\n\r\n",
        usize::MAX
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 416 "), "content: {}", content);
    assert!(!content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_unsatified_no_number_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=ab-ef\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 416 "), "content: {}", content);
    assert!(!content.contains("hello world"), "content: {}", content);
}

#[test]
fn range_unsatified_multi_range_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=0-4,-5\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 416 "), "content: {}", content);
    assert!(!content.contains("hello world"), "content: {}", content);
    assert!(!content.contains("hello"), "content: {}", content);
}

#[test]
fn range_unknown_range_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: foobar=0-4\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 200 "), "content: {}", content);
    assert!(content.contains("hello world"), "content: {}", content);
    assert!(!content.contains("Content-Range:"), "content: {}", content);
}

#[test]
fn range_none_range_test() {
    let mut client = support::new_client_to_hello_world_server();

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: none\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 200 "), "content: {}", content);
    assert!(content.contains("hello world"), "content: {}", content);
}

#[test]
fn chunked_range_test() {
    let data = &[b'A'; 65536];

    let (server, mut client) = support::new_one_server_one_client();

    let _ = thread::spawn(move || {
        let mut cycles = 3000 / 20;

        loop {
            if let Ok(Some(rq)) = server.try_recv() {
                let response = tiny_http::Response::from_slice(data);
                let _ = rq.respond(response).unwrap();
            }

            thread::sleep(Duration::from_millis(20));

            cycles -= 1;
            if cycles == 0 {
                break;
            }
        }
    });

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=20000-\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 206 "), "content: {}", content);
    assert!(content.contains("Content-Range:"), "content: {}", content);
    assert!(!content.contains("Content-Length:"), "content: {}", content);
    assert!(content.contains("AAAAAAAAAA"), "content: {}", content);
}

#[test]
fn non_status_2xx_range_test() {
    let (server, mut client) = support::new_one_server_one_client();

    let _ = thread::spawn(move || {
        let mut cycles = 3000 / 20;

        loop {
            if let Ok(Some(rq)) = server.try_recv() {
                let response = tiny_http::Response::empty(403);
                let _ = rq.respond(response).unwrap();
            }

            thread::sleep(Duration::from_millis(20));

            cycles -= 1;
            if cycles == 0 {
                break;
            }
        }
    });

    let _ = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nRange: bytes=20000-\r\n\r\n"
    );

    let mut content = String::new();
    let _ = client.read_to_string(&mut content);

    assert!(content.starts_with("HTTP/1.1 403 "), "content: {}", content);
    assert!(!content.contains("Content-Range:"), "content: {}", content);
    assert!(content.contains("Content-Length:"), "content: {}", content);
    assert!(content.contains("Forbidden"), "content: {}", content);
}
