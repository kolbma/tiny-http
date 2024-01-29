#![allow(missing_docs, unused_crate_dependencies)]

use ascii::AsciiString;
use std::fs;
use std::path::{Path, PathBuf};
use tiny_http::{Header, Response, Server};

fn get_content_type(path: &Path) -> &'static str {
    let extension = match path.extension() {
        None => return "txt",
        Some(e) => e.to_str().unwrap_or("txt"),
    };

    match extension {
        "gif" => "image/gif",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "pdf" => "application/pdf",
        "htm" | "html" => "text/html; charset=utf8",
        _ => "text/plain; charset=utf8",
    }
}

fn main() {
    let base_path: PathBuf = std::env::current_dir()
        .expect("current dir invalid")
        .canonicalize()
        .unwrap();
    println!(
        "Serving now files under working directory {}",
        base_path.to_str().unwrap()
    );

    let server = Server::http("0.0.0.0:9975").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    println!("Now listening on http://localhost:{port}/");

    loop {
        match server.recv() {
            Ok(rq) => {
                println!("{rq:?}");

                let url = rq.url().to_string();
                let url = url.trim_start_matches('/');
                let base_path = base_path.clone();
                let path = base_path.join(Path::new(&url));

                if path.exists() {
                    let mut path_ok = false;

                    if let Ok(path) = path.canonicalize() {
                        let path_str = path.to_str();
                        if path_str.is_some() && path.starts_with(base_path) {
                            println!("requesting file: {}", path_str.unwrap());
                            path_ok = true;
                        }
                    }

                    if !path_ok {
                        println!("forbidden: {url}");
                        if let Err(err) = rq.respond(Response::from(403)) {
                            eprintln!("{err:#?}");
                        }
                        continue;
                    }
                } else {
                    println!("not found: {}", path.to_str().unwrap_or("?"));
                    if let Err(err) = rq.respond(Response::from(404)) {
                        eprintln!("{err:#?}");
                    }
                    continue;
                }

                let file = fs::File::open(&path);

                if let Ok(file) = file {
                    let response = Response::from_file(file);

                    let response = response
                        .with_header(Header {
                            field: "Content-Type".parse().unwrap(),
                            value: AsciiString::from_ascii(get_content_type(&path)).unwrap(),
                        })
                        .unwrap();

                    if let Err(err) = rq.respond(response) {
                        eprintln!("{err:#?}");
                    }
                } else if let Err(err) = rq.respond(Response::from(500)) {
                    eprintln!("{err:#?}");
                }
            }
            Err(err) => {
                eprintln!("{err:#?}");
            }
        }
    }
}
