//! Reusable standard [`Response`](super::Response)

use std::{convert::TryFrom, io::Cursor};

use lazy_static::lazy_static;

use crate::{ConnectionValue, Header, Response, StatusCode};

/// `StandardResponse` is type for `static` standard [`Response`]
pub type StandardResponse = Response<Cursor<&'static [u8]>>;

lazy_static! {
    static ref CACHE_100: StandardResponse = Response::from(100);
    static ref CACHE_200: StandardResponse = Response::from(200);
    static ref CACHE_201: StandardResponse = Response::from(201);
    static ref CACHE_204: StandardResponse = Response::from(204);
    static ref CACHE_400: StandardResponse = Response::from(400);
    static ref CACHE_404: StandardResponse = Response::from(404);
    static ref CACHE_405: StandardResponse = Response::from(405);
    static ref CACHE_408: StandardResponse = Response::from(408);
    static ref CACHE_413: StandardResponse = Response::from(413);
    static ref CACHE_414: StandardResponse = Response::from(414);
    static ref CACHE_417: StandardResponse = Response::from(417);
    static ref CACHE_431: StandardResponse = Response::from(431);
    static ref CACHE_500: StandardResponse = Response::from(500);
    static ref CACHE_501: StandardResponse = Response::from(501);
    static ref CACHE_505: StandardResponse = Response::from(505);
}

/// Most used standard Http [`Response`](super::Response)
///
/// Ordered by [`StatusCode`](crate::StatusCode)
#[derive(Clone, Copy, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
#[allow(missing_docs)]
pub enum Standard {
    Continue100,
    Ok200,
    Created201,
    NoContent204,
    BadRequest400,
    NotFound404,
    MethodNotAllowed405,
    RequestTimeout408,
    PayloadTooLarge413,
    UriTooLong414,
    ExpectationFailed417,
    RequestHeaderFieldsTooLarge431,
    InternalServerError500,
    NotImplemented501,
    HttpVersionNotSupported505,
}

impl Standard {
    /// Get default headers for [`StatusCode`]
    #[must_use]
    pub fn headers(status_code: StatusCode) -> Option<Vec<Header>> {
        if status_code == 408 {
            Some(vec![Header::from(ConnectionValue::Close)])
        } else {
            None
        }
    }

    /// Get [`StandardResponse`]
    #[must_use]
    #[inline]
    pub fn response(status: &Standard) -> &'static StandardResponse {
        match status {
            Standard::Ok200 => &CACHE_200,
            Standard::NotFound404 => &CACHE_404,
            Standard::NoContent204 => &CACHE_204,
            Standard::Created201 => &CACHE_201,
            Standard::Continue100 => &CACHE_100,
            Standard::BadRequest400 => &CACHE_400,
            Standard::InternalServerError500 => &CACHE_500,
            Standard::MethodNotAllowed405 => &CACHE_405,
            Standard::RequestTimeout408 => &CACHE_408,
            Standard::PayloadTooLarge413 => &CACHE_413,
            Standard::UriTooLong414 => &CACHE_414,
            Standard::ExpectationFailed417 => &CACHE_417,
            Standard::RequestHeaderFieldsTooLarge431 => &CACHE_431,
            Standard::NotImplemented501 => &CACHE_501,
            Standard::HttpVersionNotSupported505 => &CACHE_505,
        }
    }
}

impl From<&Standard> for &'static StandardResponse {
    fn from(status: &Standard) -> Self {
        Standard::response(status)
    }
}

impl From<Standard> for &'static StandardResponse {
    fn from(status: Standard) -> Self {
        Standard::response(&status)
    }
}

impl From<Standard> for StatusCode {
    fn from(value: Standard) -> Self {
        match value {
            Standard::Continue100 => 100,
            Standard::Ok200 => 200,
            Standard::Created201 => 201,
            Standard::NoContent204 => 204,
            Standard::BadRequest400 => 400,
            Standard::NotFound404 => 404,
            Standard::MethodNotAllowed405 => 405,
            Standard::RequestTimeout408 => 408,
            Standard::PayloadTooLarge413 => 413,
            Standard::UriTooLong414 => 414,
            Standard::ExpectationFailed417 => 417,
            Standard::RequestHeaderFieldsTooLarge431 => 431,
            Standard::InternalServerError500 => 500,
            Standard::NotImplemented501 => 501,
            Standard::HttpVersionNotSupported505 => 505,
        }
        .into()
    }
}

impl TryFrom<StatusCode> for Standard {
    type Error = ();

    fn try_from(status_code: StatusCode) -> Result<Self, Self::Error> {
        Ok(match status_code.0 {
            200 => Self::Ok200,
            404 => Self::NotFound404,
            204 => Self::NoContent204,
            201 => Self::Created201,
            100 => Self::Continue100,
            400 => Self::BadRequest400,
            500 => Self::InternalServerError500,
            405 => Self::MethodNotAllowed405,
            408 => Self::RequestTimeout408,
            413 => Self::PayloadTooLarge413,
            414 => Self::UriTooLong414,
            417 => Self::ExpectationFailed417,
            431 => Self::RequestHeaderFieldsTooLarge431,
            501 => Self::NotImplemented501,
            505 => Self::HttpVersionNotSupported505,
            _ => return Err(()),
        })
    }
}

impl TryFrom<u16> for Standard {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::try_from(StatusCode(value))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, convert::TryFrom, time::Instant};

    use crate::{Response, StatusCode};

    use super::{Standard, StandardResponse};

    #[test]
    fn init_test() {
        let status_list = [
            100u16, 200, 201, 204, 400, 404, 405, 408, 413, 414, 417, 431, 500, 501, 505,
        ];
        let r_list = [
            Standard::Continue100,
            Standard::Ok200,
            Standard::Created201,
            Standard::NoContent204,
            Standard::BadRequest400,
            Standard::NotFound404,
            Standard::MethodNotAllowed405,
            Standard::RequestTimeout408,
            Standard::PayloadTooLarge413,
            Standard::UriTooLong414,
            Standard::ExpectationFailed417,
            Standard::RequestHeaderFieldsTooLarge431,
            Standard::InternalServerError500,
            Standard::NotImplemented501,
            Standard::HttpVersionNotSupported505,
        ];

        assert_eq!(status_list.len(), r_list.len());

        let mut count = 0usize;

        for r in r_list {
            match r {
                Standard::Continue100
                | Standard::Ok200
                | Standard::Created201
                | Standard::NoContent204
                | Standard::BadRequest400
                | Standard::NotFound404
                | Standard::MethodNotAllowed405
                | Standard::RequestTimeout408
                | Standard::PayloadTooLarge413
                | Standard::UriTooLong414
                | Standard::ExpectationFailed417
                | Standard::RequestHeaderFieldsTooLarge431
                | Standard::InternalServerError500
                | Standard::NotImplemented501
                | Standard::HttpVersionNotSupported505 => count += 1,
            }
        }

        assert_eq!(count, r_list.len());

        for (status, standard) in status_list.iter().zip(r_list.iter()) {
            let resp = Response::from(*status);
            let resp_standard = <&StandardResponse>::from(standard);
            assert_eq!(
                resp.status_code(),
                resp_standard.status_code(),
                "status: {status}"
            );
        }
    }

    #[test]
    fn response_from_bench() {
        let l1 = HashSet::from([
            400u16, 100u16, 201, 417, 505, 500, 405, 204, 404, 501, 200, 413, 431, 408, 414,
        ]);

        let l2 = HashSet::from([
            Standard::BadRequest400,
            Standard::Continue100,
            Standard::Created201,
            Standard::ExpectationFailed417,
            Standard::HttpVersionNotSupported505,
            Standard::InternalServerError500,
            Standard::MethodNotAllowed405,
            Standard::NoContent204,
            Standard::NotFound404,
            Standard::NotImplemented501,
            Standard::Ok200,
            Standard::PayloadTooLarge413,
            Standard::RequestHeaderFieldsTooLarge431,
            Standard::RequestTimeout408,
            Standard::UriTooLong414,
        ]);

        let rounds = 100_000;

        assert_eq!(l1.len(), l2.len());

        let now = Instant::now();
        for _ in 0..rounds {
            for status in &l1 {
                let response = Response::from(*status);
                let _hint = std::hint::black_box(response);
            }
        }
        let elaps1 = now.elapsed();

        let now = Instant::now();
        for _ in 0..rounds {
            for status in &l2 {
                let response = Standard::response(status).clone();
                let _hint = std::hint::black_box(response);
            }
        }
        let elaps2 = now.elapsed();

        assert!(
            elaps1 > elaps2,
            "elaps1: {}, elaps2: {}",
            elaps1.as_micros(),
            elaps2.as_micros()
        );
    }

    #[test]
    fn try_from_status_code_test() {
        for n in 100..600u16 {
            let std = Standard::try_from(n);
            if let Ok(std) = std {
                let status_code: StatusCode = match std {
                    Standard::Continue100 => 100,
                    Standard::Ok200 => 200,
                    Standard::Created201 => 201,
                    Standard::NoContent204 => 204,
                    Standard::BadRequest400 => 400,
                    Standard::NotFound404 => 404,
                    Standard::MethodNotAllowed405 => 405,
                    Standard::RequestTimeout408 => 408,
                    Standard::PayloadTooLarge413 => 413,
                    Standard::UriTooLong414 => 414,
                    Standard::ExpectationFailed417 => 417,
                    Standard::RequestHeaderFieldsTooLarge431 => 431,
                    Standard::InternalServerError500 => 500,
                    Standard::NotImplemented501 => 501,
                    Standard::HttpVersionNotSupported505 => 505,
                }
                .into();

                assert_eq!(
                    std,
                    Standard::try_from(status_code).unwrap(),
                    "status_code failed: {status_code}"
                );

                let status_code = StatusCode::from(std);
                let std2 = Standard::try_from(status_code);
                assert!(std2.is_ok(), "status_code failed: {}", status_code);
                assert_eq!(std, std2.unwrap(), "status_code failed: {status_code}");
            }
        }
    }
}
