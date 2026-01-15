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

//! Tests for recv() cancellation during packet processing using test-delay feature.
//!
//! These tests verify that packets are not lost when recv() is cancelled
//! while the packet is being processed (after transport receive, before delivery).
//!
//! This file only compiles when the `test-delay` feature is enabled.

#![cfg(feature = "test-delay")]

use mqtt_endpoint_tokio::mqtt_ep;
use tokio::time::{timeout, Duration};

/// Test packet loss prevention when recv() is cancelled during packet processing.
///
/// Scenario:
/// 1. Complete handshake with delay=0
/// 2. Set delay=200ms on client endpoint
/// 3. Server sends packet
/// 4. Client calls recv() with 50ms timeout
/// 5. Packet arrives but delivery is delayed 200ms - timeout occurs
/// 6. Client calls recv() again - should receive the packet (not lost)
#[tokio::test]
async fn test_recv_cancel_during_processing_v5() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Use channel to coordinate timing
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    let client_task = tokio::spawn(async move {
        let client_stream =
            mqtt_ep::transport::connect_helper::connect_tcp(&addr.to_string(), None)
                .await
                .unwrap();
        let client_transport = mqtt_ep::transport::TcpTransport::from_stream(client_stream);
        let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
            mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V5_0);
        client_endpoint
            .attach(client_transport, mqtt_ep::endpoint::Mode::Client)
            .await
            .unwrap();

        // CONNECT/CONNACK handshake (no delay)
        let connect = mqtt_ep::packet::v5_0::Connect::builder()
            .client_id("delay-test-client")
            .unwrap()
            .keep_alive(60)
            .clean_start(true)
            .build()
            .unwrap();
        client_endpoint.send(connect).await.unwrap();

        let packet = client_endpoint.recv().await.unwrap();
        assert!(matches!(
            packet,
            mqtt_ep::packet::GenericPacket::V5_0Connack(_)
        ));

        // Wait for server to send PUBLISH
        rx.recv().await.unwrap();

        // Small sleep to ensure PUBLISH is on the wire
        tokio::time::sleep(Duration::from_millis(10)).await;

        // NOW set the delay (after handshake, before recv)
        client_endpoint.set_test_delay(200).await.unwrap();

        // Try to receive with short timeout (50ms)
        // Packet will arrive but test-delay (200ms) will cause timeout during processing
        let timeout_result = timeout(Duration::from_millis(50), client_endpoint.recv()).await;
        assert!(
            timeout_result.is_err(),
            "Should timeout because test-delay is 200ms but timeout is 50ms"
        );

        // The packet should NOT be lost - receive it now
        let packet = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V5_0Publish(publish) = packet {
            assert_eq!(publish.topic_name(), "test/delay");
            assert_eq!(publish.payload().as_slice(), b"delayed-delivery");
        } else {
            panic!("Expected PUBLISH packet, got {:?}", packet);
        }

        // Reset delay
        client_endpoint.set_test_delay(0).await.unwrap();

        client_endpoint
    });

    // Server side
    let (server_stream, _) = listener.accept().await.unwrap();
    let server_transport = mqtt_ep::transport::TcpTransport::from_stream(server_stream);
    let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
        mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V5_0);
    server_endpoint
        .attach(server_transport, mqtt_ep::endpoint::Mode::Server)
        .await
        .unwrap();

    // CONNECT
    let _connect = server_endpoint.recv().await.unwrap();

    // CONNACK
    let connack = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .build()
        .unwrap();
    server_endpoint.send(connack).await.unwrap();

    // Send PUBLISH
    let publish = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/delay")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"delayed-delivery")
        .build()
        .unwrap();
    server_endpoint.send(publish).await.unwrap();

    // Signal client that PUBLISH is sent
    tx.send(()).await.unwrap();

    // Wait for client to complete
    let _client_endpoint = client_task.await.unwrap();
}

/// Test with v3.1.1
#[tokio::test]
async fn test_recv_cancel_during_processing_v3() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    let client_task = tokio::spawn(async move {
        let client_stream =
            mqtt_ep::transport::connect_helper::connect_tcp(&addr.to_string(), None)
                .await
                .unwrap();
        let client_transport = mqtt_ep::transport::TcpTransport::from_stream(client_stream);
        let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
            mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V3_1_1);
        client_endpoint
            .attach(client_transport, mqtt_ep::endpoint::Mode::Client)
            .await
            .unwrap();

        // CONNECT/CONNACK (no delay)
        let connect = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("delay-test-client-v3")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();
        client_endpoint.send(connect).await.unwrap();

        let packet = client_endpoint.recv().await.unwrap();
        assert!(matches!(
            packet,
            mqtt_ep::packet::GenericPacket::V3_1_1Connack(_)
        ));

        // Wait for server to send PUBLISH
        rx.recv().await.unwrap();

        // Small sleep to ensure PUBLISH is on the wire
        tokio::time::sleep(Duration::from_millis(10)).await;

        // NOW set the delay
        client_endpoint.set_test_delay(200).await.unwrap();

        // Try to receive with short timeout
        let timeout_result = timeout(Duration::from_millis(50), client_endpoint.recv()).await;
        assert!(timeout_result.is_err(), "Should timeout");

        // Packet should not be lost
        let packet = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V3_1_1Publish(publish) = packet {
            assert_eq!(publish.topic_name(), "test/delay");
            assert_eq!(publish.payload().as_slice(), b"delayed-delivery-v3");
        } else {
            panic!("Expected PUBLISH packet, got {:?}", packet);
        }

        // Reset delay
        client_endpoint.set_test_delay(0).await.unwrap();

        client_endpoint
    });

    // Server side
    let (server_stream, _) = listener.accept().await.unwrap();
    let server_transport = mqtt_ep::transport::TcpTransport::from_stream(server_stream);
    let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
        mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V3_1_1);
    server_endpoint
        .attach(server_transport, mqtt_ep::endpoint::Mode::Server)
        .await
        .unwrap();

    let _connect = server_endpoint.recv().await.unwrap();

    let connack = mqtt_ep::packet::v3_1_1::Connack::builder()
        .session_present(false)
        .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
        .build()
        .unwrap();
    server_endpoint.send(connack).await.unwrap();

    // Send PUBLISH
    let publish = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name("test/delay")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"delayed-delivery-v3")
        .build()
        .unwrap();
    server_endpoint.send(publish).await.unwrap();

    // Signal client
    tx.send(()).await.unwrap();

    let _client_endpoint = client_task.await.unwrap();
}

/// Test multiple packets with cancellation during processing
#[tokio::test]
async fn test_multiple_packets_with_cancel_during_processing() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    let client_task = tokio::spawn(async move {
        let client_stream =
            mqtt_ep::transport::connect_helper::connect_tcp(&addr.to_string(), None)
                .await
                .unwrap();
        let client_transport = mqtt_ep::transport::TcpTransport::from_stream(client_stream);
        let client_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
            mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V5_0);
        client_endpoint
            .attach(client_transport, mqtt_ep::endpoint::Mode::Client)
            .await
            .unwrap();

        let connect = mqtt_ep::packet::v5_0::Connect::builder()
            .client_id("multi-packet-delay-client")
            .unwrap()
            .keep_alive(60)
            .clean_start(true)
            .build()
            .unwrap();
        client_endpoint.send(connect).await.unwrap();

        let _connack = client_endpoint.recv().await.unwrap();

        // Wait for server to send packets
        rx.recv().await.unwrap();

        // Small sleep to ensure packets are on the wire
        tokio::time::sleep(Duration::from_millis(10)).await;

        // NOW set the delay
        client_endpoint.set_test_delay(150).await.unwrap();

        // First attempt - timeout during processing of packet1
        let timeout_result = timeout(Duration::from_millis(50), client_endpoint.recv()).await;
        assert!(timeout_result.is_err(), "Should timeout on packet1");

        // Second attempt - should get packet1 (not lost)
        let packet1 = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V5_0Publish(publish) = packet1 {
            assert_eq!(publish.payload().as_slice(), b"packet1");
        } else {
            panic!("Expected packet1");
        }

        // Third attempt - should get packet2
        let packet2 = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V5_0Publish(publish) = packet2 {
            assert_eq!(publish.payload().as_slice(), b"packet2");
        } else {
            panic!("Expected packet2");
        }

        // Reset delay
        client_endpoint.set_test_delay(0).await.unwrap();

        client_endpoint
    });

    let (server_stream, _) = listener.accept().await.unwrap();
    let server_transport = mqtt_ep::transport::TcpTransport::from_stream(server_stream);
    let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
        mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V5_0);
    server_endpoint
        .attach(server_transport, mqtt_ep::endpoint::Mode::Server)
        .await
        .unwrap();

    let _connect = server_endpoint.recv().await.unwrap();

    let connack = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .build()
        .unwrap();
    server_endpoint.send(connack).await.unwrap();

    // Send two packets in sequence
    let publish1 = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/multi")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"packet1")
        .build()
        .unwrap();
    server_endpoint.send(publish1).await.unwrap();

    let publish2 = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/multi")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"packet2")
        .build()
        .unwrap();
    server_endpoint.send(publish2).await.unwrap();

    // Signal client
    tx.send(()).await.unwrap();

    let _client_endpoint = client_task.await.unwrap();
}
