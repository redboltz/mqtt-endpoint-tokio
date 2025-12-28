// MIT License
//
// Copyright (c) 2025 Takatoshi Kondo
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

/// Helper functions for establishing connections with various transport layers.
///
/// This module provides convenience functions for creating connected streams
/// that can be used with transport `from_stream` methods. It handles the
/// multi-step handshake processes for TLS and WebSocket connections.
use super::TransportError;
#[cfg(feature = "quic")]
use quinn::{RecvStream, SendStream};
#[cfg(any(feature = "ws", feature = "quic"))]
use std::collections::HashMap;
#[cfg(any(feature = "ws", feature = "quic"))]
use std::net::SocketAddr;
#[cfg(any(feature = "tls", feature = "quic"))]
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::time::Duration;
#[cfg(feature = "tls")]
use tokio_rustls::{client::TlsStream, rustls, TlsConnector};
#[cfg(feature = "ws")]
use tokio_tungstenite::{
    connect_async, tungstenite::http::Request, MaybeTlsStream, WebSocketStream,
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
    match timeout {
        Some(timeout_duration) => tokio::time::timeout(timeout_duration, TcpStream::connect(addr))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io),
        None => TcpStream::connect(addr).await.map_err(TransportError::Io),
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
#[cfg(feature = "tls")]
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
        let cert_result = rustls_native_certs::load_native_certs();
        for cert in cert_result.certs {
            let _ = root_store.add(cert);
        }
        Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        )
    });

    // 3. Perform TLS handshake
    let connector = TlsConnector::from(tls_config);
    let server_name = rustls::pki_types::ServerName::try_from(domain.to_owned())
        .map_err(|e| TransportError::Tls(Box::new(e)))?;

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
#[cfg(feature = "ws")]
pub async fn connect_tcp_ws(
    addr: &str,
    path: &str,
    headers: Option<HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, TransportError> {
    // Parse address to get host and port
    let (host, port) = if let Ok(parsed_addr) = addr.parse::<SocketAddr>() {
        (parsed_addr.ip().to_string(), parsed_addr.port())
    } else {
        // Try to extract host and port from "hostname:port" format
        let parts: Vec<&str> = addr.split(':').collect();
        if parts.len() != 2 {
            return Err(TransportError::Connect(
                "Invalid address format".to_string(),
            ));
        }
        let host = parts[0].to_string();
        let port: u16 = parts[1]
            .parse()
            .map_err(|_| TransportError::Connect("Invalid port number".to_string()))?;
        (host, port)
    };
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
#[cfg(all(feature = "tls", feature = "ws"))]
pub async fn connect_tcp_tls_ws(
    addr: &str,
    domain: &str,
    path: &str,
    tls_config: Option<Arc<rustls::ClientConfig>>,
    headers: Option<HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<WebSocketStream<TlsStream<TcpStream>>, TransportError> {
    // Parse address to get port for URL construction
    let port = if let Ok(parsed_addr) = addr.parse::<SocketAddr>() {
        parsed_addr.port()
    } else {
        // Try to extract port from "hostname:port" format
        addr.split(':')
            .nth(1)
            .and_then(|port_str| port_str.parse().ok())
            .ok_or_else(|| TransportError::Connect("Invalid address format".to_string()))?
    };
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

/// Establishes a QUIC connection and opens a bidirectional stream.
///
/// This function creates a QUIC connection and opens a bidirectional stream
/// that can be used with `QuicTransport::from_streams`. It handles the full
/// QUIC handshake process.
///
/// # Parameters
///
/// * `addr` - The socket address to connect to (e.g., "127.0.0.1:4433")
/// * `domain` - The server name for certificate verification (use None for insecure connection)
/// * `timeout` - Optional timeout for the connection process (None for no timeout)
///
/// # Returns
///
/// Returns a tuple of `(SendStream, RecvStream)` or a `TransportError` if connection fails.
///
/// # Examples
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{connect_helper, QuicTransport};
/// use tokio::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Insecure connection (development only)
/// let (send_stream, recv_stream) = connect_helper::connect_quic("127.0.0.1:4433", None, None).await?;
/// let transport = QuicTransport::from_streams(send_stream, recv_stream);
///
/// // Secure connection with certificate verification
/// let (send_stream, recv_stream) = connect_helper::connect_quic("broker.example.com:4433", Some("broker.example.com"), Some(Duration::from_secs(30))).await?;
/// let transport = QuicTransport::from_streams(send_stream, recv_stream);
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "quic")]
pub async fn connect_quic(
    addr: &str,
    domain: Option<&str>,
    timeout: Option<Duration>,
) -> Result<(SendStream, RecvStream), TransportError> {
    use quinn::{ClientConfig, Endpoint};

    // Parse the address with hostname resolution support
    let socket_addr: SocketAddr = if let Ok(parsed_addr) = addr.parse() {
        // Already a valid SocketAddr (e.g., "127.0.0.1:14567")
        parsed_addr
    } else {
        // Try to resolve hostname (e.g., "localhost:14567")
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host(addr)
            .await
            .map_err(|e| TransportError::Connect(format!("Failed to resolve hostname: {e}")))?
            .collect();

        // Prefer IPv4 address over IPv6
        addrs
            .iter()
            .find(|addr| addr.is_ipv4())
            .or_else(|| addrs.first())
            .copied()
            .ok_or_else(|| TransportError::Connect("No addresses found for hostname".to_string()))?
    };

    // Create QUIC client configuration
    let client_config = if let Some(_domain_name) = domain {
        // Secure configuration with certificate verification
        let mut root_store = rustls::RootCertStore::empty();
        let cert_result = rustls_native_certs::load_native_certs();
        for cert in cert_result.certs {
            let _ = root_store.add(cert);
        }

        let rustls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        // Note: ALPN can be enabled if broker requires it
        // rustls_config.alpn_protocols = vec![b"mqtt".to_vec()];
        let rustls_config = Arc::new(rustls_config);

        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
                .map_err(|e| TransportError::Quic(Box::new(e)))?,
        ));

        // Configure transport parameters for longer connections
        let mut transport_config = quinn::TransportConfig::default();
        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(30)));
        transport_config.max_idle_timeout(Some(
            std::time::Duration::from_secs(300).try_into().unwrap(),
        ));
        client_config.transport_config(Arc::new(transport_config));

        client_config
    } else {
        // Insecure configuration (development only)
        // Create a custom verifier that accepts all certificates
        #[derive(Debug)]
        struct NoVerification;

        impl rustls::client::danger::ServerCertVerifier for NoVerification {
            fn verify_server_cert(
                &self,
                _end_entity: &rustls::pki_types::CertificateDer<'_>,
                _intermediates: &[rustls::pki_types::CertificateDer<'_>],
                _server_name: &rustls::pki_types::ServerName<'_>,
                _ocsp_response: &[u8],
                _now: rustls::pki_types::UnixTime,
            ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                _message: &[u8],
                _cert: &rustls::pki_types::CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
            {
                Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
            }

            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &rustls::pki_types::CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
            {
                Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
            }

            fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                vec![
                    rustls::SignatureScheme::RSA_PKCS1_SHA1,
                    rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                    rustls::SignatureScheme::RSA_PKCS1_SHA256,
                    rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                    rustls::SignatureScheme::RSA_PKCS1_SHA384,
                    rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                    rustls::SignatureScheme::RSA_PKCS1_SHA512,
                    rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                    rustls::SignatureScheme::RSA_PSS_SHA256,
                    rustls::SignatureScheme::RSA_PSS_SHA384,
                    rustls::SignatureScheme::RSA_PSS_SHA512,
                    rustls::SignatureScheme::ED25519,
                    rustls::SignatureScheme::ED448,
                ]
            }
        }

        let rustls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerification))
            .with_no_client_auth();
        // Note: ALPN can be enabled if broker requires it
        // rustls_config.alpn_protocols = vec![b"mqtt".to_vec()];
        let rustls_config = Arc::new(rustls_config);

        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
                .map_err(|e| TransportError::Quic(Box::new(e)))?,
        ));

        // Configure transport parameters for longer connections
        let mut transport_config = quinn::TransportConfig::default();
        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(30)));
        transport_config.max_idle_timeout(Some(
            std::time::Duration::from_secs(300).try_into().unwrap(),
        ));
        client_config.transport_config(Arc::new(transport_config));

        client_config
    };

    // Create QUIC endpoint
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
        .map_err(|e| TransportError::Quic(Box::new(e)))?;
    endpoint.set_default_client_config(client_config);

    // Establish connection
    let server_name = domain.unwrap_or("localhost");
    let connecting = endpoint
        .connect(socket_addr, server_name)
        .map_err(|e| TransportError::Quic(Box::new(e)))?;

    let connection = match timeout {
        Some(timeout_duration) => tokio::time::timeout(timeout_duration, connecting)
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::Quic(Box::new(e)))?,
        None => connecting
            .await
            .map_err(|e| TransportError::Quic(Box::new(e)))?,
    };

    // Open bidirectional stream
    let (send_stream, recv_stream) = match timeout {
        Some(timeout_duration) => tokio::time::timeout(timeout_duration, connection.open_bi())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::Quic(Box::new(e)))?,
        None => connection
            .open_bi()
            .await
            .map_err(|e| TransportError::Quic(Box::new(e)))?,
    };

    Ok((send_stream, recv_stream))
}
