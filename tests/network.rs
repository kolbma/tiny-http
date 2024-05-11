#![allow(unused_crate_dependencies)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tiny_http::{HttpVersion, LimitsConfig, ServerConfig};

#[allow(dead_code)]
mod support;

fn connection_close(v: HttpVersion) {
    let mut client = support::new_client_to_hello_world_server();

    if v == HttpVersion::Version1_1 {
        write!(client, "GET / HTTP/{v}\r\nConnection: keep-alive\r\n\r\n").unwrap();
        thread::sleep(Duration::from_millis(100));

        write!(client, "GET / HTTP/{v}\r\nConnection: close\r\n\r\n").unwrap();
    } else {
        write!(client, "GET / HTTP/{v}\r\nHost: localhost\r\n\r\n").unwrap();
    }

    let mut out = String::new();
    let _ = client.read_to_string(&mut out).unwrap();

    assert!(out.contains("hello world"), "out: {}", out);

    client
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    let result = write!(client, "GET / HTTP/{v}\r\nHost: localhost\r\n\r\n");
    assert!(result.is_err(), "server didn't close connection");

    let result = client.read_to_end(&mut Vec::new());
    assert!(
        result.is_ok() && result.unwrap() == 0,
        "client socket closed by timeout"
    );
}

#[test]
fn connection_close_http_1_0() {
    connection_close(HttpVersion::Version1_0);
}

#[test]
fn connection_close_http_1_1() {
    connection_close(HttpVersion::Version1_0);
}

fn connection_close_socket_detect_content(v: HttpVersion) {
    let timeout = Duration::from_millis(100);

    let (server, mut client) = support::new_server_client_with_cfg(&tiny_http::SocketConfig {
        read_timeout: timeout,
        write_timeout: timeout,
        ..tiny_http::SocketConfig::default()
    });

    let jh = thread::spawn(move || {
        let now = Instant::now();

        let result = server.recv_timeout(2 * timeout);

        assert!(
            result.as_ref().is_ok() && result.as_ref().unwrap().is_none(),
            "result: {:?}",
            result
        );

        let elaps = now.elapsed();
        assert!(
            elaps > 2 * timeout && elaps < 2 * timeout + Duration::from_millis(100),
            "elaps: {}",
            elaps.as_millis()
        );
    });

    let now = Instant::now();

    let client_timeout = Duration::from_millis(5);

    write!(client, "GET /").unwrap();
    client.flush().unwrap();

    write!(client, " HTTP/{v}").unwrap();
    thread::sleep(client_timeout);
    client.flush().unwrap();

    writeln!(client, "\r").unwrap();
    thread::sleep(client_timeout);

    write!(client, "Host: localhost\r\n").unwrap();
    thread::sleep(client_timeout);

    write!(client, "Content-Length: 11\r\n\r").unwrap();
    thread::sleep(client_timeout);
    writeln!(client).unwrap();
    client.flush().unwrap();

    write!(client, "hello ").unwrap();
    client.flush().unwrap();

    client.shutdown(std::net::Shutdown::Write).unwrap();

    thread::sleep(timeout);

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert_eq!(&content[9..24], "400 Bad Request", "content: {content}");

    // write!(client, "world\r\n").unwrap();
    // client.flush().unwrap();

    let elaps = now.elapsed();

    assert!(
        elaps > timeout && elaps < timeout + Duration::from_millis(100),
        "elaps: {}",
        elaps.as_millis()
    );

    let join = jh.join();
    assert!(join.is_ok(), "join: {:?}", join);
}

fn connection_close_socket_detect_header(v: HttpVersion) {
    let timeout = Duration::from_millis(100);

    let (server, mut client) = support::new_server_client_with_cfg(&tiny_http::SocketConfig {
        read_timeout: timeout,
        write_timeout: timeout,
        ..tiny_http::SocketConfig::default()
    });

    let jh = thread::spawn(move || {
        let now = Instant::now();

        let result = server.recv_timeout(2 * timeout);

        assert!(
            result.as_ref().is_ok() && result.as_ref().unwrap().is_none(),
            "result: {:?}",
            result
        );

        let elaps = now.elapsed();
        assert!(
            elaps > 2 * timeout && elaps < 2 * timeout + Duration::from_millis(100),
            "elaps: {}",
            elaps.as_millis()
        );
    });

    let now = Instant::now();

    let client_timeout = Duration::from_millis(5);

    write!(client, "GET /").unwrap();
    client.flush().unwrap();

    write!(client, " HTTP/{v}").unwrap();
    thread::sleep(client_timeout);
    client.flush().unwrap();

    client.shutdown(std::net::Shutdown::Write).unwrap();

    thread::sleep(timeout);

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert_eq!(&content[9..28], "408 Request Timeout", "content: {content}");

    let elaps = now.elapsed();

    assert!(
        elaps > timeout && elaps < timeout + Duration::from_millis(100),
        "elaps: {}",
        elaps.as_millis()
    );

    let join = jh.join();
    assert!(join.is_ok(), "join: {:?}", join);
}

#[test]
fn connection_close_socket_detect_http_1_0() {
    connection_close_socket_detect_header(HttpVersion::Version1_0);
    connection_close_socket_detect_content(HttpVersion::Version1_0);
}

#[test]
fn connection_close_socket_detect_http_1_1() {
    connection_close_socket_detect_header(HttpVersion::Version1_1);
    connection_close_socket_detect_content(HttpVersion::Version1_1);
}

#[test]
fn poor_network_test() {
    let mut client = support::new_client_to_hello_world_server();

    write!(client, "G").unwrap();
    thread::sleep(Duration::from_millis(100));
    write!(client, "ET /he").unwrap();
    thread::sleep(Duration::from_millis(100));
    write!(client, "llo HT").unwrap();
    thread::sleep(Duration::from_millis(100));
    write!(client, "TP/1.").unwrap();
    thread::sleep(Duration::from_millis(100));
    write!(client, "1\r\nHo").unwrap();
    thread::sleep(Duration::from_millis(100));
    write!(client, "st: localho").unwrap();
    thread::sleep(Duration::from_millis(100));
    write!(client, "st\r\nConnec").unwrap();
    thread::sleep(Duration::from_millis(100));
    write!(client, "tion: close\r").unwrap();
    thread::sleep(Duration::from_millis(100));
    write!(client, "\n\r").unwrap();
    thread::sleep(Duration::from_millis(100));
    writeln!(client).unwrap();

    let mut data = String::new();
    let _ = client.read_to_string(&mut data).unwrap();
    assert!(data.ends_with("hello world"));
}

#[test]
fn pipelining_test() {
    let mut client = support::new_client_to_hello_world_server();

    write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    write!(client, "GET /hello HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    write!(
        client,
        "GET /world HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();

    let mut data = String::new();
    let _ = client.read_to_string(&mut data).unwrap();
    assert_eq!(data.split("hello world").count(), 4, "data:\r\n{data}");
}

#[test]
fn server_crash_results_in_response() {
    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let ip = server.server_addr().ip().unwrap();
    let port = server.server_addr().port().unwrap();
    let mut client = TcpStream::connect((ip, port)).unwrap();

    let _ = thread::spawn(move || {
        let _ = server.recv().unwrap();
        // oops, server crash
    });

    write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(&content[9..].starts_with('5')); // 5xx status code
}

#[test]
fn responses_reordered() {
    let (server, mut client) = support::new_one_server_one_client();

    write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();

    let _ = thread::spawn(move || {
        let rq1 = server.recv().unwrap();
        let rq2 = server.recv().unwrap();

        let _ = thread::spawn(move || {
            rq2.respond(tiny_http::Response::from_string(
                "second request".to_owned(),
            ))
            .unwrap();
        });

        thread::sleep(Duration::from_millis(100));

        let _ = thread::spawn(move || {
            rq1.respond(tiny_http::Response::from_string("first request".to_owned()))
                .unwrap();
        });
    });

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(content.ends_with("second request"));
}

#[test]
fn no_transfer_encoding_on_204() {
    let (server, mut client) = support::new_one_server_one_client();

    write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nTE: chunked\r\nConnection: close\r\n\r\n"
    )
    .unwrap();

    let _ = thread::spawn(move || {
        let rq = server.recv().unwrap();

        let resp = tiny_http::Response::empty(tiny_http::StatusCode(204));
        rq.respond(resp).unwrap();
    });

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();

    assert!(content.starts_with("HTTP/1.1 204"));
    assert!(!content.contains("Transfer-Encoding: chunked"));
}

#[test]
fn chunked_threshold() {
    let resp = tiny_http::Response::from_string("test".to_string());
    assert_eq!(resp.chunked_threshold(), 32768);
    assert_eq!(resp.with_chunked_threshold(42).chunked_threshold(), 42);
}

#[test]
fn server_connection_limit_test() {
    let server = Arc::new(
        tiny_http::Server::new(&ServerConfig {
            limits: LimitsConfig {
                connection_limit: 10,
                ..LimitsConfig::default()
            },
            ..ServerConfig::default()
        })
        .unwrap(),
    );
    let port = server.server_addr().port().unwrap();
    let ip = server.server_addr().ip().unwrap();
    let mut clients = Vec::new();

    let inner_server = Arc::clone(&server);

    let _ = thread::spawn(move || while let Some(_rq) = inner_server.incoming_requests().next() {});

    for n in 1..=10 {
        let stream = TcpStream::connect((ip, port));
        assert!(
            stream.is_ok(),
            "stream error: {:?}: {}:{}",
            stream.unwrap_err(),
            ip,
            port
        );
        clients.push(stream.unwrap());

        let now = Instant::now();
        while server.num_connections() < n {
            assert!(now.elapsed() < Duration::from_millis(5000));
        }
    }

    for _ in 0..100 {
        let stream = TcpStream::connect((ip, port));
        assert!(
            stream.is_ok(),
            "stream error: {:?}: {}:{}",
            stream.unwrap_err(),
            ip,
            port
        );
        clients.push(stream.unwrap());
    }

    thread::sleep(Duration::from_millis(1000));
    assert_eq!(server.num_connections(), 10);

    for client in &mut clients {
        let _ = write!(
            client,
            "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        );
    }

    thread::sleep(Duration::from_millis(500));
    assert_eq!(server.num_connections(), 0);
}

#[test]
fn supported_http_versions_test() {
    fn assert_contains(s: u16, v: &str, content: &mut String) {
        assert!(
            content.contains(&format!("HTTP/{v} {s}")),
            "v: {} content: {}",
            v,
            content
        );
        content.clear();
    }

    fn check_client_close(client: TcpStream, content: &str) -> TcpStream {
        if content.contains("HTTP/1.0")
            || content.to_lowercase().contains("connection: close")
            || !content.contains("HTTP/")
        {
            let client = support::new_client_to_hello_world_server();
            let _ = client.set_read_timeout(Some(Duration::from_millis(100)));
            client
        } else {
            client
        }
    }

    let mut client = support::new_client_to_hello_world_server();
    let _ = client.set_read_timeout(Some(Duration::from_millis(100)));

    let mut content = String::new();

    write!(client, "GET / HTTP/0.9\r\n").unwrap();
    let _ = client.flush();

    let _ = client.read_to_string(&mut content);
    client = check_client_close(client, &content);

    #[cfg(feature = "http-0-9")]
    assert_eq!(&mut content, "hello world");
    #[cfg(not(feature = "http-0-9"))]
    {
        assert!(
            content.ends_with("HTTP Version Not Supported"),
            "content: {}",
            content
        );
        assert_contains(505, "1.0", &mut content);
    }

    for v in ["1.0", "1.1"] {
        write!(client, "GET / HTTP/{v}\r\nHost: localhost\r\n\r\n").unwrap();
        let _ = client.flush();

        let _ = client.read_to_string(&mut content);
        client = check_client_close(client, &content);
        assert_contains(200, v, &mut content);
    }

    write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let _ = client.flush();

    let _ = client.read_to_string(&mut content);
    client = check_client_close(client, &content);
    assert_contains(200, "1.1", &mut content);

    write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let _ = client.flush();

    let _ = client.read_to_string(&mut content);
    client = check_client_close(client, &content);
    assert_contains(200, "1.1", &mut content);

    write!(
        client,
        "GET / HTTP/1.0\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n"
    )
    .unwrap();
    let _ = client.flush();

    let _ = client.read_to_string(&mut content);
    assert_contains(200, "1.1", &mut content);

    write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let _ = client.flush();

    let _ = client.read_to_string(&mut content);
    client = check_client_close(client, &content);
    assert_contains(200, "1.1", &mut content);

    for v in ["2", "2.0", "3", "3.0", "2.9", "4.0"] {
        write!(client, "GET / HTTP/{v}\r\nHost: localhost\r\n\r\n").unwrap();
        let _ = client.flush();

        let _ = client.read_to_string(&mut content);
        assert!(!content.contains(&format!("HTTP/{v} 200")), "v: {}", v);
        client = check_client_close(client, &content);
        assert_contains(505, "1.0", &mut content);
    }
}
