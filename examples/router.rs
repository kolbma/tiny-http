use log::error;
use std::{
    collections::HashMap,
    io::{Cursor, Read},
};
use tiny_http::{Request, Response, Server, StatusCode};

type RouteHandler = fn(&mut Request) -> Response<Cursor<Vec<u8>>>;

fn get_root(_req: &mut Request) -> Response<Cursor<Vec<u8>>> {
    Response::from_string("You've reached the root!")
}

fn get_hello(_req: &mut Request) -> Response<Cursor<Vec<u8>>> {
    Response::from_string("Hello from /hello!")
}

fn post_echo(req: &mut Request) -> Response<Cursor<Vec<u8>>> {
    let mut request_body_bytes = Vec::new();
    if req
        .as_reader()
        .take(10_485_760)
        .read_to_end(&mut request_body_bytes)
        .is_ok()
    {
        if let Ok(request_body_string) = String::from_utf8(request_body_bytes) {
            Response::from_data(request_body_string.as_bytes())
        } else {
            let status = StatusCode(400);
            Response::from_string(status.default_reason_phrase()).with_status_code(status)
        }
    } else {
        let status = StatusCode(503);
        Response::from_string(status.default_reason_phrase()).with_status_code(status)
    }
}

fn main() {
    let routes = HashMap::from([
        ("GET:/", get_root as RouteHandler),
        ("GET:/hello", get_hello as RouteHandler),
        ("POST:/echo", post_echo as RouteHandler),
    ]);
    let server = Server::http("0.0.0.0:3000").unwrap();
    for mut request in server.incoming_requests() {
        let route_key = format!("{}:{}", request.method(), request.url());
        let response_result = match routes.get(route_key.as_str()) {
            Some(handler) => {
                let response = handler(&mut request);
                request.respond(response)
            }
            None => {
                let status = StatusCode(404);
                let response =
                    Response::from_string(status.default_reason_phrase()).with_status_code(status);
                request.respond(response)
            }
        };
        if let Err(err) = response_result {
            error!("{err}");
        }
    }
}
