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

use mqtt_endpoint_tokio::mqtt_ep;

mod common;

use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tokio_tungstenite::{accept_async, client_async};

async fn tcp_client_server_scenario() {
    common::init_tracing();

    // Create TCP listener
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    // Server task
    let server_handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let transport = mqtt_ep::transport::TcpTransport::from_stream(tcp_stream);

        // Create server endpoint
        let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
            mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
        server_endpoint
            .attach(transport, mqtt_ep::Mode::Server)
            .await
            .unwrap();

        // Receive CONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Send CONNACK packet
        let connack = mqtt_ep::packet::v3_1_1::Connack::builder()
            .session_present(false)
            .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
            .build()
            .unwrap();
        server_endpoint.send(connack).await.unwrap();

        // Receive DISCONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Close endpoint
        let _ = server_endpoint.close().await;
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client task
    let client_stream = TcpStream::connect(server_addr).await.unwrap();
    let transport = mqtt_ep::transport::TcpTransport::from_stream(client_stream);

    // Create client endpoint
    let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
    client_endpoint
        .attach(transport, mqtt_ep::Mode::Client)
        .await
        .unwrap();

    // Send CONNECT packet
    let connect = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .clean_session(true)
        .keep_alive(60)
        .build()
        .unwrap();
    client_endpoint.send(connect).await.unwrap();

    // Receive CONNACK packet
    let _packet = client_endpoint.recv().await.unwrap();

    // Send DISCONNECT packet
    let disconnect = mqtt_ep::packet::v3_1_1::Disconnect::builder()
        .build()
        .unwrap();
    client_endpoint.send(disconnect).await.unwrap();

    // Close endpoint
    let _ = client_endpoint.close().await;

    // Wait for server to complete
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_tcp_transport_client_server_v311() {
    tcp_client_server_scenario().await;
}

#[tokio::test]
async fn test_tcp_transport_from_stream() {
    common::init_tracing();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let transport = mqtt_ep::transport::TcpTransport::from_stream(stream);
        assert!(format!("{:?}", transport).contains("TcpTransport"));
    });

    let stream = TcpStream::connect(server_addr).await.unwrap();
    let transport = mqtt_ep::transport::TcpTransport::from_stream(stream);
    assert!(format!("{:?}", transport).contains("TcpTransport"));

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_tcp_transport_stream_access() {
    common::init_tracing();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut transport = mqtt_ep::transport::TcpTransport::from_stream(stream);
        let _stream_mut = transport.stream_mut();
        let _stream_ref = transport.stream();
    });

    let _stream = TcpStream::connect(server_addr).await.unwrap();

    server_handle.await.unwrap();
}

async fn load_tls_config() -> Result<(TlsConnector, TlsAcceptor), Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::BufReader;

    // Load certificates from tests/certs directory
    let cert_file = File::open("tests/certs/server.crt")?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<rustls::Certificate> = rustls_pemfile::certs(&mut cert_reader)?
        .into_iter()
        .map(rustls::Certificate)
        .collect();

    let key_file = File::open("tests/certs/server.key")?;
    let mut key_reader = BufReader::new(key_file);
    let mut keys = rustls_pemfile::pkcs8_private_keys(&mut key_reader)?;
    if keys.is_empty() {
        let key_file = File::open("tests/certs/server.key")?;
        let mut key_reader = BufReader::new(key_file);
        keys = rustls_pemfile::rsa_private_keys(&mut key_reader)?;
    }
    let key = rustls::PrivateKey(keys.into_iter().next().unwrap());

    // Server config
    let server_config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs.clone(), key)?;

    // Client config (accepting self-signed certs for testing)
    let mut root_cert_store = rustls::RootCertStore::empty();
    for cert in certs {
        root_cert_store.add(&cert)?;
    }

    let client_config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    let tls_connector = TlsConnector::from(Arc::new(client_config));
    let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

    Ok((tls_connector, tls_acceptor))
}

async fn create_test_tls_config() -> (TlsConnector, TlsAcceptor) {
    // Try to load from certs directory first, fallback to generating
    if let Ok((connector, acceptor)) = load_tls_config().await {
        (connector, acceptor)
    } else {
        // Generate self-signed certificate for testing
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = rustls::Certificate(cert.serialize_der().unwrap());
        let private_key = rustls::PrivateKey(cert.serialize_private_key_der());

        // Server config
        let server_config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], private_key)
            .unwrap();

        // Client config (accepting self-signed certs)
        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store.add(&cert_der).unwrap();

        let client_config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();

        let tls_connector = TlsConnector::from(Arc::new(client_config));
        let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

        (tls_connector, tls_acceptor)
    }
}

async fn tls_client_server_scenario() {
    common::init_tracing();

    let (tls_connector, tls_acceptor) = create_test_tls_config().await;

    // Create TCP listener
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    // Server task
    let server_handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let tls_stream = tls_acceptor.accept(tcp_stream).await.unwrap();
        let transport = mqtt_ep::transport::TlsTransport::from_stream(tls_stream);

        // Create server endpoint
        let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
            mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
        server_endpoint
            .attach(transport, mqtt_ep::Mode::Server)
            .await
            .unwrap();

        // Receive CONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Send CONNACK packet
        let connack = mqtt_ep::packet::v3_1_1::Connack::builder()
            .session_present(false)
            .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
            .build()
            .unwrap();
        server_endpoint.send(connack).await.unwrap();

        // Receive DISCONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Close endpoint
        let _ = server_endpoint.close().await;
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client task
    let tcp_stream = TcpStream::connect(server_addr).await.unwrap();
    let domain = rustls::ServerName::try_from("localhost").unwrap();
    let tls_stream = tls_connector.connect(domain, tcp_stream).await.unwrap();
    let transport = mqtt_ep::transport::TlsTransport::from_stream(tls_stream);

    // Create client endpoint
    let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
    client_endpoint
        .attach(transport, mqtt_ep::Mode::Client)
        .await
        .unwrap();

    // Send CONNECT packet
    let connect = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .clean_session(true)
        .keep_alive(60)
        .build()
        .unwrap();
    client_endpoint.send(connect).await.unwrap();

    // Receive CONNACK packet
    let _packet = client_endpoint.recv().await.unwrap();

    // Send DISCONNECT packet
    let disconnect = mqtt_ep::packet::v3_1_1::Disconnect::builder()
        .build()
        .unwrap();
    client_endpoint.send(disconnect).await.unwrap();

    // Close endpoint
    let _ = client_endpoint.close().await;

    // Wait for server to complete
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_tls_transport_client_server_v311() {
    tls_client_server_scenario().await;
}

#[tokio::test]
async fn test_tls_transport_from_stream() {
    common::init_tracing();

    let (tls_connector, tls_acceptor) = create_test_tls_config().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let tls_stream = tls_acceptor.accept(tcp_stream).await.unwrap();
        let transport = mqtt_ep::transport::TlsTransport::from_stream(tls_stream);
        assert!(format!("{:?}", transport).contains("TlsTransport"));
    });

    let tcp_stream = TcpStream::connect(server_addr).await.unwrap();
    let domain = rustls::ServerName::try_from("localhost").unwrap();
    let tls_stream = tls_connector.connect(domain, tcp_stream).await.unwrap();
    let transport = mqtt_ep::transport::TlsTransport::from_stream(tls_stream);
    assert!(format!("{:?}", transport).contains("TlsTransport"));

    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_tls_transport_stream_access() {
    common::init_tracing();

    let (tls_connector, tls_acceptor) = create_test_tls_config().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let tls_stream = tls_acceptor.accept(tcp_stream).await.unwrap();
        let mut transport = mqtt_ep::transport::TlsTransport::from_stream(tls_stream);

        // Test server-side stream access
        let _stream_mut = transport.stream_mut();
        let _stream_ref = transport.stream();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let tcp_stream = TcpStream::connect(server_addr).await.unwrap();
    let domain = rustls::ServerName::try_from("localhost").unwrap();
    let tls_stream = tls_connector.connect(domain, tcp_stream).await.unwrap();
    let mut transport = mqtt_ep::transport::TlsTransport::from_stream(tls_stream);

    // Test client-side stream access
    let _stream_mut = transport.stream_mut();
    let _stream_ref = transport.stream();

    server_handle.await.unwrap();
}

async fn websocket_client_server_scenario() {
    common::init_tracing();

    // Create TCP listener for WebSocket
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    // Server task
    let server_handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let ws_stream = accept_async(tcp_stream).await.unwrap();
        let transport = mqtt_ep::transport::WebSocketTransport::from_tcp_server_stream(ws_stream);

        // Create server endpoint
        let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
            mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
        server_endpoint
            .attach(transport, mqtt_ep::Mode::Server)
            .await
            .unwrap();

        // Receive CONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Send CONNACK packet
        let connack = mqtt_ep::packet::v3_1_1::Connack::builder()
            .session_present(false)
            .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
            .build()
            .unwrap();
        server_endpoint.send(connack).await.unwrap();

        // Receive DISCONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Close endpoint
        let _ = server_endpoint.close().await;
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client task
    let tcp_stream = TcpStream::connect(server_addr).await.unwrap();
    let url = format!("ws://127.0.0.1:{}", server_addr.port());
    let (ws_stream, _) = client_async(&url, tcp_stream).await.unwrap();
    let transport = mqtt_ep::transport::WebSocketTransport::from_tcp_client_stream(ws_stream);

    // Create client endpoint
    let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
    client_endpoint
        .attach(transport, mqtt_ep::Mode::Client)
        .await
        .unwrap();

    // Send CONNECT packet
    let connect = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .clean_session(true)
        .keep_alive(60)
        .build()
        .unwrap();
    client_endpoint.send(connect).await.unwrap();

    // Receive CONNACK packet
    let _packet = client_endpoint.recv().await.unwrap();

    // Send DISCONNECT packet
    let disconnect = mqtt_ep::packet::v3_1_1::Disconnect::builder()
        .build()
        .unwrap();
    client_endpoint.send(disconnect).await.unwrap();

    // Close endpoint
    let _ = client_endpoint.close().await;

    // Wait for server to complete
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_websocket_tcp_transport_client_server_v311() {
    websocket_client_server_scenario().await;
}

#[tokio::test]
async fn test_websocket_transport_creation() {
    common::init_tracing();

    // Create mock WebSocket streams for testing
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    // Server WebSocket
    let server_handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let ws_stream = accept_async(tcp_stream).await.unwrap();
        let transport = mqtt_ep::transport::WebSocketTransport::from_tcp_server_stream(ws_stream);
        // Just test that transport was created successfully - debug format may vary
        let debug_str = format!("{:?}", transport);
        assert!(!debug_str.is_empty());
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client WebSocket
    let tcp_stream = TcpStream::connect(server_addr).await.unwrap();
    let url = format!("ws://127.0.0.1:{}", server_addr.port());
    let (ws_stream, _) = client_async(&url, tcp_stream).await.unwrap();
    let transport = mqtt_ep::transport::WebSocketTransport::from_tcp_client_stream(ws_stream);
    // Just test that transport was created successfully - debug format may vary
    let debug_str = format!("{:?}", transport);
    assert!(!debug_str.is_empty());

    server_handle.await.unwrap();
}

async fn websocket_tls_client_server_scenario() {
    common::init_tracing();

    let (tls_connector, tls_acceptor) = create_test_tls_config().await;

    // Create TCP listener for WebSocket over TLS
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    // Server task
    let server_handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let tls_stream = tls_acceptor.accept(tcp_stream).await.unwrap();
        let ws_stream = accept_async(tls_stream).await.unwrap();
        let transport = mqtt_ep::transport::WebSocketTransport::from_tls_server_stream(ws_stream);

        // Create server endpoint
        let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
            mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
        server_endpoint
            .attach(transport, mqtt_ep::Mode::Server)
            .await
            .unwrap();

        // Receive CONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Send CONNACK packet
        let connack = mqtt_ep::packet::v3_1_1::Connack::builder()
            .session_present(false)
            .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
            .build()
            .unwrap();
        server_endpoint.send(connack).await.unwrap();

        // Receive DISCONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Close endpoint
        let _ = server_endpoint.close().await;
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client task
    let tcp_stream = TcpStream::connect(server_addr).await.unwrap();
    let domain = rustls::ServerName::try_from("localhost").unwrap();
    let tls_stream = tls_connector.connect(domain, tcp_stream).await.unwrap();
    let url = format!("wss://127.0.0.1:{}", server_addr.port());
    let (ws_stream, _) = client_async(&url, tls_stream).await.unwrap();
    let transport = mqtt_ep::transport::WebSocketTransport::from_tls_client_stream(ws_stream);

    // Create client endpoint
    let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
    client_endpoint
        .attach(transport, mqtt_ep::Mode::Client)
        .await
        .unwrap();

    // Send CONNECT packet
    let connect = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .clean_session(true)
        .keep_alive(60)
        .build()
        .unwrap();
    client_endpoint.send(connect).await.unwrap();

    // Receive CONNACK packet
    let _packet = client_endpoint.recv().await.unwrap();

    // Send DISCONNECT packet
    let disconnect = mqtt_ep::packet::v3_1_1::Disconnect::builder()
        .build()
        .unwrap();
    client_endpoint.send(disconnect).await.unwrap();

    // Close endpoint
    let _ = client_endpoint.close().await;

    // Wait for server to complete
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_websocket_tls_transport_client_server_v311() {
    websocket_tls_client_server_scenario().await;
}

#[tokio::test]
async fn test_websocket_adapter_new() {
    common::init_tracing();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    // Server WebSocket
    let server_handle = tokio::spawn(async move {
        let (tcp_stream, _) = listener.accept().await.unwrap();
        let ws_stream = accept_async(tcp_stream).await.unwrap();
        let adapter = mqtt_ep::transport::WebSocketAdapter::new(ws_stream);
        assert!(format!("{:?}", adapter).contains("WebSocketAdapter"));
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client WebSocket
    let tcp_stream = TcpStream::connect(server_addr).await.unwrap();
    let url = format!("ws://127.0.0.1:{}", server_addr.port());
    let (ws_stream, _) = client_async(&url, tcp_stream).await.unwrap();
    let adapter = mqtt_ep::transport::WebSocketAdapter::new(ws_stream);
    assert!(format!("{:?}", adapter).contains("WebSocketAdapter"));

    server_handle.await.unwrap();
}
