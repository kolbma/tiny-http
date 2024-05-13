#![allow(missing_docs, unused_crate_dependencies)]

use std::convert::TryFrom;
use std::io::{Cursor, Read, Write};
use std::rc::Rc;
use std::thread;

use base64ct::{Base64, Encoding};

use tiny_http::{Header, Method, Response};

const SEC_WEBSOCKET_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const WS_OPC_CLOSE: u8 = 0x8;
const WS_OPC_DATA_TEXT: u8 = 0x1;
const WS_OPC_FRAGMENT: u8 = 0x0;
const WS_OPC_PING: u8 = 0x9;
const WS_OPC_PONG: u8 = 0xA;

#[derive(Debug)]
enum State {
    Close,
    Data(Option<[u8; 4]>, u64, Option<Vec<u8>>),
    DataAvail(Vec<u8>),
    Error(Rc<State>, std::io::Error),
    Fragment(Vec<u8>, bool),
    Ignore,
    Ping,
    Pong,
    Unsupported(u8),
    Wait,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Error(_, l0), Self::Error(_, r0)) => l0.kind() == r0.kind(),
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

// macro_rules! trace {
//     ($e:expr) => {
//         eprint!("0x{:X} ", $e);
//     };
//     ($e:expr, $e2:expr) => {
//         eprint!("0x{:X} {} ", $e, $e2);
//     };
// }

macro_rules! info {
    ($e:expr) => {
        // println!($e);
    };
    ($e:expr, $e2:expr) => {
        // println!($e, $e2);
    };
}

macro_rules! error {
    ($e:expr) => {
        eprintln!($e);
    };
    ($e:expr, $e2:expr) => {
        eprintln!($e, $e2);
    };
}

macro_rules! status {
    ($e:expr) => {
        println!($e);
    };
    ($e:expr, $e2:expr) => {
        println!($e, $e2);
    };
}

fn home_page(port: u16) -> Response<Cursor<Vec<u8>>> {
    Response::from_string(format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
        <title>examples/websockets - cargo run --example websockets</title>
        </head>
        <body>
        <script type="text/javascript">
        var timer = 0;
        var socket = new WebSocket('ws://localhost:{port}/', 'ping');

        function log(id, msg) {{
            var now = new Date();
            var ts = '';
            if (now.getHours() < 10) {{
                ts += '0';
            }}
            ts += now.getHours() + ':';
            if (now.getMinutes() < 10) {{
                ts += '0';
            }}
            ts += now.getMinutes() + ':';
            if (now.getSeconds() < 10) {{
                ts += '0';
            }}
            ts += now.getSeconds();
            document.getElementById('result').innerHTML += ts + '&nbsp;&nbsp; - ' + ' [buffer: ' +  socket.bufferedAmount 
                + '] -  ' + id + ': ' + msg + '<br>';
        }}

        function send(data) {{
            if (data !== '<PING>') {{
                log('Me', data);
            }}
            socket.send(data);
        }}

        function keepAlive(timeout = 8000) {{
            if (socket.readyState == WebSocket.OPEN) {{
                send('<PING>');
            }}
            timer = setTimeout(keepAlive, timeout);
        }}

        socket.addEventListener('close', (event) => {{
            if (timer) {{
                clearTimeout(timer);
            }}
            document.getElementById('button').setAttribute("disabled", "disabled");
            document.getElementById('status').innerHTML = 'disconnected';
        }});

        socket.addEventListener('error', (event) => {{
            console.error("websocket error: ", event);
        }});

        socket.addEventListener('message', (event) => {{
            log('Server', event.data);
        }});

        socket.addEventListener('open', (event) => {{
            document.getElementById('status').innerHTML = 'connected';
            document.getElementById('button').removeAttribute("disabled");
            timer = setTimeout(keepAlive, 3000);
        }});

        document.addEventListener("DOMContentLoaded", function() {{
            document.getElementById('msg').addEventListener('keypress', function(event) {{
                if (event.key === 'Enter') {{
                event.preventDefault();
                document.getElementById('button').click();
                }}
            }});

            document.getElementById('button').addEventListener('click', function() {{
                send(document.getElementById('msg').value);
                document.getElementById('msg').value='';
            }});
        }});
        </script>
        <p>You can try to send a message and <b>tiny-http</b> example WebSocket server 
           will be echoing the text back to your browser.</p>
        <p>If your browser is connected to the WebSocket server, the status should be <b>connected</b></p>
        <p>Status: <span id="status">connecting</span></p>
        <p><input type="text" id="msg" maxlength="50">
        <button id="button" disabled="disabled">Send</button></p>
        <p>Received:</p>
        <p id="result"></p>
        </body>
        </html>
    "#
    ))
    .with_header("Content-type: text/html".parse::<Header>().unwrap())
    .unwrap()
}

/// Turns a Sec-WebSocket-Key into a Sec-WebSocket-Accept
fn convert_key(input: &[u8]) -> String {
    use sha1_smol::Sha1;

    let mut input = input.to_vec();
    input.extend(SEC_WEBSOCKET_GUID);

    let mut sha1 = Sha1::new();
    sha1.update(&input);

    Base64::encode_string(&sha1.digest().bytes())
}

/// Build a frame not fragmented frame for parameters
/// Also without masking only supported by server side
fn frame(frame: &mut Vec<u8>, opcode: u8, data: &[u8]) {
    frame.extend_from_slice(&[0x80 + opcode, u8::try_from(data.len()).unwrap_or(0x7D)]);
    frame.extend_from_slice(data);
}

/// Write a frame for `opcode` with `data`
///
/// `data` can be `b""`
fn frame_write<W>(w: &mut W, opcode: u8, data: &[u8])
where
    W: Write,
{
    let mut f = Vec::new();
    frame(&mut f, opcode, data);
    let _ = w.write_all(&f);
    let _ = w.flush();
    info!(
        "frame_write opcode: 0x{opcode:X} data: {}",
        std::str::from_utf8(data).unwrap()
    );
}

/// Checks `opcode` against `const` defined `ws_op_code`
macro_rules! is_opcode {
    ($opcode:expr, $ws_op_code:expr) => {
        $opcode & $ws_op_code == $ws_op_code
    };
}

/// Transform _octets_ like in specification based on `mask_key`
#[inline]
fn transform_octets(mask_key: [u8; 4], payload_data: &[u8]) -> Vec<u8> {
    let mut transformed = Vec::with_capacity(payload_data.len());
    for (i, &byte) in payload_data.iter().enumerate() {
        let j = i % 4;
        transformed.push(byte ^ mask_key[j]);
    }

    transformed
}

macro_rules! is_some_and_eq {
    ($opt:expr, $expr:expr) => {
        $opt.map(|h| h.value == $expr).unwrap_or_default()
    };
}

/// Proof-of-concept implementation for a `WebSocket` server sending Pings
/// receiving Pongs and responding with received text messages
#[allow(clippy::too_many_lines)]
fn main() {
    let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    let port = server.server_addr().port().unwrap();
    println!("Now listening on http://localhost:{port}/");

    for request in server.incoming_requests() {
        // we are handling this websocket connection in a new task
        let _ = thread::spawn(move || {
            // checking the "Upgrade" header to check that it is a websocket
            let upgrade_requested =
                is_some_and_eq!(request.header_first(b"Upgrade"), b"websocket".as_ref());

            // provide html + websocket javascript to browser
            if !upgrade_requested {
                let _ = match (request.method(), request.url()) {
                    (Method::Get, "/") => request.respond(home_page(port)),
                    (Method::Get, _) => request.respond(Response::from(404)),
                    _ => request.respond(Response::from(405)),
                }
                .unwrap();
                return;
            }

            // browser upgrades to WS
            // getting the value of Sec-WebSocket-Version
            let is_version_compatible = is_some_and_eq!(
                request.header_first(b"Sec-WebSocket-Version"),
                b"13".as_ref()
            );
            if !is_version_compatible {
                let _ = request
                    .respond(
                        Response::from(426)
                            .with_header(
                                Header::from_bytes(b"Sec-WebSocket-Version", b"13").unwrap(),
                            )
                            .unwrap(),
                    )
                    .unwrap();
                return;
            }

            // getting the value of Sec-WebSocket-Key and convert for Sec-WebSocket-Accept
            let sec_websocket_accept = request
                .header_first(b"Sec-WebSocket-Key")
                .map(|h| convert_key(h.value.as_bytes()));

            // building the "101 Switching Protocols" response
            let mut response = Response::empty(101)
                .with_header("Sec-WebSocket-Protocol: ping".parse::<Header>().unwrap())
                .unwrap();

            if let Some(sec_websocket_accept) = sec_websocket_accept {
                response
                    .add_header(
                        Header::from_bytes(b"Sec-WebSocket-Accept", &sec_websocket_accept).unwrap(),
                    )
                    .unwrap();
            }

            let remote_address = request.remote_addr_string();

            let stream = &mut request.upgrade("websocket", response);
            // stream.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
            info!("timeout: {:?}", stream.read_timeout());

            let mut state = State::Wait;
            let mut data_state = State::Wait;
            let mut frg_state = State::Ignore;
            let mut frg_opcode = 0u8;

            frame_write(
                stream,
                WS_OPC_DATA_TEXT,
                format!("Welcome, you are connected: {remote_address}").as_bytes(),
            );

            loop {
                if state == State::Wait {
                    let mut byte1 = [0u8];

                    match stream.read_exact(&mut byte1) {
                        Ok(()) => {
                            let fragment = byte1[0] & 0xF0;
                            let mut opcode = byte1[0] & 0xF;
                            status!("fragment: 0x{fragment:X} opcode: 0x{opcode:X}");
                            if fragment & 0x80 == 0x0 {
                                if opcode == WS_OPC_FRAGMENT {
                                    opcode = frg_opcode;
                                    info!("fragmentation continue");
                                } else {
                                    frg_state = State::Fragment(Vec::new(), false);
                                    frg_opcode = opcode;
                                    info!("fragmentation start");
                                }
                            } else if opcode == WS_OPC_FRAGMENT {
                                if let State::Fragment(data, _) = frg_state {
                                    data_state = State::Wait;
                                    frg_state = State::Fragment(data, true);
                                    opcode = frg_opcode;
                                    info!("fragmentation end");
                                } else {
                                    state = State::Unsupported(byte1[0]);
                                }
                            }

                            if opcode != WS_OPC_FRAGMENT {
                                if is_opcode!(opcode, WS_OPC_DATA_TEXT) {
                                    info!("got text");
                                    data_state = State::Wait;
                                } else if is_opcode!(opcode, WS_OPC_PING) {
                                    state = State::Ping;
                                    info!("got a ping");
                                } else if is_opcode!(opcode, WS_OPC_PONG) {
                                    state = State::Pong;
                                    info!("got a pong");
                                } else if is_opcode!(opcode, WS_OPC_CLOSE) {
                                    state = State::Close;
                                    info!("got a close");
                                } else {
                                    state = State::Unsupported(opcode);
                                }
                            }
                        }
                        Err(err) => {
                            state = State::Error(Rc::new(state), err);
                            data_state = State::Ignore;
                        }
                    };
                }

                if state == State::Ping {
                    frame_write(stream, WS_OPC_PONG, b"");
                    let data = b"&lt;&lt;PONG&gt;&gt;";
                    frame_write(stream, WS_OPC_DATA_TEXT, data);
                    info!("send pong");
                    state = State::Wait;
                    data_state = State::Ignore;
                } else if state == State::Pong {
                    // ignore pongs - stream alive
                    data_state = State::Wait;
                } else if let State::Unsupported(opcode) = state {
                    // UNFINISHED: only ping/pong/text supported
                    frame_write(
                        stream,
                        WS_OPC_DATA_TEXT,
                        format!("unsupported opcode: 0x{opcode:X}").as_bytes(),
                    );
                    error!("unsupported opcode: 0x{opcode:X}");
                    data_state = State::Wait;
                } else if state == State::Close {
                    break;
                }

                if data_state == State::Wait {
                    let mut mask_payload = [0u8];
                    match stream.read_exact(&mut mask_payload) {
                        Ok(()) => {
                            let payload = mask_payload[0] & 0x7F;
                            // UNFINISHED: bigger payload not supported
                            assert!(payload < 0x7E, "payload: {}", payload);
                            let data_len = u64::from(payload);
                            let mask_key = if mask_payload[0] & 0x80 == 0x0 {
                                None
                            } else {
                                Some([0u8; 4])
                            };
                            // UNFINISHED: bigger payload not supported
                            data_state = State::Data(mask_key, data_len, None);
                        }
                        Err(err) => data_state = State::Error(Rc::new(data_state), err),
                    }
                }

                if let State::Data(Some(_mask_key) /* masked */, data_len, None) = &data_state {
                    let mut mask_key = [0u8; 4];
                    match stream.read_exact(&mut mask_key) {
                        Ok(()) => data_state = State::Data(Some(mask_key), *data_len, None),
                        Err(err) => data_state = State::Error(Rc::new(data_state), err),
                    };
                }

                if let State::Data(_mask_key, data_len, data) = &mut data_state {
                    if *data_len > 0 {
                        // ATTENTION: memory usage not for productive use!
                        *data = Some(vec![0u8; usize::try_from(*data_len).unwrap_or(usize::MAX)]);
                        let payload_data = data.as_mut().unwrap();
                        if let Err(err) = stream.read_exact(payload_data) {
                            data_state = State::Error(Rc::new(data_state), err);
                        }
                    }
                }

                if let State::Data(Some(mask_key) /* masked */, _data_len, payload_data) =
                    &data_state
                {
                    if let Some(payload_data) = &payload_data {
                        let transformed_data = transform_octets(*mask_key, payload_data);
                        if *payload_data != transform_octets(*mask_key, &transformed_data) {
                            error!("transform_octets problem");
                        }
                        data_state = State::DataAvail(transformed_data);
                    }
                } else if let State::Data(None /* not masked */, _data_len, payload_data) =
                    &mut data_state
                {
                    if let Some(payload_data) = payload_data.take() {
                        data_state = State::DataAvail(payload_data);
                    }
                }

                if let State::Fragment(ref mut data, is_end) = frg_state {
                    if let State::DataAvail(payload_data) = &data_state {
                        data.extend(payload_data);
                    }
                    if is_end {
                        let data = if let Ok(text) = std::str::from_utf8(data) {
                            if text == "<PING>" {
                                frame_write(stream, WS_OPC_PING, b"");
                                let data = b"&lt;&lt;PING&gt;&gt;";
                                frame_write(stream, WS_OPC_DATA_TEXT, data);
                                info!("send ping");
                                state = State::Wait;
                                data_state = State::Wait;
                                continue;
                            }

                            text.as_bytes()
                                .iter()
                                .filter(|&c| {
                                    (b'a'..=b'~').contains(c)
                                        || (b'?'..=b'[').contains(c)
                                        || (b' '..=b'%').contains(c)
                                        || (b'\''..=b';').contains(c)
                                        || [b'=', b']', b'_'].contains(c)
                                })
                                .copied()
                                .collect::<Vec<_>>()
                        } else {
                            b"&lt;NON-UTF-8-DATA&gt;".to_vec()
                        };

                        frame_write(stream, WS_OPC_DATA_TEXT, &data);

                        frg_state = State::Ignore;
                    }
                } else if let State::DataAvail(payload_data) = &data_state {
                    if payload_data.is_empty() {
                        if state != State::Pong {
                            frame_write(stream, WS_OPC_DATA_TEXT, b"");
                        }
                    } else {
                        // some simple sample code for echo secure data to client
                        let data = if let Ok(text) = std::str::from_utf8(payload_data) {
                            if text == "<PING>" {
                                frame_write(stream, WS_OPC_PING, b"");
                                let data = b"&lt;&lt;PING&gt;&gt;";
                                frame_write(stream, WS_OPC_DATA_TEXT, data);
                                info!("send ping");
                                state = State::Wait;
                                data_state = State::Wait;
                                continue;
                            }

                            text.as_bytes()
                                .iter()
                                .filter(|&c| {
                                    (b'a'..=b'~').contains(c)
                                        || (b'?'..=b'[').contains(c)
                                        || (b' '..=b'%').contains(c)
                                        || (b'\''..=b';').contains(c)
                                        || [b'=', b']', b'_'].contains(c)
                                })
                                .copied()
                                .collect::<Vec<_>>()
                        } else {
                            b"&lt;NON-UTF-8-DATA&gt;".to_vec()
                        };

                        frame_write(stream, WS_OPC_DATA_TEXT, &data);
                    }
                }

                match (&state, &data_state) {
                    (State::Error(err_state, err), State::Error(err_data_state, data_err)) => {
                        error!("state error: {err:?} [occur in: {err_state:?}]");
                        error!("data_state error: {data_err:?} [occur in: {err_data_state:?}]");
                        break;
                    }
                    (State::Error(err_state, err), _) => {
                        error!("state error: {err:?} [occur in: {err_state:?}]");
                        break;
                    }
                    (_, State::Error(err_data_state, data_err)) => {
                        error!("data_state error: {data_err:?} [occur in: {err_data_state:?}]");
                        break;
                    }
                    _ => {}
                }

                state = State::Wait;
                data_state = State::Wait;

                thread::sleep(std::time::Duration::from_millis(500));
            }

            frame_write(stream, WS_OPC_CLOSE, b"");
            info!("send close");
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::{WS_OPC_DATA_TEXT, WS_OPC_PING};

    #[test]
    fn frame_test() {
        let mut frame = Vec::new();
        super::frame(&mut frame, WS_OPC_DATA_TEXT, b"Hello");
        assert_eq!(&frame[..], &[0x81u8, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]);

        frame.clear();
        super::frame(&mut frame, WS_OPC_PING, b"Hello");
        assert_eq!(&frame[..], &[0x89u8, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]);
    }

    #[test]
    fn handshake_key_test() {
        let key = b"dGhlIHNhbXBsZSBub25jZQ==";
        let b64 = super::convert_key(key);

        assert_eq!(&b64, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn transform_octets_test() {
        let frame = &[
            0x81u8, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
        ];
        let mask_key = [frame[4], frame[5], frame[6], frame[7]];
        let data = &frame[8..];
        let _ = super::transform_octets(mask_key, data);
    }
}
