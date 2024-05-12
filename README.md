# tiny-http

[![Crate][crate_img]][crate]
[![Documentation][docs_img]][docs]
![License][license_img]
[![CI Status][ci_badge]][ci_link]

[**Documentation**](https://docs.rs/tiny_http)

Tiny but strong HTTP server in Rust.
Its main objectives are to be 100% compliant with the HTTP standard and to provide an easy way to create an HTTP server.

What does **tiny-http** handle?
 - Accepting and managing connections to the clients
 - Parsing requests
 - Requests pipelining
 - HTTPS (using either OpenSSL, Rustls or native-tls)
 - Transfer-Encoding and Content-Encoding
 - Range Requests [RFC 9110](https://datatracker.ietf.org/doc/html/rfc9110#name-range-requests) (only single range; **multipart/byteranges** request produces 416 (Range Not Satisfiable))
 - `Connection: upgrade` (used by websockets)
 - Turning user input (eg. POST input) into a contiguous UTF-8 string (**not implemented yet**)

Tiny-http handles everything that is related to client connections and data transfers and encoding.

Everything else (parsing the values of the headers, multipart data, routing, etags, cache-control, HTML templates, etc.) must be handled by your code.
If you want to create a website in Rust, I strongly recommend using a framework instead of this library.

### Installation

Add this to the `Cargo.toml` file of your project:

```toml
[dependencies]
tiny_http = "0.12"
```

#### Minimum Supported Rust Version

At least version __1.61__.  
But feature __ssl__, __ssl-native-tls__, __ssl-rustls__ needs __1.63__ and feature __socket2__ needs __1.63__.  
Feature __content-type__ requires __1.70__.


### Features

#### Default features

- log: uses log trait to debug and error
- http-0-9: supporting HTTP/0.9 simple requests
- range-support: supporting HTTP/1.1 Range Requests

#### Optional features

- content-type: provides usual content type enum with type converters
- http-0-9: supporting HTTP/0.9 simple requests
- log: uses log trait to debug and error
- range-support: supporting HTTP/1.1 Range Requests
- socket2: provides configurable TCP socket

Select single _ssl_ feature...  
- ssl: HTTPS with openssl support
- ssl-native-tls: HTTPS with native-tls support
- ssl-rustls: HTTPS with rustls support

### Usage

```rust
use tiny_http::{Server, Response};

let server = Server::http("0.0.0.0:8000").unwrap();

for request in server.incoming_requests() {
    println!("received request! method: {:?}, url: {:?}, headers: {:?}",
        request.method(),
        request.url(),
        request.headers()
    );

    let response = Response::from_string("hello world");
    request.respond(response);
}
```

#### Running Included Examples

1. Clone this repository locally
2. to run an example in the examples folder run:
```bash
cargo run --example [example_name]
```

example:
```bash
cargo run --example hello-world
```


### Speed

Tiny-http was designed with speed in mind:
 - Each client connection will be dispatched to a thread pool. Each thread will handle one client.
 If there is no thread available when a client connects, a new one is created. Threads that are idle
 for a long time (currently 5 seconds) will automatically die.
 - If multiple requests from the same client are being pipelined (ie. multiple requests
 are sent without waiting for the answer), tiny-http will read them all at once and they will
 all be available via `server.recv()`. Tiny-http will automatically rearrange the responses
 so that they are sent in the right order.
 - One exception to the previous statement exists when a request has a large body (currently > 1kB),
 in which case the request handler will read the body directly from the stream and tiny-http
 will wait for it to be read before processing the next request. Tiny-http will never wait for
 a request to be answered to read the next one.
 - When a client connection has sent its last request (by sending `Connection: close` header),
 the thread will immediately stop reading from this client and can be reclaimed, even when the
 request has not yet been answered. The reading part of the socket will also be immediately closed.
 - Decoding the client's request is done lazily. If you don't read the request's body, it will not
 be decoded.

### HTTP/0.9

HTTP/0.9 doesn't know headers, content-types and anything different to GET requests.  
The request handling accepts only a simple GET request and provides an empty
data reader, because HTTP/0.9 can't send any data to the server.  
Responses with HTTP/0.9 version doesn't include any header. Clients expect parsing an
HTML document with an 80 chars line limit.  
Errors are sent plain text, expecting clients can handle this.

To provide a correct HTTP/0.9 response this has to be handled in every [`Response`].  
Clients see e.g. a simple text _Permanent Redirect_, if your response would have
[`StatusCode`] 308.  

By disabling the support, every HTTP/0.9 request is responded with something similar to
the Status 505 and presenting the client just the reason text _HTTP Version Not Supported_.

### Example Implementations

Examples of tiny-http in use:

* [heroku-tiny-http-hello-world](https://github.com/frewsxcv/heroku-tiny-http-hello-world) - A simple web application demonstrating how to deploy tiny-http to Heroku
* [crate-deps](https://github.com/frewsxcv/crate-deps) - A web service that generates images of dependency graphs for crates hosted on crates.io
* [rouille](https://crates.io/crates/rouille) - Web framework built on tiny-http

### License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

#### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in tiny-http by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

<!-- Links and Badges -->
[crate_img]: https://img.shields.io/crates/v/tiny_http.svg?logo=rust "Crate Page"
[crate]: https://crates.io/crates/tiny_http "Crate Link"
[docs]: https://docs.rs/tiny_http "Documentation"
[docs_img]: https://docs.rs/tiny_http/badge.svg "Documentation"
[license_img]: https://img.shields.io/crates/l/tiny_http.svg "License"
[ci_badge]: https://github.com/tiny-http/tiny-http/actions/workflows/ci.yaml/badge.svg "CI Status"
[ci_link]: https://github.com/tiny-http/tiny-http/actions/workflows/ci.yaml "Workflow Link"
