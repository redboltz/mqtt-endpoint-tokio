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

#![cfg(all(feature = "unix-socket", unix))]

use mqtt_endpoint_tokio::mqtt_ep;

mod common;

use tokio::net::{UnixListener, UnixStream};

#[tokio::test]
async fn test_unix_socket_client_server_v311() {
    common::init_tracing();

    let socket_path = "/tmp/mqtt-test-v311.sock";
    // Clean up any existing socket file
    let _ = std::fs::remove_file(socket_path);

    // Create Unix listener
    let listener = UnixListener::bind(socket_path).unwrap();

    // Server task
    let server_handle = tokio::spawn(async move {
        let (unix_stream, _) = listener.accept().await.unwrap();
        let transport = mqtt_ep::transport::UnixStreamTransport::from_stream(unix_stream);

        // Create server endpoint
        let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
            mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
        server_endpoint
            .attach(transport, mqtt_ep::Mode::Server)
            .await
            .unwrap();

        // Receive CONNECT packet
        let packet = server_endpoint.recv().await.unwrap();
        assert!(matches!(packet, mqtt_ep::packet::Packet::V3_1_1Connect(_)));

        // Send CONNACK packet
        let connack = mqtt_ep::packet::v3_1_1::Connack::builder()
            .session_present(false)
            .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
            .build()
            .unwrap();
        server_endpoint.send(connack).await.unwrap();

        // Receive DISCONNECT packet
        let packet = server_endpoint.recv().await.unwrap();
        assert!(matches!(
            packet,
            mqtt_ep::packet::Packet::V3_1_1Disconnect(_)
        ));

        // Close endpoint
        let _ = server_endpoint.close().await;
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Client task
    let client_stream = UnixStream::connect(socket_path).await.unwrap();
    let transport = mqtt_ep::transport::UnixStreamTransport::from_stream(client_stream);

    // Create client endpoint
    let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);
    client_endpoint
        .attach(transport, mqtt_ep::Mode::Client)
        .await
        .unwrap();

    // Send CONNECT packet
    let connect = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("unix_client")
        .unwrap()
        .keep_alive(60)
        .clean_session(true)
        .build()
        .unwrap();
    client_endpoint.send(connect).await.unwrap();

    // Receive CONNACK packet
    let packet = client_endpoint.recv().await.unwrap();
    assert!(matches!(packet, mqtt_ep::packet::Packet::V3_1_1Connack(_)));

    // Send DISCONNECT packet
    let disconnect = mqtt_ep::packet::v3_1_1::Disconnect::new();
    client_endpoint.send(disconnect).await.unwrap();

    // Close endpoint
    let _ = client_endpoint.close().await;

    // Wait for server to finish
    server_handle.await.unwrap();

    // Clean up socket file
    let _ = std::fs::remove_file(socket_path);
}

#[tokio::test]
async fn test_unix_socket_connect_helper() {
    common::init_tracing();

    let socket_path = "/tmp/mqtt-test-helper.sock";
    // Clean up any existing socket file
    let _ = std::fs::remove_file(socket_path);

    // Create Unix listener
    let listener = UnixListener::bind(socket_path).unwrap();

    // Server task
    let server_handle = tokio::spawn(async move {
        let (unix_stream, _) = listener.accept().await.unwrap();
        let transport = mqtt_ep::transport::UnixStreamTransport::from_stream(unix_stream);

        // Create server endpoint
        let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
            mqtt_ep::Endpoint::new(mqtt_ep::Version::V5_0);
        server_endpoint
            .attach(transport, mqtt_ep::Mode::Server)
            .await
            .unwrap();

        // Receive CONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Send CONNACK packet
        let connack = mqtt_ep::packet::v5_0::Connack::builder()
            .session_present(false)
            .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
            .build()
            .unwrap();
        server_endpoint.send(connack).await.unwrap();

        // Receive DISCONNECT packet
        let _packet = server_endpoint.recv().await.unwrap();

        // Close endpoint
        let _ = server_endpoint.close().await;
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Client task using connect helper
    let unix_stream = mqtt_ep::transport::connect_helper::connect_unix(
        socket_path,
        Some(tokio::time::Duration::from_secs(5)),
    )
    .await
    .unwrap();
    let transport = mqtt_ep::transport::UnixStreamTransport::from_stream(unix_stream);

    // Create client endpoint
    let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V5_0);
    client_endpoint
        .attach(transport, mqtt_ep::Mode::Client)
        .await
        .unwrap();

    // Send CONNECT packet
    let connect = mqtt_ep::packet::v5_0::Connect::builder()
        .client_id("unix_client_v5")
        .unwrap()
        .keep_alive(60)
        .clean_start(true)
        .build()
        .unwrap();
    client_endpoint.send(connect).await.unwrap();

    // Receive CONNACK packet
    let packet = client_endpoint.recv().await.unwrap();
    assert!(matches!(packet, mqtt_ep::packet::Packet::V5_0Connack(_)));

    // Send DISCONNECT packet
    let disconnect = mqtt_ep::packet::v5_0::Disconnect::builder()
        .reason_code(mqtt_ep::result_code::DisconnectReasonCode::NormalDisconnection)
        .build()
        .unwrap();
    client_endpoint.send(disconnect).await.unwrap();

    // Close endpoint
    let _ = client_endpoint.close().await;

    // Wait for server to finish
    server_handle.await.unwrap();

    // Clean up socket file
    let _ = std::fs::remove_file(socket_path);
}
