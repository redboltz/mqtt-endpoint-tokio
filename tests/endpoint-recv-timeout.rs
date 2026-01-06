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
use tokio::time::{timeout, Duration};

/// Test that packets are not lost when recv() times out
///
/// Scenario:
/// 1. Client calls recv() with timeout
/// 2. Server sends packetA but client times out before receiving
/// 3. Client calls recv() again - should receive packetA (not lost)
/// 4. Server sends packetB
/// 5. Client calls recv() - should receive packetB
#[tokio::test]
async fn test_recv_timeout_no_packet_loss_v5() {
    // Create a pair of connected endpoints using TCP sockets
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Client connects
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

        // Send CONNECT
        let connect = mqtt_ep::packet::v5_0::Connect::builder()
            .client_id("timeout-test-client")
            .unwrap()
            .keep_alive(60)
            .clean_start(true)
            .build()
            .unwrap();
        client_endpoint.send(connect).await.unwrap();

        // Receive CONNACK
        let packet = client_endpoint.recv().await.unwrap();
        assert!(matches!(
            packet,
            mqtt_ep::packet::GenericPacket::V5_0Connack(_)
        ));

        // Subscribe to topic
        let packet_id = client_endpoint.acquire_packet_id().await.unwrap();
        let sub_opts = mqtt_ep::packet::SubOpts::new().set_qos(mqtt_ep::packet::Qos::AtMostOnce);
        let sub_entry = mqtt_ep::packet::SubEntry::new("test/timeout", sub_opts).unwrap();
        let subscribe = mqtt_ep::packet::v5_0::Subscribe::builder()
            .packet_id(packet_id)
            .entries(vec![sub_entry])
            .build()
            .unwrap();
        client_endpoint.send(subscribe).await.unwrap();

        // Receive SUBACK
        let packet = client_endpoint.recv().await.unwrap();
        assert!(matches!(
            packet,
            mqtt_ep::packet::GenericPacket::V5_0Suback(_)
        ));

        // Now test the timeout scenario
        // Try to receive with a very short timeout - should timeout
        let timeout_result = timeout(Duration::from_millis(100), client_endpoint.recv()).await;
        assert!(timeout_result.is_err(), "Should have timed out");

        // Now receive again without timeout - should get packetA that was sent during timeout
        let packet_a = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V5_0Publish(publish) = packet_a {
            assert_eq!(publish.topic_name(), "test/timeout");
            assert_eq!(publish.payload().as_slice(), b"packetA");
        } else {
            panic!("Expected PUBLISH packet A");
        }

        // Receive packetB
        let packet_b = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V5_0Publish(publish) = packet_b {
            assert_eq!(publish.topic_name(), "test/timeout");
            assert_eq!(publish.payload().as_slice(), b"packetB");
        } else {
            panic!("Expected PUBLISH packet B");
        }

        client_endpoint
    });

    // Server accepts connection
    let (server_stream, _) = listener.accept().await.unwrap();
    let server_transport = mqtt_ep::transport::TcpTransport::from_stream(server_stream);
    let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
        mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V5_0);
    server_endpoint
        .attach(server_transport, mqtt_ep::endpoint::Mode::Server)
        .await
        .unwrap();

    // Receive CONNECT
    let packet = server_endpoint.recv().await.unwrap();
    assert!(matches!(
        packet,
        mqtt_ep::packet::GenericPacket::V5_0Connect(_)
    ));

    // Send CONNACK
    let connack = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .build()
        .unwrap();
    server_endpoint.send(connack).await.unwrap();

    // Receive SUBSCRIBE
    let packet = server_endpoint.recv().await.unwrap();
    let subscribe_packet_id = if let mqtt_ep::packet::GenericPacket::V5_0Subscribe(ref sub) = packet
    {
        sub.packet_id()
    } else {
        panic!("Expected SUBSCRIBE packet");
    };

    // Send SUBACK
    let suback = mqtt_ep::packet::v5_0::Suback::builder()
        .packet_id(subscribe_packet_id)
        .reason_codes(vec![mqtt_ep::result_code::SubackReasonCode::GrantedQos0])
        .build()
        .unwrap();
    server_endpoint.send(suback).await.unwrap();

    // Wait for client to start timeout recv and then timeout (client uses 100ms timeout)
    // We wait 150ms to ensure client times out before we send the packet
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Send packetA AFTER client has timed out
    let publish_a = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/timeout")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"packetA")
        .build()
        .unwrap();
    server_endpoint.send(publish_a).await.unwrap();

    // Wait a bit for client to call recv() again and process packetA
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send packetB
    let publish_b = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/timeout")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"packetB")
        .build()
        .unwrap();
    server_endpoint.send(publish_b).await.unwrap();

    // Wait for client to complete
    let _client_endpoint = client_task.await.unwrap();
}

/// Test with v3.1.1
#[tokio::test]
async fn test_recv_timeout_no_packet_loss_v3() {
    // Create a pair of connected endpoints using TCP sockets
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Client connects
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

        // Send CONNECT
        let connect = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("timeout-test-client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();
        client_endpoint.send(connect).await.unwrap();

        // Receive CONNACK
        let packet = client_endpoint.recv().await.unwrap();
        assert!(matches!(
            packet,
            mqtt_ep::packet::GenericPacket::V3_1_1Connack(_)
        ));

        // Subscribe to topic
        let packet_id = client_endpoint.acquire_packet_id().await.unwrap();
        let sub_opts = mqtt_ep::packet::SubOpts::new().set_qos(mqtt_ep::packet::Qos::AtMostOnce);
        let sub_entry = mqtt_ep::packet::SubEntry::new("test/timeout", sub_opts).unwrap();
        let subscribe = mqtt_ep::packet::v3_1_1::Subscribe::builder()
            .packet_id(packet_id)
            .entries(vec![sub_entry])
            .build()
            .unwrap();
        client_endpoint.send(subscribe).await.unwrap();

        // Receive SUBACK
        let packet = client_endpoint.recv().await.unwrap();
        assert!(matches!(
            packet,
            mqtt_ep::packet::GenericPacket::V3_1_1Suback(_)
        ));

        // Now test the timeout scenario
        // Try to receive with a very short timeout - should timeout
        let timeout_result = timeout(Duration::from_millis(100), client_endpoint.recv()).await;
        assert!(timeout_result.is_err(), "Should have timed out");

        // Now receive again without timeout - should get packetA
        let packet_a = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V3_1_1Publish(publish) = packet_a {
            assert_eq!(publish.topic_name(), "test/timeout");
            assert_eq!(publish.payload().as_slice(), b"packetA");
        } else {
            panic!("Expected PUBLISH packet A");
        }

        // Receive packetB
        let packet_b = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V3_1_1Publish(publish) = packet_b {
            assert_eq!(publish.topic_name(), "test/timeout");
            assert_eq!(publish.payload().as_slice(), b"packetB");
        } else {
            panic!("Expected PUBLISH packet B");
        }

        client_endpoint
    });

    // Server accepts connection
    let (server_stream, _) = listener.accept().await.unwrap();
    let server_transport = mqtt_ep::transport::TcpTransport::from_stream(server_stream);
    let server_endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
        mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V3_1_1);
    server_endpoint
        .attach(server_transport, mqtt_ep::endpoint::Mode::Server)
        .await
        .unwrap();

    // Receive CONNECT
    let packet = server_endpoint.recv().await.unwrap();
    assert!(matches!(
        packet,
        mqtt_ep::packet::GenericPacket::V3_1_1Connect(_)
    ));

    // Send CONNACK
    let connack = mqtt_ep::packet::v3_1_1::Connack::builder()
        .session_present(false)
        .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
        .build()
        .unwrap();
    server_endpoint.send(connack).await.unwrap();

    // Receive SUBSCRIBE
    let packet = server_endpoint.recv().await.unwrap();
    let subscribe_packet_id =
        if let mqtt_ep::packet::GenericPacket::V3_1_1Subscribe(ref sub) = packet {
            sub.packet_id()
        } else {
            panic!("Expected SUBSCRIBE packet");
        };

    // Send SUBACK
    let suback = mqtt_ep::packet::v3_1_1::Suback::builder()
        .packet_id(subscribe_packet_id)
        .return_codes(vec![
            mqtt_ep::result_code::SubackReturnCode::SuccessMaximumQos0,
        ])
        .build()
        .unwrap();
    server_endpoint.send(suback).await.unwrap();

    // Wait for client to start timeout recv and then timeout (client uses 100ms timeout)
    // We wait 150ms to ensure client times out before we send the packet
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Send packetA AFTER client has timed out
    let publish_a = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name("test/timeout")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"packetA")
        .build()
        .unwrap();
    server_endpoint.send(publish_a).await.unwrap();

    // Wait a bit for client to call recv() again and process packetA
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send packetB
    let publish_b = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name("test/timeout")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"packetB")
        .build()
        .unwrap();
    server_endpoint.send(publish_b).await.unwrap();

    // Wait for client to complete
    let _client_endpoint = client_task.await.unwrap();
}

/// Test multiple consecutive timeouts
#[tokio::test]
async fn test_multiple_recv_timeouts_v5() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

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
            .client_id("multi-timeout-client")
            .unwrap()
            .keep_alive(60)
            .clean_start(true)
            .build()
            .unwrap();
        client_endpoint.send(connect).await.unwrap();

        let _connack = client_endpoint.recv().await.unwrap();

        // Timeout multiple times
        for _ in 0..3 {
            let timeout_result = timeout(Duration::from_millis(100), client_endpoint.recv()).await;
            assert!(timeout_result.is_err(), "Should timeout");
        }

        // Now receive the packet that was sent
        let packet = client_endpoint.recv().await.unwrap();
        if let mqtt_ep::packet::GenericPacket::V5_0Publish(publish) = packet {
            assert_eq!(publish.payload().as_slice(), b"delayed-packet");
        } else {
            panic!("Expected PUBLISH");
        }

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

    // Wait for client to timeout 3 times (each timeout is 100ms)
    // So we wait 350ms to ensure all 3 timeouts have occurred
    tokio::time::sleep(Duration::from_millis(350)).await;

    // Send packet AFTER client has timed out 3 times
    let publish = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/topic")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"delayed-packet")
        .build()
        .unwrap();
    server_endpoint.send(publish).await.unwrap();

    let _client_endpoint = client_task.await.unwrap();
}
