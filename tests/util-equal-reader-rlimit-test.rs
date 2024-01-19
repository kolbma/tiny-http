#![cfg(unix)]
#![allow(unused_crate_dependencies)]

use std::io::Write;

#[allow(dead_code)]
mod support;

#[test]
fn test_equal_reader_drop_rlimit() {
    // this limits need to be fiddled out, because test runs need a lot more memory than the productive server
    rlimit::Resource::AS
        .set(100_000_000, 100_000_000)
        .expect("ulimit -v 100_000_000 failed");

    let (server, client) = support::new_one_server_one_client();

    {
        let mut client = client;
        (write!(client, "GET / HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain; charset=utf8\r\nContent-Length: 204857600\r\n\r\nhello")).unwrap();
    }

    let mut request = server.recv().unwrap();

    let mut output = String::new();
    let _ = request.as_reader().read_to_string(&mut output).unwrap();
    assert_eq!(output, "hello");
}
