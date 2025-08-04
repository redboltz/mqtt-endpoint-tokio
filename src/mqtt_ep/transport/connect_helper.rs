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
/// Helper functions for establishing connections with various transport layers.
///
/// This module provides convenience functions for creating connected streams
/// that can be used with transport `from_stream` methods. It handles the
/// multi-step handshake processes for TLS and WebSocket connections.
use super::TransportError;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::time::Duration;
use tokio_rustls::{TlsConnector, client::TlsStream, rustls};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::http::Request,
};

/// Establishes a TCP connection to the specified address.
///
/// This function creates a basic TCP connection that can be used with
/// `TcpTransport::from_stream`.
///
/// # Parameters
///
/// * `addr` - The socket address to connect to (e.g., "127.0.0.1:1883")
/// * `timeout` - Optional timeout for the connection (None for no timeout)
///
/// # Returns
///
/// Returns a connected `TcpStream` or a `TransportError` if the connection fails.
///
/// # Examples
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{connect_helper, TcpTransport};
/// use tokio::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Without timeout
/// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
/// let transport = TcpTransport::from_stream(tcp_stream);
///
/// // With timeout
/// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", Some(Duration::from_secs(10))).await?;
/// let transport = TcpTransport::from_stream(tcp_stream);
/// # Ok(())
/// # }
/// ```
pub async fn connect_tcp(
    addr: &str,
    timeout: Option<Duration>,
) -> Result<TcpStream, TransportError> {
    let socket_addr: SocketAddr = addr
        .parse()
        .map_err(|e| TransportError::Connect(format!("Invalid address: {e}")))?;

    match timeout {
        Some(timeout_duration) => {
            tokio::time::timeout(timeout_duration, TcpStream::connect(socket_addr))
                .await
                .map_err(|_| TransportError::Timeout)?
                .map_err(TransportError::Io)
        }
        None => TcpStream::connect(socket_addr)
            .await
            .map_err(TransportError::Io),
    }
}

/// Establishes a TCP connection followed by a TLS handshake.
///
/// This function creates a TLS-encrypted connection that can be used with
/// `TlsTransport::from_stream`. It handles both TCP connection establishment
/// and TLS handshake.
///
/// # Parameters
///
/// * `addr` - The socket address to connect to (e.g., "127.0.0.1:8883")
/// * `domain` - The server name for TLS certificate verification
/// * `tls_config` - Optional custom TLS configuration (uses default if None)
/// * `timeout` - Optional timeout for the entire connection process (None for no timeout)
///
/// # Returns
///
/// Returns a connected `TlsStream<TcpStream>` or a `TransportError` if any step fails.
///
/// # Examples
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{connect_helper, TlsTransport};
/// use tokio::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Without timeout
/// let tls_stream = connect_helper::connect_tcp_tls("127.0.0.1:8883", "localhost", None, None).await?;
/// let transport = TlsTransport::from_stream(tls_stream);
///
/// // With timeout
/// let tls_stream = connect_helper::connect_tcp_tls("127.0.0.1:8883", "localhost", None, Some(Duration::from_secs(30))).await?;
/// let transport = TlsTransport::from_stream(tls_stream);
/// # Ok(())
/// # }
/// ```
pub async fn connect_tcp_tls(
    addr: &str,
    domain: &str,
    tls_config: Option<Arc<rustls::ClientConfig>>,
    timeout: Option<Duration>,
) -> Result<TlsStream<TcpStream>, TransportError> {
    // 1. Establish TCP connection
    let tcp_stream = connect_tcp(addr, timeout).await?;

    // 2. Setup TLS configuration
    let tls_config = tls_config.unwrap_or_else(|| {
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
    let server_name =
        rustls::ServerName::try_from(domain).map_err(|e| TransportError::Tls(Box::new(e)))?;

    match timeout {
        Some(timeout_duration) => {
            tokio::time::timeout(timeout_duration, connector.connect(server_name, tcp_stream))
                .await
                .map_err(|_| TransportError::Timeout)?
                .map_err(|e| TransportError::Tls(Box::new(e)))
        }
        None => connector
            .connect(server_name, tcp_stream)
            .await
            .map_err(|e| TransportError::Tls(Box::new(e))),
    }
}

/// Establishes a TCP connection followed by a WebSocket handshake.
///
/// This function creates a WebSocket connection over TCP that can be used with
/// `WebSocketTransport::from_tcp_client_stream`. It handles both TCP connection and
/// WebSocket handshake, automatically including the `Sec-WebSocket-Protocol: mqtt`
/// header for MQTT over WebSocket support.
///
/// # Parameters
///
/// * `addr` - The socket address to connect to (e.g., "127.0.0.1:8080")
/// * `path` - The WebSocket path (e.g., "/mqtt")
/// * `headers` - Optional HTTP headers for the WebSocket handshake
/// * `timeout` - Optional timeout for the entire connection process (None for no timeout)
///
/// # Returns
///
/// Returns a connected `WebSocketStream<MaybeTlsStream<TcpStream>>` or a `TransportError` if any step fails.
///
/// # Note
///
/// This function returns `WebSocketStream<MaybeTlsStream<TcpStream>>` which is compatible
/// with the old WebSocketTransport design. With the new specialized variants, you should
/// use direct connection establishment rather than this helper.
pub async fn connect_tcp_ws(
    addr: &str,
    path: &str,
    headers: Option<HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, TransportError> {
    // Parse address to get host and port
    let socket_addr: SocketAddr = addr
        .parse()
        .map_err(|e| TransportError::Connect(format!("Invalid address: {e}")))?;

    let host = socket_addr.ip().to_string();
    let port = socket_addr.port();
    let url = format!("ws://{host}:{port}{path}");

    // Create request with required WebSocket headers and optional custom headers
    let mut request_builder = Request::builder()
        .uri(&url)
        .header("Host", format!("{host}:{port}"))
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Protocol", "mqtt");

    if let Some(headers) = headers {
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }
    }

    let request = request_builder
        .body(())
        .map_err(|e| TransportError::Connect(format!("Failed to build request: {e}")))?;

    // Establish WebSocket connection
    let (ws_stream, _response) = match timeout {
        Some(timeout_duration) => tokio::time::timeout(timeout_duration, connect_async(request))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::WebSocket(Box::new(e)))?,
        None => connect_async(request)
            .await
            .map_err(|e| TransportError::WebSocket(Box::new(e)))?,
    };

    Ok(ws_stream)
}

/// Establishes a TCP connection, followed by TLS handshake, followed by WebSocket handshake.
///
/// This function creates a secure WebSocket connection over TLS that can be used with
/// `WebSocketTransport::from_tls_client_stream`. It handles TCP connection, TLS handshake,
/// and WebSocket handshake in sequence, automatically including the `Sec-WebSocket-Protocol: mqtt`
/// header for MQTT over WebSocket support.
///
/// # Parameters
///
/// * `addr` - The socket address to connect to (e.g., "broker.example.com:443")
/// * `domain` - The server name for TLS certificate verification
/// * `path` - The WebSocket path (e.g., "/mqtt")
/// * `tls_config` - Optional custom TLS configuration (uses default if None)
/// * `headers` - Optional HTTP headers for the WebSocket handshake
/// * `timeout` - Optional timeout for the entire connection process (None for no timeout)
///
/// # Returns
///
/// Returns a connected `WebSocketStream<TlsStream<TcpStream>>` or a `TransportError` if any step fails.
///
/// # Examples
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{connect_helper, WebSocketTransport};
/// use tokio::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Without timeout
/// let wss_stream = connect_helper::connect_tcp_tls_ws("broker.example.com:443", "broker.example.com", "/mqtt", None, None, None).await?;
/// let transport = WebSocketTransport::from_tls_client_stream(wss_stream);
///
/// // With timeout
/// let wss_stream = connect_helper::connect_tcp_tls_ws("broker.example.com:443", "broker.example.com", "/mqtt", None, None, Some(Duration::from_secs(60))).await?;
/// let transport = WebSocketTransport::from_tls_client_stream(wss_stream);
/// # Ok(())
/// # }
/// ```
pub async fn connect_tcp_tls_ws(
    addr: &str,
    domain: &str,
    path: &str,
    tls_config: Option<Arc<rustls::ClientConfig>>,
    headers: Option<HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<WebSocketStream<TlsStream<TcpStream>>, TransportError> {
    // Parse address to get host and port for connection
    let socket_addr: SocketAddr = addr
        .parse()
        .map_err(|e| TransportError::Connect(format!("Invalid address: {e}")))?;

    let port = socket_addr.port();
    let url = format!("wss://{domain}:{port}{path}");

    // 1. Establish TLS connection
    let tls_stream = connect_tcp_tls(addr, domain, tls_config, timeout).await?;

    // 2. Create WebSocket request with required headers and optional custom headers
    let mut request_builder = Request::builder()
        .uri(&url)
        .header("Host", format!("{domain}:{port}"))
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Protocol", "mqtt");

    if let Some(headers) = headers {
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }
    }

    let request = request_builder
        .body(())
        .map_err(|e| TransportError::Connect(format!("Failed to build request: {e}")))?;

    // 3. Perform WebSocket handshake over TLS
    let (ws_stream, _response) = match timeout {
        Some(timeout_duration) => tokio::time::timeout(
            timeout_duration,
            tokio_tungstenite::client_async(request, tls_stream),
        )
        .await
        .map_err(|_| TransportError::Timeout)?
        .map_err(|e| TransportError::WebSocket(Box::new(e)))?,
        None => tokio_tungstenite::client_async(request, tls_stream)
            .await
            .map_err(|e| TransportError::WebSocket(Box::new(e)))?,
    };

    Ok(ws_stream)
}
