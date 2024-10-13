#![allow(unused_crate_dependencies)]
#![cfg(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
))]

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use support::create_client;
use tiny_http::{
    response, ConfigListenAddr, FnRequestHandler, ListenerThread, ServerConfig, SslConfig,
};

#[allow(dead_code)]
mod support;

/// With openssl there is handshake fail error if connecting with HTTP to HTTPS endpoint.
/// This needs to be handled and request handling threads shouldn't exit.
/// When all threads exited there would be an ungraceful server exit.
#[test]
fn ssl_handshake_fail_test() {
    let mut server = tiny_http::MTServer::new(&ServerConfig {
        addr: ConfigListenAddr::from_socket_addrs("127.0.0.1:0").unwrap(),
        exit_graceful_timeout: Duration::from_secs(1),
        ssl: Some(SslConfig::new(
            include_bytes!("./ssl_tests/cert.pem").to_vec(),
            include_bytes!("./ssl_tests/key.pem").to_vec(),
        )),
        worker_thread_nr: 2,
        ..ServerConfig::default()
    })
    .unwrap()
    .add_request_handler(FnRequestHandler(|listener: &ListenerThread| {
        for rq in listener.incoming_requests() {
            let _ = rq.respond(
                <&response::StandardResponse>::from(response::Standard::NotFound404).clone(),
            );
        }
    }));
    let port = server.server_addr().port().unwrap();

    let header = "\
User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0\r\n\
Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/png,image/svg+xml,*/*;q=0.8\r\n\
Accept-Language: de-DE,de;q=0.8,en-US;q=0.5,en;q=0.3\r\n\
Accept-Encoding: gzip, deflate, br, zstd\r\n\
Connection: keep-alive\r\n\
Upgrade-Insecure-Requests: 1\r\n\
Sec-Fetch-Dest: document\r\n\
Sec-Fetch-Mode: navigate\r\n\
Sec-Fetch-Site: none\r\n\
Sec-Fetch-User: ?1\r\n\
Priority: u=0, i\r\n";

    let mut clients = Vec::new();

    thread::sleep(Duration::from_millis(100)); // server needs some time to startup

    let stop_thread = Arc::new(AtomicBool::default());
    let inner_stop_thread = Arc::clone(&stop_thread);

    let jh = thread::spawn(move || {
        for _ in 0..10 {
            clients.push(create_client(
                ("127.0.0.1", port),
                Some(Duration::from_secs(2)),
                None,
            ));
        }

        while !inner_stop_thread.load(Ordering::Relaxed) {
            if let Some(mut client) = clients.pop() {
                clients.push(create_client(
                    ("127.0.0.1", port),
                    Some(Duration::from_secs(2)),
                    None,
                ));
                let _ = write!(
                    client,
                    "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{header}\r\n"
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
    });

    thread::sleep(Duration::from_secs(2)); // wait some time for connections in thread
    assert_ne!(server.num_connections(), 0); // still have connections

    stop_thread.store(true, Ordering::Relaxed);
    let _ = jh.join();

    let stop_server = Arc::new(AtomicBool::default());
    let inner_stop_server = Arc::clone(&stop_server);

    let jh = thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        inner_stop_server.store(true, Ordering::Relaxed);
    });

    server.wait_for_exit(Some(&stop_server));
    let _ = jh.join();
}
