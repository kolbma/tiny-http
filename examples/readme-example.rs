#![allow(missing_docs, unused_crate_dependencies)]

fn main() {
    use tiny_http::{Response, Server};

    let server = Server::http("0.0.0.0:9975").unwrap();
    let port = server.server_addr().port().unwrap();
    println!("Now listening on http://localhost:{port}/");

    for request in server.incoming_requests() {
        println!(
            "received request! method: {:?}, url: {:?}, headers: {:?}",
            request.method(),
            request.url(),
            request.headers()
        );

        let response = Response::from_string("hello world");
        let _ = request.respond(response).expect("Responded");
    }
}
