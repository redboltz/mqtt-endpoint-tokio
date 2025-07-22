/**
 * MIT License
 *
 * Copyright (c) 2025 Takatoshi Kondo
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
use super::{ClientConfig, ServerConfig, TransportError, TransportOps};
use std::io::IoSlice;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};
use tokio_rustls::{TlsAcceptor, TlsConnector, rustls};

trait TlsStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin {}

impl<T> TlsStream for T where T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin {}

pub struct TlsTransport {
    stream: Box<dyn TlsStream>,
}

impl std::fmt::Debug for TlsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsTransport")
            .field("stream", &"<TLS stream>")
            .finish()
    }
}

impl TlsTransport {
    pub fn from_stream<S>(stream: S) -> Self
    where
        S: TlsStream + 'static,
    {
        Self {
            stream: Box::new(stream),
        }
    }

    pub async fn connect(addr: &str, domain: &str) -> Result<Self, TransportError> {
        Self::connect_with_config(addr, domain, &ClientConfig::default(), None).await
    }

    pub async fn connect_with_config(
        addr: &str,
        domain: &str,
        config: &ClientConfig,
        tls_config: Option<Arc<rustls::ClientConfig>>,
    ) -> Result<Self, TransportError> {
        let tcp_stream = timeout(config.connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;

        let tls_config = tls_config.unwrap_or_else(|| {
            use rustls::RootCertStore;
            use std::sync::Arc;
            let mut root_store = RootCertStore::empty();
            for cert in rustls_native_certs::load_native_certs().unwrap_or_default() {
                let _ = root_store.add(&rustls::Certificate(cert.0));
            }
            Arc::new(
                rustls::ClientConfig::builder()
                    .with_safe_defaults()
                    .with_root_certificates(root_store)
                    .with_no_client_auth(),
            )
        });

        let connector = TlsConnector::from(tls_config);
        let domain =
            rustls::ServerName::try_from(domain).map_err(|e| TransportError::Tls(Box::new(e)))?;

        let tls_stream = timeout(
            config.connect_timeout,
            connector.connect(domain, tcp_stream),
        )
        .await
        .map_err(|_| TransportError::Timeout)?
        .map_err(|e| TransportError::Tls(Box::new(e)))?;

        Ok(Self::from_stream(tls_stream))
    }

    pub async fn accept(
        listener: &TcpListener,
        acceptor: Arc<TlsAcceptor>,
    ) -> Result<Self, TransportError> {
        Self::accept_with_config(listener, acceptor, &ServerConfig::default()).await
    }

    pub async fn accept_with_config(
        listener: &TcpListener,
        acceptor: Arc<TlsAcceptor>,
        config: &ServerConfig,
    ) -> Result<Self, TransportError> {
        let (tcp_stream, _addr) = timeout(config.accept_timeout, listener.accept())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;

        let tls_stream = timeout(config.accept_timeout, acceptor.accept(tcp_stream))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::Tls(Box::new(e)))?;

        Ok(Self::from_stream(tls_stream))
    }
}

impl TransportOps for TlsTransport {
    async fn send(&mut self, buffers: &[IoSlice<'_>]) -> Result<(), TransportError> {
        for buf in buffers {
            self.stream
                .write_all(buf)
                .await
                .map_err(TransportError::Io)?;
        }
        Ok(())
    }

    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize, TransportError> {
        self.stream.read(buffer).await.map_err(TransportError::Io)
    }

    async fn shutdown(&mut self, timeout_duration: Duration) -> Result<(), TransportError> {
        timeout(timeout_duration, self.stream.shutdown())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)
    }
}
