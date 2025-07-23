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
use std::future::Future;
use std::io::IoSlice;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};
use tokio_rustls::{TlsAcceptor, TlsConnector, rustls};

trait TlsStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin {}

impl<T> TlsStream for T where T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin {}

pub struct TlsTransport {
    stream: Option<Box<dyn TlsStream>>,
    remote_addr: Option<SocketAddr>,
    server_name: Option<String>,
    tls_config: Option<Arc<rustls::ClientConfig>>,
    config: ClientConfig,
}

impl std::fmt::Debug for TlsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsTransport")
            .field("stream", &self.stream.is_some())
            .field("remote_addr", &self.remote_addr)
            .field("server_name", &self.server_name)
            .finish()
    }
}

impl TlsTransport {
    /// Create a new TlsTransport for the given remote address and server name (not yet connected)
    pub fn new(remote_addr: SocketAddr, server_name: String) -> Self {
        Self {
            stream: None,
            remote_addr: Some(remote_addr),
            server_name: Some(server_name),
            tls_config: None,
            config: ClientConfig::default(),
        }
    }

    /// Create a new TlsTransport with custom configuration
    pub fn new_with_config(
        remote_addr: SocketAddr,
        server_name: String,
        config: ClientConfig,
        tls_config: Option<Arc<rustls::ClientConfig>>,
    ) -> Self {
        Self {
            stream: None,
            remote_addr: Some(remote_addr),
            server_name: Some(server_name),
            tls_config,
            config,
        }
    }

    /// Create TlsTransport from an already established TLS stream
    pub fn from_stream<S>(stream: S) -> Self
    where
        S: TlsStream + 'static,
    {
        Self {
            stream: Some(Box::new(stream)),
            remote_addr: None,
            server_name: None,
            tls_config: None,
            config: ClientConfig::default(),
        }
    }

    /// Legacy method for backward compatibility
    pub async fn connect(addr: &str, domain: &str) -> Result<Self, TransportError> {
        Self::connect_with_config(addr, domain, &ClientConfig::default(), None).await
    }

    /// Legacy method for backward compatibility
    pub async fn connect_with_config(
        addr: &str,
        domain: &str,
        config: &ClientConfig,
        tls_config: Option<Arc<rustls::ClientConfig>>,
    ) -> Result<Self, TransportError> {
        let remote_addr: SocketAddr = addr
            .parse()
            .map_err(|e| TransportError::Handshake(format!("Invalid address: {}", e)))?;

        let mut transport =
            Self::new_with_config(remote_addr, domain.to_string(), config.clone(), tls_config);

        transport.handshake().await?;
        Ok(transport)
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
    fn handshake<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            // If already connected, nothing to do
            if self.stream.is_some() {
                return Ok(());
            }

            // Get required connection parameters
            let remote_addr = self.remote_addr.ok_or_else(|| {
                TransportError::Handshake("No remote address specified".to_string())
            })?;

            let server_name = self
                .server_name
                .as_ref()
                .ok_or_else(|| TransportError::Handshake("No server name specified".to_string()))?
                .clone();

            // 1. Establish TCP connection
            let tcp_stream = timeout(self.config.connect_timeout, TcpStream::connect(remote_addr))
                .await
                .map_err(|_| TransportError::Timeout)?
                .map_err(TransportError::Io)?;

            // 2. Setup TLS configuration
            let tls_config = self.tls_config.clone().unwrap_or_else(|| {
                use rustls::RootCertStore;
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

            // 3. Perform TLS handshake
            let connector = TlsConnector::from(tls_config);
            let domain = rustls::ServerName::try_from(server_name.as_str())
                .map_err(|e| TransportError::Tls(Box::new(e)))?;

            let tls_stream = timeout(
                self.config.connect_timeout,
                connector.connect(domain, tcp_stream),
            )
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::Tls(Box::new(e)))?;

            self.stream = Some(Box::new(tls_stream));
            Ok(())
        })
    }

    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            let stream = self.stream.as_mut().ok_or(TransportError::NotConnected)?;
            
            stream
                .write_vectored(buffers)
                .await
                .map(|_| ()) // Ignore bytes written count
                .map_err(TransportError::Io)
        })
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            let stream = self.stream.as_mut().ok_or(TransportError::NotConnected)?;
            stream.read(buffer).await.map_err(TransportError::Io)
        })
    }

    fn shutdown<'a>(
        &'a mut self,
        timeout_duration: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if let Some(ref mut stream) = self.stream {
                // Try graceful TLS shutdown first with timeout
                let graceful_result = timeout(timeout_duration, stream.shutdown()).await;

                // If graceful shutdown fails or times out, force close the connection
                match graceful_result {
                    Ok(Ok(())) => {
                        // Graceful TLS shutdown succeeded
                    }
                    Ok(Err(_io_error)) => {
                        // Graceful TLS shutdown failed, force close by dropping the stream
                        // The stream will be closed when it goes out of scope
                    }
                    Err(_timeout_error) => {
                        // Timeout occurred, force close by dropping the stream
                        // The stream will be closed when it goes out of scope
                    }
                }
            }

            // Clear the stream to indicate disconnected state
            self.stream = None;
        })
    }
}
