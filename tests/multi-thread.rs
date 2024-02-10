#![allow(unused_crate_dependencies)]

use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct RqHandler(Arc<AtomicBool>);
impl tiny_http::RequestHandler for RqHandler {
    fn handle_requests(&self, listener: &tiny_http::ListenerThread) {
        let mut response = <&tiny_http::response::StandardResponse>::from(
            tiny_http::response::Standard::NoContent204,
        )
        .clone();

        #[allow(clippy::while_let_loop)]
        loop {
            match listener.recv() {
                Ok(rq) => {
                    let _ = rq.respond_ref(&mut response);
                }
                Err(_err) => {
                    // eprintln!("error: {err:?}");
                    break;
                }
            }
        }
    }
}

impl Drop for RqHandler {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn stop_server_test() {
    let check = Arc::new(AtomicBool::default());
    let rq_hdl_check = Arc::clone(&check);

    let mut server = tiny_http::MTServer::http("0.0.0.0:0")
        .unwrap()
        .add_request_handler(RqHandler(rq_hdl_check));

    let mut client_1 =
        TcpStream::connect(("127.0.0.1", server.server_addr().port().unwrap())).unwrap();

    client_1
        .write_all(b"GET / HTTP/1.0\r\nHost: locahost\r\n\r\n")
        .unwrap();

    let mut content = String::new();
    let _ = client_1.read_to_string(&mut content);
    assert!(content.contains(" 204 "));

    let force_exit = Arc::new(AtomicBool::default());
    let switch_force_exit = Arc::clone(&force_exit);

    let now = Instant::now();

    let th = thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        switch_force_exit.store(true, Ordering::Release);
        thread::sleep(Duration::from_millis(1001));
        assert!(!switch_force_exit.load(Ordering::Acquire));
    });

    server.wait_for_exit(Some(&force_exit));

    let elaps = now.elapsed();

    assert!(
        elaps < Duration::from_millis(2000),
        "elaps: {}",
        elaps.as_millis()
    );

    thread::sleep(Duration::from_millis(200));

    assert!(check.load(Ordering::Acquire));
    assert!(th.join().is_ok());
}

#[test]
fn unblock_terminate_threads_test() {
    let check = Arc::new(AtomicBool::default());
    let rq_hdl_check = Arc::clone(&check);

    let server = tiny_http::MTServer::http("0.0.0.0:0")
        .unwrap()
        .add_request_handler(RqHandler(rq_hdl_check));

    let mut client_1 =
        TcpStream::connect(("127.0.0.1", server.server_addr().port().unwrap())).unwrap();

    let mut client_2 =
        TcpStream::connect(("127.0.0.1", server.server_addr().port().unwrap())).unwrap();

    client_1
        .write_all(b"GET / HTTP/1.0\r\nHost: locahost\r\n\r\n")
        .unwrap();
    client_2
        .write_all(b"GET / HTTP/1.0\r\nHost: locahost\r\n\r\n")
        .unwrap();

    let mut content = String::new();
    let _ = client_1.read_to_string(&mut content);
    assert!(content.contains(" 204 "));

    content.clear();

    let _ = client_2.read_to_string(&mut content);
    assert!(content.contains(" 204 "));

    let mut client_2 =
        TcpStream::connect(("127.0.0.1", server.server_addr().port().unwrap())).unwrap();
    client_2
        .write_all(b"GET / HTTP/1.0\r\nHost: locahost\r\n\r\n")
        .unwrap();

    content.clear();

    let _ = client_2.read_to_string(&mut content);
    assert!(content.contains(" 204 "));

    drop(server);

    thread::sleep(Duration::from_millis(200));

    assert!(check.load(Ordering::Acquire));
}
