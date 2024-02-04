/*!

A web server that redirects every request to a PHP script.

Usage: php-cgi <php-script-path>

*/
#![allow(unused_crate_dependencies)]

use std::env;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::sync::Arc;
use std::thread;

use ascii::{AsAsciiStr, AsciiStr};
use tiny_http::Response;

const PHP_CGI_DEFAULT: &str = "php-cgi";
#[cfg(unix)]
const PHP_CGI: &str = "examples/php-cgi.sh";
#[cfg(windows)]
const PHP_CGI: &str = "examples/php-cgi.cmd";

fn handle(rq: tiny_http::Request, script: &str) -> Result<(), IoError> {
    use std::io::Write;
    use std::process::Command;

    let mut err = None;

    let php = [PHP_CGI_DEFAULT, PHP_CGI].iter().find_map(|php| {
        let result = Command::new(php)
            .arg(script)
            //.stdin(Ignored)
            //.extra_io(Ignored)
            .env("AUTH_TYPE", "")
            .env("CONTENT_LENGTH", format!("{}", 198 + script.len()))
            .env("CONTENT_TYPE", "")
            .env("GATEWAY_INTERFACE", "CGI/1.1")
            .env("PATH_INFO", "")
            .env("PATH_TRANSLATED", "")
            .env("QUERY_STRING", rq.url())
            .env("REMOTE_ADDR", rq.remote_addr().unwrap().to_string())
            .env("REMOTE_HOST", "")
            .env("REMOTE_IDENT", "")
            .env("REMOTE_USER", "")
            .env("REQUEST_METHOD", rq.method().as_str())
            .env("SCRIPT_NAME", script)
            .env("SERVER_NAME", "tiny-http php-cgi example")
            .env("SERVER_PORT", rq.remote_addr().unwrap().to_string())
            .env("SERVER_PROTOCOL", "HTTP/1.1")
            .env("SERVER_SOFTWARE", "tiny-http php-cgi example")
            .output();

        match result {
            Ok(php) => Some(php),
            Err(inner_err) => {
                err = Some(inner_err);
                None
            }
        }
    });

    let php = match php {
        Some(php) => php,
        None => return Err(err.unwrap()),
    };

    // note: this is not a good implementation
    // the headers returned by cgi could be used
    // also many headers will be missing in the response
    match php.status {
        status if status.success() => {
            let mut writer = rq.into_writer();
            let writer: &mut dyn Write = &mut *writer;

            (write!(writer, "HTTP/1.1 200 OK\r\n")).unwrap();
            (write!(writer, "{}", php.stdout.clone().as_ascii_str().unwrap())).unwrap();

            writer.flush().unwrap();
            Ok(())
        }
        _ => Err(IoError::new(
            IoErrorKind::Other,
            "php: ".to_string()
                + php
                    .stderr
                    .as_ascii_str()
                    .map(AsciiStr::as_str)
                    .unwrap_or("unknown"),
        )),
    }
}

macro_rules! is_some_and_contains {
    ($opt:expr, $expr:expr) => {
        $opt.map(|h| h.value.as_str().contains($expr))
            .unwrap_or_default()
    };
}

fn main() {
    let php_script = Arc::new({
        let mut args = env::args();
        if args.len() < 2 {
            eprintln!("Usage: php-cgi <php-script-path>");
            return;
        }
        args.nth(1).unwrap()
    });

    let server = Arc::new(tiny_http::Server::http("0.0.0.0:9975").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    println!("Now listening on http://localhost:{port}/");

    let num_cpus = 4; // TODO: dynamically generate this value
    let mut jhs = Vec::with_capacity(num_cpus);

    for _ in 0..num_cpus {
        let server = server.clone();
        let php_script = Arc::clone(&php_script);

        jhs.push(thread::spawn(move || {
            for rq in server.incoming_requests() {
                if is_some_and_contains!(rq.header_first(b"Accept"), "text/html") {
                    if let Err(err) = handle(rq, &php_script) {
                        eprintln!("php error: {err:?}");
                    }
                } else {
                    let _ = rq.respond(Response::from(404));
                }
            }
        }));
    }

    for jh in jhs {
        let _ = jh.join();
    }
}
