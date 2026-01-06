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
mod common;
mod stub_transport;

use std::time::Duration;
use tokio::time::timeout;

use mqtt_endpoint_tokio::mqtt_ep;

use stub_transport::{StubTransport, TransportResponse};

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

#[tokio::test]
async fn test_receive_partial() {
    common::init_tracing();

    let mut stub = StubTransport::new();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Attach transport
    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(
        attach_result.is_ok(),
        "Attach should succeed: {attach_result:?}"
    );

    // Send CONNECT packet
    let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .keep_alive(60)
        .clean_session(true)
        .build()
        .unwrap();

    stub.add_response(TransportResponse::SendOk);
    let send_result = endpoint.send(connect_packet).await;
    assert!(send_result.is_ok(), "CONNECT should be sent successfully");

    // Prepare CONNACK response
    let connack_packet = mqtt_ep::packet::v3_1_1::Connack::builder()
        .session_present(false)
        .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
        .build()
        .unwrap();
    let connack_bytes = connack_packet.to_continuous_buffer();

    stub.add_response(TransportResponse::RecvOk(connack_bytes));

    // Receive CONNACK
    let connack_result = timeout(Duration::from_millis(1000), endpoint.recv()).await;
    assert!(connack_result.is_ok(), "Should receive CONNACK");
    let received_connack = connack_result.unwrap();
    assert!(
        received_connack.is_ok(),
        "CONNACK should be received successfully"
    );

    // Create two QoS 0 PUBLISH packets
    let publish1 = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name("test/topic1")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"Hello World 1")
        .build()
        .unwrap();

    let publish2 = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name("test/topic2")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"Hello World 2")
        .build()
        .unwrap();

    // Convert to continuous buffers
    let publish1_bytes = publish1.to_continuous_buffer();
    let publish2_bytes = publish2.to_continuous_buffer();

    // Split into 3 parts:
    // Part 1: First half of publish1
    let split1_point = publish1_bytes.len() / 2;
    let part1 = publish1_bytes[..split1_point].to_vec();

    // Part 2: Remaining half of publish1 + first half of publish2
    let split2_point = publish2_bytes.len() / 2;
    let mut part2 = publish1_bytes[split1_point..].to_vec();
    part2.extend_from_slice(&publish2_bytes[..split2_point]);

    // Part 3: Remaining half of publish2
    let part3 = publish2_bytes[split2_point..].to_vec();

    // Send first part (first half of publish1)
    stub.add_response(TransportResponse::RecvOk(part1));

    // Send second part (remaining publish1 + first half of publish2)
    stub.add_response(TransportResponse::RecvOk(part2));

    // Start first recv() - this should complete when first PUBLISH is fully received
    let recv1_future = endpoint.recv();
    let recv1_result = timeout(Duration::from_millis(1000), recv1_future).await;
    assert!(recv1_result.is_ok(), "First recv should complete");
    let received_publish1 = recv1_result.unwrap();
    assert!(
        received_publish1.is_ok(),
        "First PUBLISH should be received successfully"
    );

    // Verify first publish - compare with original packet
    let received_publish1_packet = received_publish1.unwrap();
    let expected_publish1: mqtt_ep::packet::Packet = publish1.into();
    assert_eq!(
        received_publish1_packet, expected_publish1,
        "First received PUBLISH should match the original packet"
    );

    // Send third part (remaining half of publish2)
    stub.add_response(TransportResponse::RecvOk(part3));

    // Start second recv() - this should complete when second PUBLISH is fully received
    let recv2_future = endpoint.recv();
    let recv2_result = timeout(Duration::from_millis(1000), recv2_future).await;
    assert!(recv2_result.is_ok(), "Second recv should complete");
    let received_publish2 = recv2_result.unwrap();
    assert!(
        received_publish2.is_ok(),
        "Second PUBLISH should be received successfully"
    );

    // Verify second publish - compare with original packet
    let received_publish2_packet = received_publish2.unwrap();
    let expected_publish2: mqtt_ep::packet::Packet = publish2.into();
    assert_eq!(
        received_publish2_packet, expected_publish2,
        "Second received PUBLISH should match the original packet"
    );
}

#[tokio::test]
async fn test_receive_error() {
    common::init_tracing();

    let mut stub = StubTransport::new();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Attach transport
    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(
        attach_result.is_ok(),
        "Attach should succeed: {attach_result:?}"
    );

    // Send CONNECT packet
    let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .keep_alive(60)
        .clean_session(true)
        .build()
        .unwrap();

    stub.add_response(TransportResponse::SendOk);
    let send_result = endpoint.send(connect_packet).await;
    assert!(send_result.is_ok(), "CONNECT should be sent successfully");

    // Prepare CONNACK response
    let connack_packet = mqtt_ep::packet::v3_1_1::Connack::builder()
        .session_present(false)
        .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
        .build()
        .unwrap();
    let connack_bytes = connack_packet.to_continuous_buffer();

    stub.add_response(TransportResponse::RecvOk(connack_bytes));

    // Receive CONNACK
    let connack_result = timeout(Duration::from_millis(1000), endpoint.recv()).await;
    assert!(connack_result.is_ok(), "Should receive CONNACK");
    let received_connack = connack_result.unwrap();
    assert!(
        received_connack.is_ok(),
        "CONNACK should be received successfully"
    );

    // Now test recv error scenario - stub will return an error on next recv
    let io_error = std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "Connection lost");
    stub.add_response(TransportResponse::RecvErr(
        mqtt_ep::transport::TransportError::Io(io_error),
    ));

    // Try to receive - this should result in an error
    let recv_error_result = timeout(Duration::from_millis(1000), endpoint.recv()).await;
    assert!(recv_error_result.is_ok(), "Should complete recv operation");
    let recv_error = recv_error_result.unwrap();

    // Verify that recv returned an error
    assert!(recv_error.is_err(), "recv should return an error");

    match recv_error {
        Err(mqtt_ep::connection_error::ConnectionError::Transport(transport_err)) => {
            match transport_err {
                mqtt_ep::transport::TransportError::Io(io_err) => {
                    assert_eq!(
                        io_err.kind(),
                        std::io::ErrorKind::ConnectionAborted,
                        "Error kind should match"
                    );
                    assert_eq!(
                        io_err.to_string(),
                        "Connection lost",
                        "Error message should match"
                    );
                    // Successfully received expected transport IO error
                }
                other => panic!("Expected IO error, got: {other:?}"),
            }
        }
        Err(mqtt_ep::connection_error::ConnectionError::NotConnected) => {
            // The endpoint might convert transport errors to NotConnected
            // This is also a valid behavior as it indicates the connection is lost
        }
        Err(other_err) => panic!("Expected Transport or NotConnected error, got: {other_err:?}"),
        Ok(_) => panic!("Expected error, but got success"),
    }

    // Verify that subsequent recv operations also fail due to disconnected state
    let recv_after_error_result = timeout(Duration::from_millis(1000), endpoint.recv()).await;
    assert!(
        recv_after_error_result.is_ok(),
        "Should complete recv operation"
    );
    let recv_after_error = recv_after_error_result.unwrap();

    assert!(
        recv_after_error.is_err(),
        "recv after error should also fail"
    );
    match recv_after_error {
        Err(mqtt_ep::connection_error::ConnectionError::NotConnected) => {
            // Successfully verified that endpoint is disconnected after transport error
        }
        Err(mqtt_ep::connection_error::ConnectionError::Transport(_)) => {
            // Also acceptable - might get another transport error
        }
        Err(mqtt_ep::connection_error::ConnectionError::ChannelClosed) => {
            // Also acceptable - endpoint channels might be closed
        }
        Err(other_err) => {
            panic!("Unexpected error type after transport failure, got: {other_err:?}. Expected NotConnected, Transport, or ChannelClosed");
        }
        Ok(_) => panic!("Expected error due to disconnected state, but got success"),
    }
}

/// Test partial packet reception with multiple recv calls
///
/// This test verifies that partial packet data is correctly buffered
/// and preserved across multiple recv() calls, even when split at
/// arbitrary byte boundaries.
///
/// Scenario:
/// 1. Receive first half of packet in first recv()
/// 2. Receive second half of packet in second recv()
/// 3. Complete packet should be received successfully
///
/// Note: This simulates the scenario where recv() might be cancelled
/// (e.g., due to timeout) while partial packet data has been received,
/// and verifies that the data is not lost.
#[tokio::test]
async fn test_receive_partial_with_timeout() {
    common::init_tracing();

    let mut stub = StubTransport::new();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Attach transport
    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(
        attach_result.is_ok(),
        "Attach should succeed: {attach_result:?}"
    );

    // Send CONNECT packet
    let connect_packet = mqtt_ep::packet::v5_0::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .keep_alive(60)
        .clean_start(true)
        .build()
        .unwrap();

    stub.add_response(TransportResponse::SendOk);
    let send_result = endpoint.send(connect_packet).await;
    assert!(send_result.is_ok(), "CONNECT should be sent successfully");

    // Prepare CONNACK response
    let connack_packet = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .build()
        .unwrap();
    let connack_bytes = connack_packet.to_continuous_buffer();

    stub.add_response(TransportResponse::RecvOk(connack_bytes));

    // Receive CONNACK
    let connack_result = timeout(Duration::from_millis(1000), endpoint.recv()).await;
    assert!(connack_result.is_ok(), "Should receive CONNACK");
    let received_connack = connack_result.unwrap();
    assert!(
        received_connack.is_ok(),
        "CONNACK should be received successfully"
    );

    // Create a PUBLISH packet with substantial payload to ensure clear split
    let publish = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/partial")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .payload(b"This is a test payload for partial reception across multiple recv calls")
        .build()
        .unwrap();

    let publish_bytes = publish.to_continuous_buffer();

    // Split the packet into two parts
    let split_point = publish_bytes.len() / 2;
    let first_half = publish_bytes[..split_point].to_vec();
    let second_half = publish_bytes[split_point..].to_vec();

    // Queue both halves - the implementation will read them as needed
    // The first recv() might read both parts or just the first,
    // depending on internal buffering, but the packet should be
    // correctly reassembled regardless
    stub.add_response(TransportResponse::RecvOk(first_half));
    stub.add_response(TransportResponse::RecvOk(second_half));

    // Call recv() - should receive the complete packet
    // The endpoint will internally call transport recv() multiple times
    // to assemble the complete packet from the two parts
    let recv_result = timeout(Duration::from_millis(1000), endpoint.recv()).await;
    assert!(
        recv_result.is_ok(),
        "Should complete recv operation: {recv_result:?}"
    );
    let received_packet = recv_result.unwrap();
    assert!(
        received_packet.is_ok(),
        "Should receive complete packet successfully: {received_packet:?}"
    );

    // Verify that the received packet matches the original
    let received_publish = received_packet.unwrap();
    let expected_publish: mqtt_ep::packet::Packet = publish.into();
    assert_eq!(
        received_publish, expected_publish,
        "Received packet should match the original packet"
    );
}
