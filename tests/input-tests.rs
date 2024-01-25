#![allow(unused_crate_dependencies)]

use std::io::{Read, Write};
use std::net::Shutdown;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[allow(dead_code)]
mod support;

#[test]
fn basic_hello_world_http_1_0() {
    let mut client = support::new_client_to_hello_world_server();

    write!(client, "GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();

    assert!(content.contains("hello world"), "{}", content);
    assert!(!content.contains("Connection: Keep-Alive"), "{}", content);
}

#[test]
fn basic_hello_world_close_http_1_0() {
    let mut client = support::new_client_to_hello_world_server();

    write!(
        client,
        "GET / HTTP/1.0\r\nHost: localhost\r\nConnection: Close\r\n\r\n"
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();

    assert!(content.contains("hello world"), "{}", content);
    assert!(!content.contains("Connection: Keep-Alive"), "{}", content);
    assert!(content.contains("Connection: Close"), "{}", content);
}

#[test]
fn basic_hello_world_data_http_1_0() {
    let mut client = support::new_client_to_echo_server();

    write!(client, "POST / HTTP/1.0\r\nHost: localhost\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\n\r\nhello").unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();

    assert!(content.contains("hello"), "{}", content);
}

#[test]
fn basic_hello_world_http_1_1() {
    let mut client = support::new_client_to_hello_world_server_2(Some(3000), None);

    write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    // connection is keep-alive, so we need to close or it is waiting for server to close socket in read_to_string() below
    write!(
        client,
        "HEAD / HTTP/1.1\r\nHost: localhost\r\nConnection: Close\r\n\r\n"
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();

    assert!(content.contains("hello world"), "{}", content);
    assert!(content.contains("Connection: Keep-Alive"), "{}", content);
    assert!(content.contains("Connection: Close"), "{}", content);
}

#[test]
fn basic_hello_world_data_http_1_1() {
    let mut client = support::new_client_to_echo_server();

    write!(client, "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 11\r\n\r\nhello world").unwrap();
    // connection is keep-alive, so we need to close or it is waiting for server to close socket in read_to_string() below
    write!(
        client,
        "HEAD / HTTP/1.1\r\nHost: localhost\r\nConnection: Close\r\n\r\n"
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();

    assert!(content.contains("hello world"), "{}", content);
    assert!(content.contains("Connection: Keep-Alive"), "{}", content);
    assert!(content.contains("Connection: Close"), "{}", content);
    #[cfg(feature = "content-type")]
    assert!(
        content.contains("Content-Type: text/plain; charset=utf8"),
        "{}",
        content
    );
}

#[test]
fn basic_hello_world_close_http_1_1() {
    let mut client = support::new_client_to_hello_world_server();

    write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();

    assert!(content.contains("hello world"), "{}", content);
    assert!(!content.contains("Connection: Keep-Alive"), "{}", content);
}

#[test]
fn basic_hello_world_data_close_http_1_1() {
    let mut client = support::new_client_to_echo_server();

    write!(
        client,
        "POST / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 11\r\n\r\nhello world"
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();

    assert!(content.contains("hello world"), "{}", content);
    assert!(!content.contains("Connection: Keep-Alive"), "{}", content);
    #[cfg(feature = "content-type")]
    assert!(
        content.contains("Content-Type: text/plain; charset=utf8"),
        "{}",
        content
    );
}

#[test]
fn basic_string_input() {
    let (server, client) = support::new_one_server_one_client();

    {
        let mut client = client;
        write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\n\r\nhello").unwrap();
    }

    let mut request = server.recv().unwrap();

    let mut output = String::new();
    let _ = request.as_reader().read_to_string(&mut output).unwrap();
    assert_eq!(output, "hello", "output: {output}");
}

#[test]
fn content_length_shorter_no_close() {
    let (server, client) = support::new_one_server_one_client();

    {
        let mut client = client;
        write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 3\r\n\r\nhello").unwrap();
        // correct data after one with wrong content length
        write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\n\r\n12345").unwrap();
    }

    let result = server.recv();
    assert!(result.is_ok());

    let mut rq = result.unwrap();
    let mut content = String::new();
    let result = rq.as_reader().read_to_string(&mut content);
    assert!(result.is_ok(), "err: {:?}", result.unwrap_err());
    assert!(content.ends_with("hel"), "content: {}", content);

    rq.respond(tiny_http::Response::empty(204)).unwrap();

    // handle rest data of get request
    let rq = server.try_recv().unwrap().unwrap();
    assert_ne!(rq.method(), &tiny_http::Method::Get);
    rq.respond(tiny_http::Response::empty(204)).unwrap();
}

#[test]
fn content_length_shorter_close() {
    let (server, client) = support::new_one_server_one_client();

    {
        let mut client = client;
        write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: Close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 3\r\n\r\nhello").unwrap();
    }

    let timeout = Duration::from_millis(100);
    let now = Instant::now();
    let result = server.recv_timeout(timeout).unwrap();
    let elaps = now.elapsed();
    assert!(result.is_none());
    assert!(
        elaps >= timeout && elaps < timeout + Duration::from_millis(50),
        "elaps: {}",
        elaps.as_millis()
    );
}

#[test]
fn content_length_higher_no_close() {
    let (server, client) = support::new_one_server_one_client();

    {
        let mut client = client;
        write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 24\r\n\r\nhello").unwrap();
    }

    let timeout = Duration::from_millis(100);
    let now = Instant::now();
    let result = server.recv_timeout(timeout).unwrap();
    let elaps = now.elapsed();
    assert!(result.is_none());
    assert!(
        elaps >= timeout && elaps < timeout + Duration::from_millis(50),
        "elaps: {}",
        elaps.as_millis()
    );
}

#[test]
fn content_length_higher_close() {
    let (server, client) = support::new_one_server_one_client();

    {
        let mut client = client;
        write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: Close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 24\r\n\r\nhello").unwrap();
    }

    let timeout = Duration::from_millis(100);
    let now = Instant::now();
    let result = server.recv_timeout(timeout).unwrap();
    let elaps = now.elapsed();
    assert!(result.is_none());
    assert!(
        elaps >= timeout && elaps < timeout + Duration::from_millis(50),
        "elaps: {}",
        elaps.as_millis()
    );
}

#[test]
fn expect_100_continue() {
    let (server, client) = support::new_one_server_one_client();

    let mut client = client;
    write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nExpect: 100-continue\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\n\r\n").unwrap();
    client.flush().unwrap();

    let (tx, rx) = mpsc::channel();

    let _ = thread::spawn(move || {
        let mut request = server.recv().unwrap();
        let mut output = String::new();
        let _ = request.as_reader().read_to_string(&mut output).unwrap();
        assert_eq!(output, "hello");
        tx.send(()).unwrap();
    });

    // client.set_keepalive(Some(3)).unwrap(); FIXME: reenable this
    let mut content = vec![0; 12];
    client.read_exact(&mut content).unwrap();
    assert!(content[9..].starts_with(b"100")); // 100 status code

    write!(client, "hello").unwrap();
    client.flush().unwrap();
    client.shutdown(Shutdown::Write).unwrap();

    rx.recv().unwrap();
}

#[test]
fn unsupported_expect_header() {
    let mut client = support::new_client_to_hello_world_server();

    write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nExpect: 189-dummy\r\nContent-Type: text/plain; charset=utf8\r\n\r\n").unwrap();

    // client.set_keepalive(Some(3)).unwrap(); FIXME: reenable this
    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(content[9..].starts_with("417")); // 417 status code
}

#[test]
fn invalid_header_name() {
    let mut client = support::new_client_to_hello_world_server();

    // note the space hidden in the Content-Length, which is invalid
    write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length : 5\r\n\r\nhello").unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(content[9..].starts_with("400 Bad Request")); // 400 status code
}

#[test]
fn custom_content_type_response_header() {
    let (server, mut stream) = support::new_one_server_one_client();
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();

    let request = server.recv().unwrap();
    request
        .respond(
            tiny_http::Response::from_string("{\"custom\": \"Content-Type\"}")
                .with_header(
                    "Content-Type: application/json"
                        .parse::<tiny_http::Header>()
                        .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();

    let mut content = String::new();
    let _ = stream.read_to_string(&mut content).unwrap();

    assert!(content.ends_with("{\"custom\": \"Content-Type\"}"));
    assert_ne!(content.find("Content-Type: application/json"), None);
}

#[test]
fn too_long_header_field() {
    let just_ok_buf = String::from_utf8([b'X'; 2048 - 21].to_vec()).unwrap();
    assert_eq!(just_ok_buf.len(), 2048 - 21);

    let mut client = support::new_client_to_hello_world_server();

    // in limit
    write!(client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\nX-A-Too-Long-Field: {}\r\n\r\nhello", &just_ok_buf
    ).unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(&content[9..].starts_with("200 OK"), "{}", &content); // 200 status with body

    // out of limit
    let mut client = support::new_client_to_hello_world_server();

    // one more byte (1)
    write!(client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\nX-A-Too-Long-Field: {}1\r\n\r\nhello", &just_ok_buf
    ).unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(&content[9..].starts_with("431 Request Header Fields Too Large")); // 431 status code
}

#[test]
fn too_long_header_http_1_0() {
    let data = String::from_utf8([b'X'; 1024].to_vec()).unwrap();
    assert_eq!(data.len(), 1024);

    // out of limit
    let mut client = support::new_client_to_hello_world_server();

    // one more byte (748)
    write!(client,
        "GET / HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\n"
    ).unwrap();
    for n in 0..7 {
        write!(client, "X-A-Too-Long-Field-{}: {}\r\n", n, &data).unwrap();
    }
    write!(
        client,
        "X-A-Too-Long-Field-7: {}\r\n\r\nhello",
        data.split_at(748).0
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(&content[9..].starts_with("431 Request Header Fields Too Large")); // 431 status code
}

#[test]
fn too_long_header_http_1_1() {
    let data = String::from_utf8([b'X'; 1024].to_vec()).unwrap();
    assert_eq!(data.len(), 1024);

    let mut client = support::new_client_to_hello_world_server();

    // in limit
    write!(client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\n"
    ).unwrap();
    for n in 0..7 {
        write!(client, "X-A-Too-Long-Field-{}: {}\r\n", n, &data).unwrap();
    }
    write!(
        client,
        "X-A-Too-Long-Field-7: {}\r\n\r\nhello",
        data.split_at(747).0
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(&content[9..].starts_with("200 OK"), "{}", &content); // 200 status with body

    // out of limit
    let mut client = support::new_client_to_hello_world_server();

    // one more byte (748)
    write!(client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 5\r\n"
    ).unwrap();
    for n in 0..7 {
        write!(client, "X-A-Too-Long-Field-{}: {}\r\n", n, &data).unwrap();
    }
    write!(
        client,
        "X-A-Too-Long-Field-7: {}\r\n\r\nhello",
        data.split_at(748).0
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(&content[9..].starts_with("431 Request Header Fields Too Large")); // 431 status code
}
