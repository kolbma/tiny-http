use crate::util::refined_tcp_stream::Stream as RefinedStream;
use crate::ConnectionStream;
use std::error::Error;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// A wrapper around an owned Rustls connection and corresponding stream.
///
/// Uses an internal Mutex to permit disparate reader & writer threads to access the stream independently.
pub(crate) struct RustlsStream(
    Arc<Mutex<rustls::StreamOwned<rustls::ServerConnection, ConnectionStream>>>,
);

impl RustlsStream {
    pub(crate) fn peer_addr(&mut self) -> std::io::Result<Option<SocketAddr>> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .sock
            .peer_addr()
    }

    pub(crate) fn read_timeout(&self) -> std::io::Result<Option<std::time::Duration>> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .sock
            .read_timeout()
    }

    pub(crate) fn set_read_timeout(
        &mut self,
        dur: Option<std::time::Duration>,
    ) -> std::io::Result<()> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .sock
            .set_read_timeout(dur)
    }

    pub(crate) fn shutdown(&mut self, how: Shutdown) -> std::io::Result<()> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .sock
            .shutdown(how)
    }
}

impl Clone for RustlsStream {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Read for RustlsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .read(buf)
    }
}

impl Write for RustlsStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .flush()
    }
}

pub(crate) struct RustlsContext(Arc<rustls::ServerConfig>);

impl RustlsContext {
    pub(crate) fn from_pem(
        certificates: &Vec<u8>,
        private_key: &Zeroizing<Vec<u8>>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let certificate_chain: Vec<rustls::pki_types::CertificateDer<'_>> =
            rustls_pemfile::certs(&mut certificates.as_slice())
                .flatten()
                .collect();

        if certificate_chain.is_empty() {
            return Err("Couldn't extract certificate chain from config.".into());
        }

        let private_key = {
            let pkcs8_keys = rustls_pemfile::pkcs8_private_keys(&mut private_key.as_slice())
                .collect::<Vec<Result<_, _>>>();
            let is_pkcs8_keys_empty = pkcs8_keys.is_empty();

            if let Some(pkcs8_key) = pkcs8_keys.into_iter().find_map(Result::ok) {
                rustls::pki_types::PrivateKeyDer::Pkcs8(pkcs8_key)
            } else if !is_pkcs8_keys_empty {
                return Err(
                    "file contains invalid pkcs8 private key (encrypted keys are not supported)"
                        .into(),
                );
            } else {
                let rsa_key = rustls_pemfile::rsa_private_keys(&mut private_key.as_slice())
                    .flatten()
                    .next()
                    .expect("file contains invalid rsa private key");
                rustls::pki_types::PrivateKeyDer::Pkcs1(rsa_key)
            }
        };

        let tls_conf = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificate_chain, private_key)?;

        Ok(Self(Arc::new(tls_conf)))
    }

    pub(crate) fn accept(
        &self,
        stream: ConnectionStream,
    ) -> Result<RustlsStream, Box<dyn Error + Send + Sync + 'static>> {
        let connection = rustls::ServerConnection::new(self.0.clone())?;
        Ok(RustlsStream(Arc::new(Mutex::new(
            rustls::StreamOwned::new(connection, stream),
        ))))
    }
}

impl From<RustlsStream> for RefinedStream {
    fn from(stream: RustlsStream) -> Self {
        Self::Https(stream)
    }
}
