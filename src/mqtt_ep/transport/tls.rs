use super::{TransportError, TransportOps, ClientConfig, ServerConfig};
use std::io::IoSlice;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, TcpListener};
use tokio::time::{timeout, Duration};
use tokio_rustls::{TlsStream, TlsConnector, TlsAcceptor, rustls};

#[derive(Debug)]
pub struct TlsTransport {
    stream: TlsStream<TcpStream>,
}

impl TlsTransport {
    pub fn from_stream(stream: TlsStream<TcpStream>) -> Self {
        Self { stream }
    }

    pub async fn connect(addr: &str, domain: &str) -> Result<Self, TransportError> {
        Self::connect_with_config(addr, domain, &ClientConfig::default(), None).await
    }

    pub async fn connect_with_config(
        addr: &str, 
        domain: &str, 
        config: &ClientConfig,
        tls_config: Option<Arc<rustls::ClientConfig>>
    ) -> Result<Self, TransportError> {
        let tcp_stream = timeout(config.connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;

        let tls_config = tls_config.unwrap_or_else(|| {
            Arc::new(
                rustls::ClientConfig::builder()
                    .with_safe_defaults()
                    .with_root_certificates(rustls_native_certs::load_native_certs().unwrap_or_default())
                    .with_no_client_auth()
            )
        });

        let connector = TlsConnector::from(tls_config);
        let domain = rustls::ServerName::try_from(domain)
            .map_err(|e| TransportError::Tls(Box::new(e)))?;

        let tls_stream = timeout(config.connect_timeout, connector.connect(domain, tcp_stream))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::Tls(Box::new(e)))?;

        Ok(Self::from_stream(tls_stream))
    }

    pub async fn accept(listener: &TcpListener, acceptor: Arc<TlsAcceptor>) -> Result<Self, TransportError> {
        Self::accept_with_config(listener, acceptor, &ServerConfig::default()).await
    }

    pub async fn accept_with_config(
        listener: &TcpListener, 
        acceptor: Arc<TlsAcceptor>,
        config: &ServerConfig
    ) -> Result<Self, TransportError> {
        let (tcp_stream, _addr) = timeout(config.accept_timeout, listener.accept())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;

        let tls_stream = timeout(config.accept_timeout, acceptor.accept(tcp_stream))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::Tls(Box::new(e)))?;

        Ok(Self::from_stream(tls_stream.into()))
    }
}

impl TransportOps for TlsTransport {
    async fn send(&mut self, buffers: &[IoSlice<'_>]) -> Result<(), TransportError> {
        use tokio::io::AsyncWriteExt;
        self.stream.write_vectored_all(buffers).await.map_err(TransportError::Io)
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