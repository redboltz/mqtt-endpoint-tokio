use super::{TransportError, TransportOps, ClientConfig, ServerConfig};
use std::io::IoSlice;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, TcpListener};
use tokio::time::{timeout, Duration};

#[derive(Debug)]
pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    pub fn from_stream(stream: TcpStream) -> Self {
        Self { stream }
    }

    pub async fn connect(addr: &str) -> Result<Self, TransportError> {
        Self::connect_with_config(addr, &ClientConfig::default()).await
    }

    pub async fn connect_with_config(addr: &str, config: &ClientConfig) -> Result<Self, TransportError> {
        let stream = timeout(config.connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;
        
        Ok(Self::from_stream(stream))
    }

    pub async fn accept(listener: &TcpListener) -> Result<Self, TransportError> {
        Self::accept_with_config(listener, &ServerConfig::default()).await
    }

    pub async fn accept_with_config(listener: &TcpListener, config: &ServerConfig) -> Result<Self, TransportError> {
        let (stream, _addr) = timeout(config.accept_timeout, listener.accept())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;
        
        Ok(Self::from_stream(stream))
    }
}

impl TransportOps for TcpTransport {
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