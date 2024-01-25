/*!

A web server that redirects every request to a PHP script.

Usage: php-cgi <php-script-path>

*/
#![allow(unused_crate_dependencies)]

use ascii::AsAsciiStr;

fn handle(rq: tiny_http::Request, script: &str) {
    use std::io::Write;
    use std::process::Command;

    let php = Command::new("php-cgi")
        .arg(script)
        //.stdin(Ignored)
        //.extra_io(Ignored)
        .env("AUTH_TYPE", "")
        .env(
            "CONTENT_LENGTH",
            rq.content_length().unwrap_or_default().to_string(),
        )
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
        .output()
        .unwrap();

    // note: this is not a good implementation
    // cgi returns the status code in the headers ; also many headers will be missing
    //  in the response
    match php.status {
        status if status.success() => {
            let mut writer = rq.into_writer();
            let writer: &mut dyn Write = &mut *writer;

            (write!(writer, "HTTP/1.1 200 OK\r\n")).unwrap();
            (write!(writer, "{}", php.stdout.clone().as_ascii_str().unwrap())).unwrap();

            writer.flush().unwrap();
        }
        _ => {
            println!(
                "Error in script execution: {}",
                php.stderr.as_ascii_str().unwrap()
            );
        }
    }
}

fn main() {
    use std::env;
    use std::sync::Arc;
    use std::thread::spawn;

    let php_script = {
        let mut args = env::args();
        if args.len() < 2 {
            println!("Usage: php-cgi <php-script-path>");
            return;
        }
        args.nth(1).unwrap()
    };

    let server = Arc::new(tiny_http::Server::http("0.0.0.0:9975").unwrap());
    println!("Now listening on port 9975");

    let num_cpus = 4; // TODO: dynamically generate this value
    for _ in 0..num_cpus {
        let server = server.clone();
        let php_script = php_script.clone();

        let _ = spawn(move || {
            for rq in server.incoming_requests() {
                handle(rq, &php_script);
            }
        });
    }
}
