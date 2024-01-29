#![allow(missing_docs, unused_crate_dependencies)]

#[cfg(not(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
)))]
fn main() {
    println!("This example requires one of the supported `ssl-*` features to be enabled");
}

#[cfg(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
))]
fn main() {
    use tiny_http::{Response, Server};

    let server = Server::https(
        "0.0.0.0:9975",
        tiny_http::SslConfig::new(
            include_bytes!("ssl-cert.pem").to_vec(),
            include_bytes!("ssl-key.pem").to_vec(),
        ),
    )
    .unwrap();

    println!(
        "Note: connecting to this server will likely give you a warning from your browser \
              because the connection is unsecure. This is because the certificate used by this \
              example is self-signed. With a real certificate, you wouldn't get this warning."
    );

    let port = server.server_addr().to_ip().unwrap().port();
    println!("Now listening on https://localhost:{port}/");

    for request in server.incoming_requests() {
        assert!(request.secure());

        println!(
            "received request! method: {:?}, url: {:?}, headers: {:?}",
            request.method(),
            request.url(),
            request.headers()
        );

        let response = Response::from_string("hello world");
        if let Err(err) = request.respond(response) {
            eprintln!("Failed to respond to request: {err:?}");
        }
    }
}
