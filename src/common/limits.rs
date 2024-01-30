//! Limits used in `ClientConnection` and [`Request`] handling
//!
//! [`Request`]: crate::Request
//!

/// Default maximum allowed concurrent client connections
pub const CONNECTION_LIMIT_DEFAULT: u16 = 200;
/// Default value for size of content buffer
pub const CONTENT_BUFFER_SIZE_DEFAULT: usize = 1024;
/// Default value for allowed length/size of a [`Header`](crate::Header) line
pub const HEADER_LINE_LEN_DEFAULT: usize = 2048;
/// Default value for allowed size of [`Header`](crate::Header)
pub const HEADER_MAX_SIZE_DEFAULT: usize = 8192;

/// Size of [`EqualReader`](crate::util::EqualReader) buffer
pub(crate) const EQUAL_READER_BUF_SIZE: usize = 256;
/// Size of [`LineReader`](crate::util::LineReader) buffer
pub(crate) const HEADER_READER_BUF_SIZE: usize = 256;

/// [`Config`] for `limits`
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Allowed concurrent client connections
    pub connection_limit: u16,
    /// Size of content buffer
    pub content_buffer_size: usize,
    /// Allowed length/size of a [`Header`](crate::Header) line
    pub header_line_len: usize,
    /// Allowed size of [`Header`](crate::Header)
    pub header_max_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connection_limit: CONNECTION_LIMIT_DEFAULT,
            content_buffer_size: CONTENT_BUFFER_SIZE_DEFAULT,
            header_line_len: HEADER_LINE_LEN_DEFAULT,
            header_max_size: HEADER_MAX_SIZE_DEFAULT,
        }
    }
}
