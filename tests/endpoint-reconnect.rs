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

//! Reconnection tests for MQTT endpoint
//!
//! These tests verify that an endpoint can be successfully reconnected after
//! various types of disconnection events. The tests cover all paths that call
//! `Self::handle_close_event()`:
//!
//! 1. Connection close (recv returns 0) - normal connection termination
//! 2. Connection error (recv returns error) - transport error
//! 3. Connection establish timeout - timeout during connection setup
//!
//! Each test verifies that after the disconnection event, the endpoint can
//! be attached with a new transport and operate normally.

use mqtt_endpoint_tokio::mqtt_ep;
use std::time::Duration;

mod common;
mod stub_transport;

use stub_transport::{StubTransport, TransportResponse};

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;
type ServerEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Server>;

/// Helper function to create CONNACK bytes for V3.1.1
fn create_connack_v311_bytes() -> Vec<u8> {
    let connack_packet = mqtt_ep::packet::v3_1_1::Connack::builder()
        .session_present(false)
        .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
        .build()
        .unwrap();
    connack_packet.to_continuous_buffer()
}

/// Helper function to create CONNECT bytes for V3.1.1
fn create_connect_v311_bytes() -> Vec<u8> {
    let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .keep_alive(60)
        .clean_session(true)
        .build()
        .unwrap();
    connect_packet.to_continuous_buffer()
}

// =============================================================================
// Path 1: Reconnection after connection close (recv returns 0)
// =============================================================================

/// Test reconnection after connection is closed normally (recv returns 0 bytes)
///
/// This tests the path at line 1633 where handle_close_event is called
/// when transport.recv() returns Ok(0), indicating connection was closed
/// by the remote peer.
#[tokio::test]
async fn test_reconnect_after_connection_close_recv_zero() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // First connection: establish and then close normally
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        // Setup responses for first connection
        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response
        stub.add_response(TransportResponse::RecvOk(vec![])); // Connection close (0 bytes)
        stub.add_response(TransportResponse::Shutdown);

        // Attach and establish connection
        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("First attach should succeed");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send should succeed");

        // Receive CONNACK
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive CONNACK");
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully"
        );

        // Trigger recv that returns 0 bytes (connection close)
        // This will call handle_close_event()
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Recv should complete");
        // The result should be an error indicating connection closed
        match recv_result.unwrap() {
            Err(_) => {} // Expected: connection closed
            Ok(packet) => panic!("Expected error after connection close, got packet: {packet:?}"),
        }
    }

    // Second connection: reconnect with new transport
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        // Setup responses for reconnection
        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response

        // Attach new transport - this should work after handle_close_event()
        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("Reconnect attach should succeed");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client_reconnected")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send on reconnect should succeed");

        // Receive CONNACK to verify reconnection works
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(
            recv_result.is_ok(),
            "Should receive CONNACK on reconnection"
        );
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully on reconnection"
        );
    }
}

// =============================================================================
// Path 2: Reconnection after connection error (recv returns error)
// =============================================================================

/// Test reconnection after transport recv error
///
/// This tests the path at line 1642 where handle_close_event is called
/// when transport.recv() returns an error.
#[tokio::test]
async fn test_reconnect_after_recv_error() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // First connection: establish and then encounter recv error
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        // Setup responses for first connection
        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response
        stub.add_response(TransportResponse::RecvErr(
            mqtt_ep::TransportError::NotConnected,
        )); // Recv error
        stub.add_response(TransportResponse::Shutdown);

        // Attach and establish connection
        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("First attach should succeed");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send should succeed");

        // Receive CONNACK
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive CONNACK");
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully"
        );

        // Trigger recv error - this will call handle_close_event()
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Recv should complete (with error)");
        match recv_result.unwrap() {
            Err(_) => {} // Expected: transport error
            Ok(packet) => panic!("Expected error after recv error, got packet: {packet:?}"),
        }
    }

    // Second connection: reconnect with new transport
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        // Setup responses for reconnection
        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response

        // Attach new transport - this should work after handle_close_event()
        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("Reconnect attach should succeed after recv error");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client_reconnected")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send on reconnect should succeed");

        // Receive CONNACK to verify reconnection works
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(
            recv_result.is_ok(),
            "Should receive CONNACK on reconnection"
        );
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully on reconnection"
        );
    }
}

// =============================================================================
// Path 3: Reconnection after connection establish timeout
// =============================================================================

/// Test reconnection after connection establishment timeout
///
/// This tests the path at line 1663 where handle_close_event is called
/// when the connection establishment timer expires.
#[tokio::test]
async fn test_reconnect_after_connection_timeout() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // First connection: establish with a short timeout that will expire
    {
        let mut stub = StubTransport::new();

        // Setup responses: delay recv longer than timeout
        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::DelayMs(2000)); // Delay longer than timeout
        stub.add_response(TransportResponse::RecvOk(create_connack_v311_bytes()));
        stub.add_response(TransportResponse::Shutdown);

        // Attach with a short connection timeout
        let option = mqtt_ep::ConnectionOption::builder()
            .connection_establish_timeout_ms(100u64)
            .build()
            .unwrap();

        endpoint
            .attach_with_options(stub.clone(), mqtt_ep::Mode::Client, option)
            .await
            .expect("First attach should succeed");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send should succeed");

        // Wait for connection timeout - this will call handle_close_event()
        let recv_result = tokio::time::timeout(Duration::from_millis(500), endpoint.recv()).await;

        // The recv should complete with timeout error
        assert!(recv_result.is_ok(), "Recv should complete (with error)");
        match recv_result.unwrap() {
            Err(mqtt_ep::ConnectionError::Transport(mqtt_ep::TransportError::Timeout)) => {} // Expected timeout error
            Err(e) => {
                // Other errors may also be acceptable depending on timing
                eprintln!("Got error after connection timeout: {e:?}");
            }
            Ok(packet) => {
                panic!("Expected timeout error, got packet: {packet:?}")
            }
        }

        // Wait a bit to ensure handle_close_event has completed
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Second connection: reconnect with new transport
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        // Setup responses for reconnection
        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response

        // Attach new transport - this should work after handle_close_event()
        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("Reconnect attach should succeed after connection timeout");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client_reconnected")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send on reconnect should succeed");

        // Receive CONNACK to verify reconnection works
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(
            recv_result.is_ok(),
            "Should receive CONNACK on reconnection"
        );
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully on reconnection"
        );
    }
}

// =============================================================================
// Additional tests for edge cases
// =============================================================================

/// Test multiple consecutive reconnections
///
/// Verifies that an endpoint can be disconnected and reconnected multiple times.
#[tokio::test]
async fn test_multiple_consecutive_reconnections() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    for i in 0..3 {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        // Setup responses
        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response
        stub.add_response(TransportResponse::RecvOk(vec![])); // Connection close
        stub.add_response(TransportResponse::Shutdown);

        // Attach and establish connection
        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .unwrap_or_else(|_| panic!("Attach #{} should succeed", i + 1));

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id(format!("test_client_{}", i))
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .unwrap_or_else(|_| panic!("CONNECT send #{} should succeed", i + 1));

        // Receive CONNACK
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive CONNACK #{}", i + 1);
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK #{} should be received successfully",
            i + 1
        );

        // Trigger connection close
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Recv #{} should complete", i + 1);
        // Connection closed error is expected
        assert!(
            recv_result.unwrap().is_err(),
            "Connection #{} should close",
            i + 1
        );
    }
}

/// Test reconnection with Server role
///
/// Verifies that server endpoints can also be reconnected after disconnection.
#[tokio::test]
async fn test_reconnect_server_role() {
    common::init_tracing();

    let endpoint = ServerEndpoint::new(mqtt_ep::Version::V3_1_1);

    // First connection
    {
        let mut stub = StubTransport::new();
        let connect_bytes = create_connect_v311_bytes();

        // Setup responses for server
        stub.add_response(TransportResponse::RecvOk(connect_bytes)); // Receive CONNECT from client
        stub.add_response(TransportResponse::SendOk); // For CONNACK
        stub.add_response(TransportResponse::RecvOk(vec![])); // Connection close
        stub.add_response(TransportResponse::Shutdown);

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Server)
            .await
            .expect("First server attach should succeed");

        // Receive CONNECT
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive CONNECT");
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNECT should be received successfully"
        );

        // Send CONNACK
        let connack_packet = mqtt_ep::packet::v3_1_1::Connack::builder()
            .session_present(false)
            .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
            .build()
            .unwrap();
        endpoint
            .send(connack_packet)
            .await
            .expect("CONNACK send should succeed");

        // Trigger connection close
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Recv should complete");
        assert!(recv_result.unwrap().is_err(), "Connection should be closed");
    }

    // Reconnection
    {
        let mut stub = StubTransport::new();
        let connect_bytes = create_connect_v311_bytes();

        // Setup responses for reconnection
        stub.add_response(TransportResponse::RecvOk(connect_bytes)); // Receive CONNECT
        stub.add_response(TransportResponse::SendOk); // For CONNACK

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Server)
            .await
            .expect("Reconnect server attach should succeed");

        // Receive CONNECT to verify reconnection works
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(
            recv_result.is_ok(),
            "Should receive CONNECT on reconnection"
        );
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNECT should be received successfully on reconnection"
        );
    }
}

/// Test reconnection after explicit close()
///
/// Verifies that after calling close(), the endpoint can be reconnected.
/// Note: close() calls handle_close_request(), not handle_close_event(),
/// but this test ensures the endpoint is reusable after manual close.
#[tokio::test]
async fn test_reconnect_after_explicit_close() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // First connection and explicit close
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response
        stub.add_response(TransportResponse::Shutdown); // For close

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("First attach should succeed");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send should succeed");

        // Receive CONNACK
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive CONNACK");
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully"
        );

        // Explicitly close the endpoint
        endpoint.close().await.expect("Close should succeed");
    }

    // Reconnection after explicit close
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("Reconnect attach should succeed after explicit close");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client_reconnected")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send on reconnect should succeed");

        // Receive CONNACK to verify reconnection works
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(
            recv_result.is_ok(),
            "Should receive CONNACK on reconnection"
        );
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully on reconnection"
        );
    }
}

/// Test reconnection with V5.0 protocol
///
/// Verifies that reconnection works correctly with MQTT 5.0.
#[tokio::test]
async fn test_reconnect_v5() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // First connection
    {
        let mut stub = StubTransport::new();

        // Create V5.0 CONNACK
        let connack_packet = mqtt_ep::packet::v5_0::Connack::builder()
            .session_present(false)
            .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
            .build()
            .unwrap();
        let connack_bytes = connack_packet.to_continuous_buffer();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response
        stub.add_response(TransportResponse::RecvErr(
            mqtt_ep::TransportError::NotConnected,
        )); // Connection error
        stub.add_response(TransportResponse::Shutdown);

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("First attach should succeed");

        let connect_packet = mqtt_ep::packet::v5_0::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send should succeed");

        // Receive CONNACK
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive CONNACK");
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully"
        );

        // Trigger connection error
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Recv should complete");
        assert!(recv_result.unwrap().is_err(), "Connection should error");
    }

    // Reconnection
    {
        let mut stub = StubTransport::new();

        // Create V5.0 CONNACK
        let connack_packet = mqtt_ep::packet::v5_0::Connack::builder()
            .session_present(false)
            .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
            .build()
            .unwrap();
        let connack_bytes = connack_packet.to_continuous_buffer();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK response

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("Reconnect attach should succeed");

        let connect_packet = mqtt_ep::packet::v5_0::Connect::builder()
            .client_id("test_client_reconnected")
            .unwrap()
            .keep_alive(60)
            .build()
            .unwrap();

        endpoint
            .send(connect_packet)
            .await
            .expect("CONNECT send on reconnect should succeed");

        // Receive CONNACK to verify reconnection works
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(
            recv_result.is_ok(),
            "Should receive CONNACK on reconnection"
        );
        assert!(
            recv_result.unwrap().is_ok(),
            "CONNACK should be received successfully on reconnection"
        );
    }
}

/// Test that send operations work correctly after reconnection
///
/// Verifies that after reconnection, the endpoint can send various packet types.
#[tokio::test]
async fn test_send_operations_after_reconnect() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // First connection and disconnect
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK
        stub.add_response(TransportResponse::RecvOk(vec![])); // Connection close
        stub.add_response(TransportResponse::Shutdown);

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .unwrap();

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint.send(connect_packet).await.unwrap();

        // Receive CONNACK
        let _ = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;

        // Wait for connection close
        let _ = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
    }

    // Reconnect and test various send operations
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK
        stub.add_response(TransportResponse::SendOk); // For SUBSCRIBE
        stub.add_response(TransportResponse::SendOk); // For PUBLISH
        stub.add_response(TransportResponse::SendOk); // For PINGREQ

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("Reconnect should succeed");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint.send(connect_packet).await.unwrap();

        // Receive CONNACK
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok() && recv_result.unwrap().is_ok());

        // Test SUBSCRIBE
        let packet_id = endpoint.acquire_packet_id().await.unwrap();
        let sub_opts = mqtt_ep::packet::SubOpts::new().set_qos(mqtt_ep::packet::Qos::AtLeastOnce);
        let sub_entry = mqtt_ep::packet::SubEntry::new("test/topic", sub_opts).unwrap();
        let subscribe_packet = mqtt_ep::packet::v3_1_1::Subscribe::builder()
            .packet_id(packet_id)
            .entries(vec![sub_entry])
            .build()
            .unwrap();
        endpoint
            .send(subscribe_packet)
            .await
            .expect("SUBSCRIBE after reconnect should work");

        // Test PUBLISH QoS 0
        let publish_packet = mqtt_ep::packet::v3_1_1::Publish::builder()
            .topic_name("test/topic")
            .unwrap()
            .qos(mqtt_ep::packet::Qos::AtMostOnce)
            .payload(b"test payload".to_vec())
            .build()
            .unwrap();
        endpoint
            .send(publish_packet)
            .await
            .expect("PUBLISH after reconnect should work");

        // Test PINGREQ
        let pingreq = mqtt_ep::packet::v3_1_1::Pingreq::new();
        endpoint
            .send(pingreq)
            .await
            .expect("PINGREQ after reconnect should work");
    }
}

/// Test that receive operations work correctly after reconnection
///
/// Verifies that after reconnection, the endpoint can receive various packet types.
#[tokio::test]
async fn test_recv_operations_after_reconnect() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // First connection and disconnect
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK
        stub.add_response(TransportResponse::RecvErr(
            mqtt_ep::TransportError::NotConnected,
        )); // Error
        stub.add_response(TransportResponse::Shutdown);

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .unwrap();

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint.send(connect_packet).await.unwrap();

        // Receive CONNACK
        let _ = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;

        // Wait for error
        let _ = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
    }

    // Reconnect and test receive operations
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        // Create PUBLISH packet bytes
        let publish_packet = mqtt_ep::packet::v3_1_1::Publish::builder()
            .topic_name("test/topic")
            .unwrap()
            .qos(mqtt_ep::packet::Qos::AtMostOnce)
            .payload(b"test payload".to_vec())
            .build()
            .unwrap();
        let publish_bytes = publish_packet.to_continuous_buffer();

        // Create PINGRESP packet bytes
        let pingresp_packet = mqtt_ep::packet::v3_1_1::Pingresp::new();
        let pingresp_bytes = pingresp_packet.to_continuous_buffer();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK
        stub.add_response(TransportResponse::RecvOk(publish_bytes)); // PUBLISH
        stub.add_response(TransportResponse::RecvOk(pingresp_bytes)); // PINGRESP

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("Reconnect should succeed");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint.send(connect_packet).await.unwrap();

        // Receive CONNACK
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive CONNACK");
        let packet = recv_result.unwrap().unwrap();
        match packet {
            mqtt_ep::packet::GenericPacket::V3_1_1Connack(_) => {}
            _ => panic!("Expected CONNACK"),
        }

        // Receive PUBLISH
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive PUBLISH");
        let packet = recv_result.unwrap().unwrap();
        match packet {
            mqtt_ep::packet::GenericPacket::V3_1_1Publish(_) => {}
            _ => panic!("Expected PUBLISH, got {:?}", packet),
        }

        // Receive PINGRESP
        let recv_result = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
        assert!(recv_result.is_ok(), "Should receive PINGRESP");
        let packet = recv_result.unwrap().unwrap();
        match packet {
            mqtt_ep::packet::GenericPacket::V3_1_1Pingresp(_) => {}
            _ => panic!("Expected PINGRESP, got {:?}", packet),
        }
    }
}

/// Test packet ID management after reconnection
///
/// Verifies that packet ID allocation works correctly after reconnection.
#[tokio::test]
async fn test_packet_id_management_after_reconnect() {
    common::init_tracing();

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // First connection - acquire some packet IDs
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK
        stub.add_response(TransportResponse::RecvOk(vec![])); // Connection close
        stub.add_response(TransportResponse::Shutdown);

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .unwrap();

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint.send(connect_packet).await.unwrap();

        // Receive CONNACK
        let _ = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;

        // Acquire packet IDs
        let pid1 = endpoint.acquire_packet_id().await.unwrap();
        let pid2 = endpoint.acquire_packet_id().await.unwrap();
        assert_ne!(pid1, pid2, "Packet IDs should be unique");

        // Wait for connection close
        let _ = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;
    }

    // Reconnect and verify packet ID management still works
    {
        let mut stub = StubTransport::new();
        let connack_bytes = create_connack_v311_bytes();

        stub.add_response(TransportResponse::SendOk); // For CONNECT
        stub.add_response(TransportResponse::RecvOk(connack_bytes)); // CONNACK

        endpoint
            .attach(stub.clone(), mqtt_ep::Mode::Client)
            .await
            .expect("Reconnect should succeed");

        let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
            .client_id("test_client")
            .unwrap()
            .keep_alive(60)
            .clean_session(true)
            .build()
            .unwrap();

        endpoint.send(connect_packet).await.unwrap();

        // Receive CONNACK
        let _ = tokio::time::timeout(Duration::from_millis(100), endpoint.recv()).await;

        // Acquire new packet IDs after reconnection
        let pid3 = endpoint.acquire_packet_id().await.unwrap();
        let pid4 = endpoint.acquire_packet_id().await.unwrap();
        assert_ne!(pid3, pid4, "Packet IDs after reconnect should be unique");

        // Test registering a custom packet ID
        let custom_pid: u16 = 100;
        let register_result = endpoint.register_packet_id(custom_pid).await;
        assert!(
            register_result.is_ok(),
            "Registering packet ID after reconnect should work"
        );

        // Release packet ID
        let release_result = endpoint.release_packet_id(pid3).await;
        assert!(
            release_result.is_ok(),
            "Releasing packet ID after reconnect should work"
        );
    }
}
