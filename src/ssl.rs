//! Modules providing SSL/TLS implementations. For backwards compatibility, OpenSSL is the default
//! implementation, but Rustls is highly recommended as a pure Rust alternative.
//!
//! In order to simplify the swappable implementations these SSL/TLS modules adhere to an implicit
//! trait contract and specific implementations are re-exported as [`SslContextImpl`] and [`SslStream`].
//! The concrete type of these aliases will depend on which module you enable in `Cargo.toml`.
#![cfg(any(
    feature = "ssl-openssl",
    feature = "ssl-rustls",
    feature = "ssl-native-tls"
))]

use zeroize::Zeroizing;

#[cfg(feature = "ssl-openssl")]
pub(crate) mod openssl;
#[cfg(feature = "ssl-openssl")]
pub(crate) use self::openssl::OpenSslContext as SslContextImpl;
#[cfg(feature = "ssl-openssl")]
pub(crate) use self::openssl::SplitOpenSslStream as SslStream;

#[cfg(feature = "ssl-rustls")]
pub(crate) mod rustls;
#[cfg(feature = "ssl-rustls")]
pub(crate) use self::rustls::RustlsContext as SslContextImpl;
#[cfg(feature = "ssl-rustls")]
pub(crate) use self::rustls::RustlsStream as SslStream;

#[cfg(feature = "ssl-native-tls")]
pub(crate) mod native_tls;
#[cfg(feature = "ssl-native-tls")]
pub(crate) use self::native_tls::NativeTlsContext as SslContextImpl;
#[cfg(feature = "ssl-native-tls")]
pub(crate) use self::native_tls::NativeTlsStream as SslStream;

/// Configuration of the server for SSL.
#[derive(Debug, Clone)]
pub struct SslConfig {
    /// Contains the public certificate to send to clients.
    pub(crate) certificate: Vec<u8>,
    /// Contains the ultra-secret private key used to decode communications.
    pub(crate) private_key: Zeroizing<Vec<u8>>,
}

impl SslConfig {
    /// Create `[SslConfig]`
    ///
    #[must_use]
    pub fn new(certificate: Vec<u8>, private_key: Vec<u8>) -> Self {
        Self {
            certificate,
            private_key: Zeroizing::new(private_key),
        }
    }
}
