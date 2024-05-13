use std::thread;
use std::time::Duration;
use std::{net::TcpStream, sync::Arc};

use tiny_http::SocketConfig;

/// Creates a [`TcpStream`] Client for first `addr`
#[cfg(feature = "socket2")]
pub(crate) fn create_client<A>(
    addr: A,
    timeout: Option<Duration>,
    keep_alive_idle: Option<Duration>,
) -> TcpStream
where
    A: std::net::ToSocketAddrs,
{
    let addr = addr.to_socket_addrs().unwrap().next().unwrap();
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::STREAM,
        None,
    )
    .unwrap();

    if timeout.is_some() {
        socket.set_read_timeout(timeout).unwrap();
        socket.set_write_timeout(timeout).unwrap();
    }
    if let Some(keep_alive_idle) = keep_alive_idle {
        socket
            .set_tcp_keepalive(&socket2::TcpKeepalive::new().with_time(keep_alive_idle))
            .unwrap();
    }
    socket.connect(&addr.into()).unwrap();
    socket.into()
}

/// Creates a [`TcpStream`] Client for first `addr`
///
/// `keep_alive_idle` is ignored here, because not supported
#[cfg(not(feature = "socket2"))]
pub(crate) fn create_client<A>(
    addr: A,
    timeout: Option<Duration>,
    _keep_alive_idle: Option<Duration>,
) -> TcpStream
where
    A: std::net::ToSocketAddrs,
{
    let addr = addr.to_socket_addrs().unwrap().next().unwrap();

    let stream = if let Some(timeout) = timeout {
        TcpStream::connect_timeout(&addr, timeout)
    } else {
        TcpStream::connect(addr)
    }
    .unwrap();

    stream.set_nodelay(true).unwrap();
    if timeout.is_some() {
        stream.set_read_timeout(timeout).unwrap();
        stream.set_write_timeout(timeout).unwrap();
    }

    stream
}

/// Creates a server and a client connected to the server.
pub(crate) fn new_one_server_one_client() -> (tiny_http::Server, TcpStream) {
    new_one_server_one_client_2(None, None)
}

/// Creates a server and a client connected to the server.
pub(crate) fn new_one_server_one_client_2(
    timeout_ms: Option<u64>,
    keep_alive_idle_sec: Option<u64>,
) -> (tiny_http::Server, TcpStream) {
    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let port = server.server_addr().port().unwrap();
    let client = create_client(
        ("127.0.0.1", port),
        timeout_ms.map(Duration::from_millis),
        keep_alive_idle_sec.map(Duration::from_secs),
    );
    (server, client)
}

/// Creates a "hello world" server with a client connected to the server.
///
/// The server will automatically close after 3 seconds.
pub(crate) fn new_client_to_hello_world_server() -> TcpStream {
    new_client_to_hello_world_server_2(None, None)
}

/// Creates a "hello world" server with a client connected to the server.
///
/// The server will automatically close after `timeout_ms` milliseconds or 3 seconds.
pub(crate) fn new_client_to_hello_world_server_2(
    timeout_ms: Option<u64>,
    keep_alive_idle_sec: Option<u64>,
) -> TcpStream {
    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let port = server.server_addr().port().unwrap();
    let client = create_client(
        ("127.0.0.1", port),
        timeout_ms.map(Duration::from_millis),
        keep_alive_idle_sec.map(Duration::from_secs),
    );

    let _ = thread::spawn(move || {
        let mut cycles = timeout_ms.unwrap_or(3000) / 20;

        loop {
            if let Ok(Some(rq)) = server.try_recv() {
                let response = tiny_http::Response::from_string("hello world".to_string());
                let _ = rq.respond(response).unwrap();
            }

            thread::sleep(Duration::from_millis(20));

            cycles -= 1;
            if cycles == 0 {
                break;
            }
        }
    });

    client
}

/// Creates an "echo" server with a client connected to the server.
///
/// Server responds with data sent before.
///
/// The server will automatically close after 3 seconds.
pub(crate) fn new_client_to_echo_server() -> TcpStream {
    new_client_to_echo_server_2(None, None)
}

/// Creates an "echo" server with a client connected to the server.
///
/// Server responds with data sent before.
///
/// The server will automatically close after `timeout_ms` milliseconds or 3 seconds.
pub(crate) fn new_client_to_echo_server_2(
    timeout_ms: Option<u64>,
    keep_alive_idle_sec: Option<u64>,
) -> TcpStream {
    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let port = server.server_addr().port().unwrap();
    let client = create_client(
        ("127.0.0.1", port),
        timeout_ms.map(Duration::from_millis),
        keep_alive_idle_sec.map(Duration::from_secs),
    );

    let _ = thread::spawn(move || {
        let mut cycles = timeout_ms.unwrap_or(3000) / 20;

        loop {
            if let Ok(Some(mut rq)) = server.try_recv() {
                if *rq.method() == tiny_http::Method::Post {
                    let mut response = tiny_http::Response::from_string("");

                    #[cfg(feature = "content-type")]
                    {
                        if let Some(content_type) = rq.content_type() {
                            let _ = response.add_header(tiny_http::Header::from(content_type));
                        }
                    }

                    let mut data = vec![0u8; 2048];
                    let mut offset = 0;
                    while offset < 2048 {
                        let size = rq.as_reader().read(&mut data[offset..]);
                        if size.is_err() {
                            break;
                        }
                        let size = size.unwrap();
                        if size == 0 {
                            break;
                        }
                        offset += size;
                    }

                    response = response
                        .with_data(std::io::Cursor::new(data[..offset].to_vec()), Some(offset));

                    let _ = rq.respond(response).unwrap();
                } else {
                    let _ = rq.respond(tiny_http::Response::empty(405)).unwrap();
                }
            }

            thread::sleep(Duration::from_millis(20));

            cycles -= 1;
            if cycles == 0 {
                break;
            }
        }
    });

    client
}

/// Creates a server and a client connected to the server with
pub(crate) fn new_server_client_with_cfg(
    socket_config: &SocketConfig,
) -> (tiny_http::Server, TcpStream) {
    use tiny_http::ServerConfig;

    let server = tiny_http::Server::new(&ServerConfig {
        addr: tiny_http::ConfigListenAddr::from_socket_addrs("0.0.0.0:0").unwrap(),
        socket_config: Arc::new(socket_config.clone()),
        ..ServerConfig::default()
    })
    .unwrap();
    let port = server.server_addr().port().unwrap();
    let client = create_client(
        ("127.0.0.1", port),
        Some(socket_config.write_timeout),
        #[cfg(feature = "socket2")]
        if socket_config.tcp_keep_alive {
            Some(socket_config.tcp_keepalive_time)
        } else {
            None
        },
        #[cfg(not(feature = "socket2"))]
        None,
    );
    (server, client)
}

/// Creates a "hello world" server with [`SocketConfig`] with a client connected to the server.
///
/// The server will automatically close after 3 seconds.
pub(crate) fn new_client_to_hello_world_server_with_cfg(socket_config: &SocketConfig) -> TcpStream {
    let (server, client) = new_server_client_with_cfg(socket_config);

    let _ = thread::spawn(move || {
        let mut cycles = 3 * 1000 / 20;

        loop {
            if let Ok(Some(rq)) = server.try_recv() {
                let response = tiny_http::Response::from_string("hello world".to_string());
                let _ = rq.respond(response).unwrap();
            }

            thread::sleep(Duration::from_millis(20));

            cycles -= 1;
            if cycles == 0 {
                break;
            }
        }
    });

    client
}
