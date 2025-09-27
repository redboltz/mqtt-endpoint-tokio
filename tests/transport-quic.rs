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

use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;

async fn create_test_quic_config() -> (ClientConfig, ServerConfig) {
    // Generate self-signed certificate for testing
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = CertificateDer::from(cert.serialize_der().unwrap());
    let private_key = PrivateKeyDer::try_from(cert.serialize_private_key_der()).unwrap();

    // Server config
    let rustls_server_config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], private_key)
            .unwrap(),
    );

    let server_config = ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(rustls_server_config).unwrap(),
    ));

    // Client config (accepting self-signed certs for testing)
    let mut root_cert_store = rustls::RootCertStore::empty();
    root_cert_store.add(cert_der).unwrap();

    let rustls_client_config = Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth(),
    );

    let client_config = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(rustls_client_config).unwrap(),
    ));

    (client_config, server_config)
}

async fn quic_client_server_scenario() {
    common::init_tracing();

    let (client_config, server_config) = create_test_quic_config().await;

    // Create QUIC server endpoint
    let server_endpoint = Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let server_addr = server_endpoint.local_addr().unwrap();

    // Server task
    let server_handle = tokio::spawn(async move {
        if let Some(incoming_conn) = server_endpoint.accept().await {
            let connection = incoming_conn.await.unwrap();
            let (send_stream, recv_stream) = connection.accept_bi().await.unwrap();
            let transport =
                mqtt_ep::transport::QuicTransport::from_streams(send_stream, recv_stream);

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
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Create QUIC client endpoint
    let mut client_endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
    client_endpoint.set_default_client_config(client_config);

    // Client task
    let connection = client_endpoint
        .connect(server_addr, "localhost")
        .unwrap()
        .await
        .unwrap();
    let (send_stream, recv_stream) = connection.open_bi().await.unwrap();
    let transport = mqtt_ep::transport::QuicTransport::from_streams(send_stream, recv_stream);

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
async fn test_quic_transport_client_server_v311() {
    quic_client_server_scenario().await;
}

#[tokio::test]
#[ignore = "QUIC test unstable in CI"]
async fn test_quic_transport_from_streams() {
    common::init_tracing();

    let (client_config, server_config) = create_test_quic_config().await;

    let server_endpoint = Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let server_addr = server_endpoint.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let timeout_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            server_endpoint.accept(),
        )
        .await;

        match timeout_result {
            Ok(Some(incoming_conn)) => match incoming_conn.await {
                Ok(connection) => match connection.accept_bi().await {
                    Ok((send_stream, recv_stream)) => {
                        let transport = mqtt_ep::transport::QuicTransport::from_streams(
                            send_stream,
                            recv_stream,
                        );
                        assert!(format!("{:?}", transport).contains("QuicTransport"));
                    }
                    Err(e) => panic!("Failed to accept bidirectional stream: {e}"),
                },
                Err(e) => panic!("Failed to establish connection: {e}"),
            },
            Ok(None) => panic!("No incoming connection"),
            Err(_) => panic!("Server accept timeout"),
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let mut client_endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
    client_endpoint.set_default_client_config(client_config);

    let connection_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        client_endpoint.connect(server_addr, "localhost").unwrap(),
    )
    .await;

    let connection = match connection_result {
        Ok(Ok(conn)) => conn,
        Ok(Err(e)) => panic!("QUIC connection failed: {e}"),
        Err(_) => panic!("QUIC connection timeout"),
    };
    let (send_stream, recv_stream) = connection.open_bi().await.unwrap();
    let transport = mqtt_ep::transport::QuicTransport::from_streams(send_stream, recv_stream);
    assert!(format!("{:?}", transport).contains("QuicTransport"));

    server_handle.await.unwrap();
}

#[tokio::test]
#[ignore = "QUIC test unstable in CI"]
async fn test_quic_transport_stream_access() {
    common::init_tracing();

    let (client_config, server_config) = create_test_quic_config().await;

    let server_endpoint = Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let server_addr = server_endpoint.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let timeout_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            server_endpoint.accept(),
        )
        .await;

        match timeout_result {
            Ok(Some(incoming_conn)) => {
                match incoming_conn.await {
                    Ok(connection) => {
                        match connection.accept_bi().await {
                            Ok((send_stream, recv_stream)) => {
                                let mut transport = mqtt_ep::transport::QuicTransport::from_streams(
                                    send_stream,
                                    recv_stream,
                                );

                                // Test server-side stream access
                                let _send_stream_mut = transport.send_stream_mut();
                                let _send_stream_ref = transport.send_stream();
                                let _recv_stream_mut = transport.recv_stream_mut();
                                let _recv_stream_ref = transport.recv_stream();
                            }
                            Err(e) => panic!("Failed to accept bidirectional stream: {e}"),
                        }
                    }
                    Err(e) => panic!("Failed to establish connection: {e}"),
                }
            }
            Ok(None) => panic!("No incoming connection"),
            Err(_) => panic!("Server accept timeout"),
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let mut client_endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
    client_endpoint.set_default_client_config(client_config);

    let connection_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        client_endpoint.connect(server_addr, "localhost").unwrap(),
    )
    .await;

    let connection = match connection_result {
        Ok(Ok(conn)) => conn,
        Ok(Err(e)) => panic!("QUIC connection failed: {e}"),
        Err(_) => panic!("QUIC connection timeout"),
    };
    let (send_stream, recv_stream) = connection.open_bi().await.unwrap();
    let mut transport = mqtt_ep::transport::QuicTransport::from_streams(send_stream, recv_stream);

    // Test client-side stream access
    let _send_stream_mut = transport.send_stream_mut();
    let _send_stream_ref = transport.send_stream();
    let _recv_stream_mut = transport.recv_stream_mut();
    let _recv_stream_ref = transport.recv_stream();

    server_handle.await.unwrap();
}
