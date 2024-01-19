#![allow(unused_crate_dependencies)]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tiny_http::ServerConfig;

#[allow(dead_code)]
mod support;

#[test]
fn connection_close_header() {
    let mut client = support::new_client_to_hello_world_server();

    write!(client, "GET / HTTP/1.1\r\nConnection: keep-alive\r\n\r\n").unwrap();
    thread::sleep(Duration::from_millis(1000));

    write!(client, "GET / HTTP/1.1\r\nConnection: close\r\n\r\n").unwrap();

    // if the connection was not closed, this will err with timeout
    // client.set_keepalive(Some(1)).unwrap(); FIXME: reenable this
    let mut out = Vec::new();
    let _ = client.read_to_end(&mut out).unwrap();
}

#[test]
fn http_1_0_connection_close() {
    let mut client = support::new_client_to_hello_world_server();

    write!(client, "GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();

    // if the connection was not closed, this will err with timeout
    // client.set_keepalive(Some(1)).unwrap(); FIXME: reenable this
    let mut out = Vec::new();
    let _ = client.read_to_end(&mut out).unwrap();
}

#[test]
fn detect_connection_closed() {
    let mut client = support::new_client_to_hello_world_server();

    write!(client, "GET / HTTP/1.1\r\nConnection: keep-alive\r\n\r\n").unwrap();
    thread::sleep(Duration::from_millis(1000));

    client.shutdown(Shutdown::Write).unwrap();

    // if the connection was not closed, this will err with timeout
    // client.set_keepalive(Some(1)).unwrap(); FIXME: reenable this
    let mut out = Vec::new();
    let _ = client.read_to_end(&mut out).unwrap();
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

    // client.set_keepalive(Some(2)).unwrap(); FIXME: reenable this
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

    // client.set_keepalive(Some(2)).unwrap(); FIXME: reenable this
    let mut data = String::new();
    let _ = client.read_to_string(&mut data).unwrap();
    assert_eq!(data.split("hello world").count(), 4);
}

#[test]
fn server_crash_results_in_response() {
    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();

    let _ = thread::spawn(move || {
        let _ = server.recv().unwrap();
        // oops, server crash
    });

    write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();

    // client.set_keepalive(Some(2)).unwrap(); FIXME: reenable this
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

    // client.set_keepalive(Some(2)).unwrap(); FIXME: reenable this
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
#[cfg(feature = "socket2")]
fn connection_timeout() -> Result<(), std::io::Error> {
    use std::time::{Duration, Instant};
    use tiny_http::ServerConfig;

    let now = Instant::now();

    let (server, mut client) = {
        let server = tiny_http::Server::new(&ServerConfig {
            addr: tiny_http::ConfigListenAddr::from_socket_addrs("0.0.0.0:0")?,
            socket_config: tiny_http::SocketConfig {
                read_timeout: Duration::from_millis(100),
                write_timeout: Duration::from_millis(100),
                ..tiny_http::SocketConfig::default()
            },
            ..ServerConfig::default()
        })
        .unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        (server, client)
    };

    let _ = thread::spawn(move || {
        let rq = server.recv_timeout(Duration::from_secs(300));
        assert!(rq.is_ok(), "req fail: {}", rq.unwrap_err());

        let rq = rq.unwrap();
        assert!(rq.is_some());
        let rq = rq.unwrap();

        let resp = tiny_http::Response::empty(tiny_http::StatusCode(204));
        rq.respond(resp).unwrap();
    });

    write!(client, "GET / HTTP/1.1\r\n\r\n")?;

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(content.starts_with("HTTP/1.1 204"));

    thread::sleep(Duration::from_millis(200));

    let err = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nTE: chunked\r\nConnection: close\r\n\r\n"
    );
    assert!(err.is_ok());

    let elaps = now.elapsed();
    assert!(
        elaps > Duration::from_millis(230) && elaps < Duration::from_millis(320),
        "elaps: {}",
        elaps.as_millis()
    );

    Ok(())
}

#[test]
#[cfg(feature = "socket2")]
fn connection_timeout_wait_check() -> Result<(), std::io::Error> {
    use std::time::{Duration, Instant};
    use tiny_http::ServerConfig;

    let now = Instant::now();

    let (server, mut client) = {
        let server = tiny_http::Server::new(&ServerConfig {
            addr: tiny_http::ConfigListenAddr::from_socket_addrs("0.0.0.0:0")?,
            socket_config: tiny_http::SocketConfig {
                read_timeout: Duration::from_millis(250),
                write_timeout: Duration::from_millis(250),
                ..tiny_http::SocketConfig::default()
            },
            ..ServerConfig::default()
        })
        .unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        (server, client)
    };

    let _ = thread::spawn(move || {
        let rq = server.recv_timeout(Duration::from_secs(300));
        assert!(rq.is_err());
    });

    // make sure it is waiting longer than server timeouts
    thread::sleep(Duration::from_millis(300));

    let err = write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nTE: chunked\r\nConnection: close\r\n\r\n"
    );
    assert!(err.is_ok());

    let elaps = now.elapsed();
    assert!(
        elaps > Duration::from_millis(300) && elaps < Duration::from_millis(330),
        "elaps: {}",
        elaps.as_millis()
    );

    Ok(())
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
            connection_limit: 10,
            ..ServerConfig::default()
        })
        .unwrap(),
    );
    let port = server.server_addr().to_ip().unwrap().port();
    let ip = server.server_addr().to_ip().unwrap().ip();
    let mut clients = Vec::new();

    let inner_server = Arc::clone(&server);

    let _ = thread::spawn(move || while let Some(_rq) = inner_server.incoming_requests().next() {});

    for n in 1..=10 {
        let stream = TcpStream::connect(("127.0.0.1", port));
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
        let stream = TcpStream::connect(("127.0.0.1", port));
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
