use std::io::{Error as IoError, Read};

use crate::{Response, StatusCode};

/// `Result` returned by [`Request::respond`](crate::Request::respond)
pub type ResponseResult = Result<Data, IoError>;

/// `Data` of successful [`ResponseResult`]
#[derive(Debug)]
pub struct Data {
    /// Size of response data
    pub content_length: usize,
    /// Responded [`StatusCode`]
    pub status_code: StatusCode,
}

impl<R> From<&Response<R>> for Data
where
    R: Read,
{
    fn from(response: &Response<R>) -> Self {
        let content_length;
        #[cfg(feature = "range-support")]
        {
            content_length = if let Some(range_header) = &response.content_range {
                if let Some(content_length) =
                    crate::common::range_header::response::range_content_length(range_header)
                {
                    content_length
                } else {
                    response.data_length.unwrap_or_default()
                }
            } else {
                response.data_length.unwrap_or_default()
            };
        }
        #[cfg(not(feature = "range-support"))]
        {
            content_length = response.data_length.unwrap_or_default();
        }

        Self {
            content_length,
            status_code: response.status_code,
        }
    }
}

impl<R> From<Response<R>> for Data
where
    R: Read,
{
    fn from(response: Response<R>) -> Self {
        Self::from(&response)
    }
}

impl<R> From<&Response<R>> for ResponseResult
where
    R: Read,
{
    fn from(response: &Response<R>) -> Self {
        Ok(Data::from(response))
    }
}

impl<R> From<Response<R>> for ResponseResult
where
    R: Read,
{
    fn from(response: Response<R>) -> Self {
        Ok(Data::from(response))
    }
}
