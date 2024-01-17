use ascii::AsciiString;
use std::fs;
use std::path::Path;

extern crate ascii;
extern crate tiny_http;

fn get_content_type(path: &Path) -> &'static str {
    let extension = match path.extension() {
        None => return "text/plain",
        Some(e) => e,
    };

    match extension.to_str().unwrap() {
        "gif" => "image/gif",
        "jpg" => "image/jpeg",
        "jpeg" => "image/jpeg",
        "png" => "image/png",
        "pdf" => "application/pdf",
        "htm" => "text/html; charset=utf8",
        "html" => "text/html; charset=utf8",
        "txt" => "text/plain; charset=utf8",
        _ => "text/plain; charset=utf8",
    }
}

fn main() {
    let server = tiny_http::Server::http("0.0.0.0:8000").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    println!("Now listening on port {}", port);

    while let Ok(rq) = server.recv() {
        println!("{:?}", rq);

        let url = rq.url().to_string();
        let path = Path::new(&url);
        let file = fs::File::open(path);

        if let Ok(file) = file {
            let response = tiny_http::Response::from_file(file);

            let response = response
                .with_header(tiny_http::Header {
                    field: "Content-Type".parse().unwrap(),
                    value: AsciiString::from_ascii(get_content_type(path)).unwrap(),
                })
                .unwrap();

            if let Err(err) = rq.respond(response) {
                eprintln!("{err:#?}");
            }
        } else {
            let status = tiny_http::StatusCode(404);
            if let Err(err) = rq.respond(
                tiny_http::Response::from_string(status.default_reason_phrase())
                    .with_status_code(status),
            ) {
                eprintln!("{err:#?}");
            }
        }
    }
}
