#![cfg(unix)]
#![allow(unused_crate_dependencies)]

use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

#[allow(dead_code)]
mod support;

#[test]
fn unix_basic_handling() {
    let server = tiny_http::Server::http_unix(Path::new("/tmp/tiny-http-test.sock")).unwrap();
    let path: PathBuf = server
        .server_addr()
        .unix_socket_addrs()
        .unwrap()
        .as_pathname()
        .unwrap()
        .into();
    let mut client = UnixStream::connect(path).unwrap();

    write!(
        client,
        "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();

    let request = server.recv().unwrap();
    assert!(*request.method() == tiny_http::Method::Get);
    //assert!(request.url() == "/");
    let _ = request
        .respond(tiny_http::Response::from_string("hello world".to_owned()))
        .unwrap();

    let _ = server.try_recv().unwrap();

    let mut content = String::new();
    let _ = client.read_to_string(&mut content).unwrap();
    assert!(content.ends_with("hello world"));
}
