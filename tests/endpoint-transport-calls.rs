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

use stub_transport::{StubTransport, TransportCall, TransportResponse};

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

#[tokio::test]
async fn test_attach_accepts_connected_transport() {
    common::init_tracing();
    let stub = StubTransport::new();
    // No connect response needed - transport is already "connected"

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Attach should accept already connected transport
    let result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(result.is_ok(), "Attach should succeed");

    // Verify no connect was called (transport was already connected)
    let calls = stub.get_calls();
    assert_eq!(
        calls.len(),
        0,
        "No transport methods should be called during attach"
    );
}

#[tokio::test]
async fn test_attach_with_options_accepts_connected_transport() {
    common::init_tracing();
    let stub = StubTransport::new();
    // No connect response needed - transport is already "connected"

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Attach should accept already connected transport
    let result = endpoint
        .attach_with_options(
            stub.clone(),
            mqtt_ep::Mode::Client,
            mqtt_ep::ConnectionOption::builder()
                .connection_establish_timeout_ms(5000u64)
                .recv_buffer_size(65536usize)
                .build()
                .unwrap(),
        )
        .await;
    assert!(result.is_ok(), "Attach should succeed");

    // Verify no connect was called (transport was already connected)
    let calls = stub.get_calls();
    assert_eq!(
        calls.len(),
        0,
        "No transport methods should be called during attach"
    );
}

#[tokio::test]
async fn test_attach_with_options_accepts_timeout() {
    common::init_tracing();

    let mut stub = StubTransport::new();
    // Add a delay longer than the timeout to ensure timeout occurs first
    stub.add_response(TransportResponse::DelayMs(100));
    stub.add_response(TransportResponse::RecvOk(vec![0x20, 0x02, 0x00, 0x00])); // CONNACK

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    let result = endpoint
        .attach_with_options(
            stub.clone(),
            mqtt_ep::Mode::Client,
            mqtt_ep::ConnectionOption::builder()
                .connection_establish_timeout_ms(50u64) // Shorter than DelayMs
                .build()
                .unwrap(),
        )
        .await;

    assert!(result.is_ok(), "Attach should succeed");

    let recv_result = endpoint.recv().await;
    match recv_result {
        Err(mqtt_ep::ConnectionError::Transport(mqtt_ep::TransportError::Timeout)) => {
            // Expected - timeout error should be returned
        }
        other => {
            panic!("Expected TransportError::Timeout, got: {:?}", other);
        }
    }
}

#[tokio::test]
async fn test_attach_with_options_accepts_timeout_cancel() {
    common::init_tracing();

    let mut stub = StubTransport::new();
    stub.add_response(TransportResponse::RecvOk(vec![0x20, 0x02, 0x00, 0x00])); // CONNACK

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    let result = endpoint
        .attach_with_options(
            stub.clone(),
            mqtt_ep::Mode::Client,
            mqtt_ep::ConnectionOption::builder()
                .connection_establish_timeout_ms(1000u64) // Long timeout that should be cancelled
                .build()
                .unwrap(),
        )
        .await;

    assert!(result.is_ok(), "Attach should succeed");

    // CONNACK should be received immediately without timeout
    let recv_result = endpoint.recv().await;
    assert!(
        recv_result.is_ok(),
        "Expected successful CONNACK reception, got error: {:?}",
        recv_result.unwrap_err()
    );

    // Verify that transport recv was called
    let calls = stub.get_calls();
    assert!(calls.len() > 0, "Should have made transport calls");

    let has_recv = calls
        .iter()
        .any(|call| matches!(call, TransportCall::Recv { .. }));
    assert!(has_recv, "Should have made recv call to transport");
}

// Note: Connection failure handling is now moved to connect_helper functions
// This test is no longer relevant as attach() expects already connected transports

#[tokio::test]
async fn test_recv_calls_transport_recv() {
    common::init_tracing();
    let mut stub = StubTransport::new();

    // Setup responses
    // Add a MQTT CONNACK packet as response (simplified)
    let connack_packet = vec![0x20, 0x02, 0x00, 0x00]; // CONNACK with success
    stub.add_response(TransportResponse::RecvOk(connack_packet));

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Attach first
    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    // Try to receive a packet (with timeout to avoid hanging)
    let recv_future = endpoint.recv_filtered(mqtt_ep::PacketFilter::Any);
    let _result = timeout(Duration::from_millis(100), recv_future).await;

    // Check the calls made to transport
    let calls = stub.get_calls();

    // Should have at least one recv call
    assert!(calls.len() >= 1, "Should have recv calls");

    // Check if there's a recv call
    let has_recv = calls
        .iter()
        .any(|call| matches!(call, TransportCall::Recv { .. }));
    assert!(has_recv, "Should have made recv call to transport");
}

#[tokio::test]
async fn test_close_calls_shutdown() {
    common::init_tracing();
    let mut stub = StubTransport::new();
    stub.add_response(TransportResponse::Shutdown);

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Attach first
    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    // Close the connection
    let close_result = endpoint.close().await;
    assert!(close_result.is_ok(), "Close should succeed");

    // Verify shutdown was called
    let calls = stub.get_calls();

    assert!(calls.len() >= 1, "Should have shutdown calls");

    // Check if there's a shutdown call
    let has_shutdown = calls
        .iter()
        .any(|call| matches!(call, TransportCall::Shutdown { .. }));
    assert!(has_shutdown, "Should have made shutdown call to transport");
}

#[tokio::test]
async fn test_multiple_recv_attempts_for_unmatched_packets() {
    common::init_tracing();
    let mut stub = StubTransport::new();

    // Setup responses

    // Add multiple different packets that don't match our filter
    let ping_packet = vec![0xC0, 0x00]; // PINGREQ
    let pingresp_packet = vec![0xD0, 0x00]; // PINGRESP
    let connack_packet = vec![0x20, 0x02, 0x00, 0x00]; // CONNACK (what we want)

    stub.add_response(TransportResponse::RecvOk(ping_packet));
    stub.add_response(TransportResponse::RecvOk(pingresp_packet));
    stub.add_response(TransportResponse::RecvOk(connack_packet));

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Attach first
    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    // Try to receive only CONNACK packets
    let recv_future = endpoint.recv_filtered(mqtt_ep::PacketFilter::include(vec![
        mqtt_ep::packet::PacketType::Connack,
    ]));
    let _result = timeout(Duration::from_millis(500), recv_future).await;

    // Check the calls made to transport
    let calls = stub.get_calls();

    // Should have multiple recv calls as it tries to find matching packet
    let recv_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, TransportCall::Recv { .. }))
        .collect();
    assert!(
        recv_calls.len() >= 2,
        "Should have made multiple recv calls to find matching packet"
    );
}

#[tokio::test]
async fn test_connection_establish_timeout_triggers() {
    common::init_tracing();

    let mut stub = StubTransport::new();
    // Add a delay longer than the timeout to ensure timeout occurs first
    stub.add_response(TransportResponse::DelayMs(150));
    stub.add_response(TransportResponse::RecvOk(vec![0x20, 0x02, 0x00, 0x00])); // CONNACK

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    let result = endpoint
        .attach_with_options(
            stub.clone(),
            mqtt_ep::Mode::Client,
            mqtt_ep::ConnectionOption::builder()
                .connection_establish_timeout_ms(100u64) // Shorter than DelayMs
                .recv_buffer_size(1024usize)
                .build()
                .unwrap(),
        )
        .await;

    assert!(result.is_ok(), "Attach should succeed");

    let recv_result = endpoint.recv().await;
    match recv_result {
        Err(mqtt_ep::ConnectionError::Transport(mqtt_ep::TransportError::Timeout)) => {
            // Expected - timeout error should be returned
        }
        other => {
            panic!("Expected TransportError::Timeout, got: {:?}", other);
        }
    }

    let calls = stub.get_calls();
    assert!(calls.len() > 0, "Should have made transport calls");
}
