#![feature(test)]
#![allow(unused_crate_dependencies)]

extern crate test;

use std::io::Read;
use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tiny_http::{ListenerThread, Method, RequestHandler};

#[test]
#[ignore]
// TODO: obtain time
fn curl_bench() {
    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let port = server.server_addr().port().unwrap();
    let num_requests = 10usize;

    let _ = match Command::new("curl")
        .arg("-s")
        .arg(format!("http://localhost:{port}/?[1-{num_requests}]"))
        .output()
    {
        Ok(p) => p,
        Err(_) => return, // ignoring test
    };

    drop(server);
}

#[bench]
fn sequential_requests(bencher: &mut test::Bencher) {
    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let port = server.server_addr().port().unwrap();

    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();

    bencher.iter(|| {
        (write!(stream, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")).unwrap();

        let request = server.recv().unwrap();

        assert_eq!(request.method(), &Method::Get);

        let _ = request.respond(tiny_http::Response::empty(204));
    });
}

#[bench]
fn parallel_requests(bencher: &mut test::Bencher) {
    let _ = fdlimit::raise_fd_limit();

    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let port = server.server_addr().port().unwrap();

    bencher.iter(|| {
        let mut streams = Vec::new();

        for _ in 0..1000usize {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            (write!(
                stream,
                "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            ))
            .unwrap();
            streams.push(stream);
        }

        loop {
            let request = match server.try_recv().unwrap() {
                None => break,
                Some(rq) => rq,
            };

            assert_eq!(request.method(), &Method::Get);

            let _ = request.respond(tiny_http::Response::empty(204));
        }
    });
}

/// for bench `parallel_mt_requests`
#[derive(Clone)]
struct BenchRqHandler(Arc<AtomicBool>);
impl RequestHandler for BenchRqHandler {
    fn handle_requests(&self, listener: &ListenerThread) {
        let timeout = Duration::from_secs(1);

        while !self.0.load(Ordering::Acquire) {
            if let Ok(request) = listener.recv_timeout(timeout) {
                if let Some(request) = request {
                    let _ = request.respond(tiny_http::Response::empty(204));
                }
            } else {
                break;
            }
        }
    }
}

#[bench]
fn parallel_mt_requests(bencher: &mut test::Bencher) {
    let _ = fdlimit::raise_fd_limit();

    let cond = Arc::new(AtomicBool::default());

    let mut server = tiny_http::MTServer::new(&tiny_http::ServerConfig {
        addr: tiny_http::ConfigListenAddr::from_socket_addrs("0.0.0.0:0").unwrap(),
        worker_thread_nr: 3,
        ..tiny_http::ServerConfig::default()
    })
    .unwrap()
    .add_request_handler(BenchRqHandler(Arc::clone(&cond)));

    let port = server.server_addr().port().unwrap();

    bencher.iter(|| {
        let mut streams = Vec::new();

        for _ in 0..100 {
            for _ in 0..10 {
                let addr = ("127.0.0.1", port);
                streams.push(thread::spawn(move || {
                    let mut stream = std::net::TcpStream::connect(addr).unwrap();
                    (write!(
                        stream,
                        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                    ))
                    .unwrap();
                    stream
                }));
            }

            while let Some(jh) = streams.pop() {
                let mut s = jh.join().unwrap();
                let mut buf = String::new();
                let _ = s.read_to_string(&mut buf);
                assert!(buf[..20].contains(" 204 "));
                continue;
            }
        }
    });

    let _ = thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        cond.store(true, Ordering::Release);
    });

    server.wait_for_exit(None);
}
