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
use mqtt_endpoint_tokio::mqtt_ep;

use futures_util::SinkExt;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_rustls::{TlsAcceptor, rustls};

/// Test TCP server that accepts connections and sends a simple response
async fn run_tcp_test_server(addr: &str, shutdown_rx: oneshot::Receiver<()>) -> SocketAddr {
    let listener = TcpListener::bind(addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx;
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((mut stream, _)) => {
                            tokio::spawn(async move {
                                let mut buffer = [0; 1024];
                                match stream.read(&mut buffer).await {
                                    Ok(n) if n > 0 => {
                                        let _ = stream.write_all(b"Hello from TCP server").await;
                                    }
                                    _ => {}
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    actual_addr
}

/// Load TLS configuration for the test server
fn load_tls_acceptor() -> TlsAcceptor {
    let cert_file = File::open("tests/certs/server.crt.pem").unwrap();
    let mut cert_reader = BufReader::new(cert_file);
    let cert_chain = rustls_pemfile::certs(&mut cert_reader)
        .unwrap()
        .into_iter()
        .map(rustls::Certificate)
        .collect();

    let key_file = File::open("tests/certs/server.key.pem").unwrap();
    let mut key_reader = BufReader::new(key_file);

    // Try PKCS8 first, then PKCS1
    let private_keys = rustls_pemfile::pkcs8_private_keys(&mut key_reader).unwrap();
    let private_key = if private_keys.is_empty() {
        // Reset reader and try PKCS1
        key_reader = BufReader::new(File::open("tests/certs/server.key.pem").unwrap());
        let rsa_keys = rustls_pemfile::rsa_private_keys(&mut key_reader).unwrap();
        rustls::PrivateKey(rsa_keys[0].clone())
    } else {
        rustls::PrivateKey(private_keys[0].clone())
    };

    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .unwrap();

    TlsAcceptor::from(Arc::new(config))
}

/// Test TLS server that accepts connections and sends a simple response
async fn run_tls_test_server(addr: &str, shutdown_rx: oneshot::Receiver<()>) -> SocketAddr {
    let listener = TcpListener::bind(addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();
    let acceptor = load_tls_acceptor();

    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx;
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let acceptor = acceptor.clone();
                            tokio::spawn(async move {
                                if let Ok(mut tls_stream) = acceptor.accept(stream).await {
                                    let mut buffer = [0; 1024];
                                    if let Ok(n) = tls_stream.read(&mut buffer).await {
                                        if n > 0 {
                                            let _ = tls_stream.write_all(b"Hello from TLS server").await;
                                        }
                                    }
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    actual_addr
}

/// Test WebSocket server that accepts connections with MQTT subprotocol
async fn run_ws_test_server(addr: &str, shutdown_rx: oneshot::Receiver<()>) -> SocketAddr {
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    let listener = TcpListener::bind(addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx;
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            tokio::spawn(async move {
                                let callback = |req: &Request, mut response: Response| {
                                    // Check if client requests MQTT subprotocol
                                    if let Some(protocols) = req.headers().get("Sec-WebSocket-Protocol") {
                                        if protocols.to_str().unwrap_or("").contains("mqtt") {
                                            // Accept MQTT subprotocol
                                            response.headers_mut().insert(
                                                "Sec-WebSocket-Protocol",
                                                HeaderValue::from_static("mqtt")
                                            );
                                        }
                                    }
                                    Ok(response)
                                };

                                if let Ok(mut ws_stream) = tokio_tungstenite::accept_hdr_async(stream, callback).await {
                                    use tokio_tungstenite::tungstenite::Message;
                                    // Send binary message as MQTT over WebSocket typically uses binary
                                    let binary_data = b"Hello from WebSocket server".to_vec();
                                    let _ = ws_stream.send(Message::Binary(binary_data)).await;
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    actual_addr
}

/// Test TLS WebSocket server that accepts connections with MQTT subprotocol
async fn run_tls_ws_test_server(addr: &str, shutdown_rx: oneshot::Receiver<()>) -> SocketAddr {
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    let listener = TcpListener::bind(addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();
    let acceptor = load_tls_acceptor();

    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx;
        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let acceptor = acceptor.clone();
                            tokio::spawn(async move {
                                if let Ok(tls_stream) = acceptor.accept(stream).await {
                                    let callback = |req: &Request, mut response: Response| {
                                        // Check if client requests MQTT subprotocol
                                        if let Some(protocols) = req.headers().get("Sec-WebSocket-Protocol") {
                                            if protocols.to_str().unwrap_or("").contains("mqtt") {
                                                // Accept MQTT subprotocol
                                                response.headers_mut().insert(
                                                    "Sec-WebSocket-Protocol",
                                                    HeaderValue::from_static("mqtt")
                                                );
                                            }
                                        }
                                        Ok(response)
                                    };

                                    if let Ok(mut ws_stream) = tokio_tungstenite::accept_hdr_async(tls_stream, callback).await {
                                        use tokio_tungstenite::tungstenite::Message;
                                        // Send binary message as MQTT over WebSocket typically uses binary
                                        let binary_data = b"Hello from TLS WebSocket server".to_vec();
                                        let _ = ws_stream.send(Message::Binary(binary_data)).await;
                                    }
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    actual_addr
}

/// Load client TLS configuration with custom CA
fn load_client_tls_config() -> Arc<rustls::ClientConfig> {
    let ca_file = File::open("tests/certs/cacert.pem").unwrap();
    let mut ca_reader = BufReader::new(ca_file);
    let ca_certs = rustls_pemfile::certs(&mut ca_reader).unwrap();

    let mut root_store = rustls::RootCertStore::empty();
    for cert_der in ca_certs {
        let cert = rustls::Certificate(cert_der);
        root_store.add(&cert).unwrap();
    }

    Arc::new(
        rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth(),
    )
}

/// Test-specific connect function that allows overriding the connect address for certificate hostname mismatch
async fn connect_tcp_tls_ws_for_test(
    connect_addr: &str,
    tls_hostname: &str,
    path: &str,
    tls_config: Option<Arc<rustls::ClientConfig>>,
    headers: Option<HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
    mqtt_ep::transport::TransportError,
> {
    use mqtt_ep::transport::TransportError;
    use tokio_tungstenite::tungstenite::http::Request;

    // 1. Establish TLS connection using connect address and hostname
    let tls_stream = mqtt_ep::transport::connect_helper::connect_tcp_tls(
        connect_addr,
        tls_hostname,
        tls_config,
        timeout,
    )
    .await?;

    // 2. Create WebSocket request using the hostname (not the IP)
    let url = format!("wss://{tls_hostname}{path}");
    let mut request_builder = Request::builder()
        .uri(&url)
        .header("Host", tls_hostname)
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

#[tokio::test]
async fn test_connect_tcp() {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let addr = run_tcp_test_server("127.0.0.1:0", shutdown_rx).await;

    // Test successful connection without timeout
    let result = mqtt_ep::transport::connect_helper::connect_tcp(&addr.to_string(), None).await;
    assert!(result.is_ok());

    // Test successful connection with timeout
    let result = mqtt_ep::transport::connect_helper::connect_tcp(
        &addr.to_string(),
        Some(Duration::from_secs(5)),
    )
    .await;
    assert!(result.is_ok());

    // Test connection to invalid address
    let result = mqtt_ep::transport::connect_helper::connect_tcp("invalid_address", None).await;
    assert!(result.is_err());

    // Test timeout with unreachable address
    let result = mqtt_ep::transport::connect_helper::connect_tcp(
        "192.0.2.1:12345", // TEST-NET-1 address (unreachable)
        Some(Duration::from_millis(100)),
    )
    .await;
    assert!(result.is_err());

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_connect_tcp_tls() {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let addr = run_tls_test_server("127.0.0.1:0", shutdown_rx).await;
    let tls_config = load_client_tls_config();

    // Test successful TLS connection without timeout
    let result = mqtt_ep::transport::connect_helper::connect_tcp_tls(
        &addr.to_string(),
        "localhost",
        Some(tls_config.clone()),
        None,
    )
    .await;
    assert!(result.is_ok());

    // Test successful TLS connection with timeout
    let result = mqtt_ep::transport::connect_helper::connect_tcp_tls(
        &addr.to_string(),
        "localhost",
        Some(tls_config.clone()),
        Some(Duration::from_secs(10)),
    )
    .await;
    assert!(result.is_ok());

    // Test connection to invalid address
    let result = mqtt_ep::transport::connect_helper::connect_tcp_tls(
        "invalid_address",
        "localhost",
        Some(tls_config.clone()),
        None,
    )
    .await;
    assert!(result.is_err());

    // Test timeout with unreachable address
    let result = mqtt_ep::transport::connect_helper::connect_tcp_tls(
        "192.0.2.1:12345", // TEST-NET-1 address (unreachable)
        "localhost",
        Some(tls_config),
        Some(Duration::from_millis(100)),
    )
    .await;
    assert!(result.is_err());

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_connect_tcp_ws() {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let addr = run_ws_test_server("127.0.0.1:0", shutdown_rx).await;

    // Test successful WebSocket connection without timeout
    let result =
        mqtt_ep::transport::connect_helper::connect_tcp_ws(&addr.to_string(), "/mqtt", None, None)
            .await;
    assert!(result.is_ok());

    // Test successful WebSocket connection with timeout
    let result = mqtt_ep::transport::connect_helper::connect_tcp_ws(
        &addr.to_string(),
        "/mqtt",
        None,
        Some(Duration::from_secs(10)),
    )
    .await;
    assert!(result.is_ok());

    // Test WebSocket connection with custom headers
    let mut headers = HashMap::new();
    headers.insert("X-Custom-Header".to_string(), "TestValue".to_string());
    let result = mqtt_ep::transport::connect_helper::connect_tcp_ws(
        &addr.to_string(),
        "/mqtt",
        Some(headers),
        Some(Duration::from_secs(10)),
    )
    .await;
    assert!(result.is_ok());

    // Test connection to invalid address
    let result =
        mqtt_ep::transport::connect_helper::connect_tcp_ws("invalid_address", "/mqtt", None, None)
            .await;
    assert!(result.is_err());

    // Test timeout with unreachable address
    let unreachable_addr = "192.0.2.1:12345";
    let result = mqtt_ep::transport::connect_helper::connect_tcp_ws(
        unreachable_addr,
        "/mqtt",
        None,
        Some(Duration::from_millis(100)),
    )
    .await;
    assert!(result.is_err());

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_connect_tcp_tls_ws() {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let addr = run_tls_ws_test_server("127.0.0.1:0", shutdown_rx).await;
    let tls_config = load_client_tls_config();

    // Test successful TLS WebSocket connection without timeout
    let result = connect_tcp_tls_ws_for_test(
        &addr.to_string(),
        "localhost",
        "/mqtt",
        Some(tls_config.clone()),
        None,
        None,
    )
    .await;
    assert!(result.is_ok());

    // Test successful TLS WebSocket connection with timeout
    let result = connect_tcp_tls_ws_for_test(
        &addr.to_string(),
        "localhost",
        "/mqtt",
        Some(tls_config.clone()),
        None,
        Some(Duration::from_secs(10)),
    )
    .await;
    assert!(result.is_ok());

    // Test TLS WebSocket connection with custom headers
    let mut headers = HashMap::new();
    headers.insert("X-Custom-Header".to_string(), "TestValue".to_string());
    let result = connect_tcp_tls_ws_for_test(
        &addr.to_string(),
        "localhost",
        "/mqtt",
        Some(tls_config.clone()),
        Some(headers),
        Some(Duration::from_secs(10)),
    )
    .await;
    assert!(result.is_ok());

    // Test connection to invalid address
    let result = mqtt_ep::transport::connect_helper::connect_tcp_tls_ws(
        "invalid_address",
        "localhost",
        "/mqtt",
        Some(tls_config.clone()),
        None,
        None,
    )
    .await;
    assert!(result.is_err());

    // Test timeout with unreachable address
    let unreachable_addr = "192.0.2.1:12345";
    let result = mqtt_ep::transport::connect_helper::connect_tcp_tls_ws(
        unreachable_addr,
        "192.0.2.1",
        "/mqtt",
        Some(tls_config),
        None,
        Some(Duration::from_millis(100)),
    )
    .await;
    assert!(result.is_err());

    let _ = shutdown_tx.send(());
}
