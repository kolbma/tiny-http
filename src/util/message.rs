use std::io::Error as IoError;

use crate::Request;

pub(crate) enum Message {
    Error(IoError),
    NewRequest(Request),
}

impl From<IoError> for Message {
    fn from(err: IoError) -> Message {
        Message::Error(err)
    }
}

impl From<Request> for Message {
    fn from(rq: Request) -> Message {
        Message::NewRequest(rq)
    }
}
