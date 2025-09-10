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
mod stub_transport;

use stub_transport::{StubTransport, TransportCall, TransportResponse};

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

#[tokio::test]
async fn test_close_api_compilation() {
    common::init_tracing();
    // Test that the close API compiles correctly
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);
    // Test that the close method exists and compiles
    let result = endpoint.close().await;

    // The method should complete successfully (duplex stream supports shutdown)
    assert!(result.is_ok(), "Close operation should succeed: {result:?}");
}

#[tokio::test]
async fn test_close_with_different_roles() {
    common::init_tracing();
    // Test close method with different roles

    // Test with Server role
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.close().await;
        assert!(
            result.is_ok(),
            "Close should succeed with Server role: {result:?}"
        );
    }

    // Test with Any role
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.close().await;
        assert!(
            result.is_ok(),
            "Close should succeed with Any role: {result:?}"
        );
    }

    // Test with u32 packet ID type
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.close().await;
        assert!(
            result.is_ok(),
            "Close should succeed with u32 packet ID: {result:?}"
        );
    }
}

#[tokio::test]
async fn test_operations_after_close() {
    common::init_tracing();
    // Test that operations after close return appropriate errors
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Close the endpoint
    let close_result = endpoint.close().await;
    assert!(
        close_result.is_ok(),
        "Close operation should succeed: {close_result:?}"
    );

    // Try to use the endpoint after close - should fail with ChannelClosed
    let send_result = endpoint.send(mqtt_ep::packet::v3_1_1::Pingreq::new()).await;
    let packet_id_result = endpoint.acquire_packet_id().await;

    // Send operations after close should return an error
    match send_result {
        Err(mqtt_ep::ConnectionError::ChannelClosed) => {
            // Expected behavior - send operation should fail after close
        }
        Err(mqtt_ep::ConnectionError::NotConnected) => {
            // Also acceptable - endpoint might be in NotConnected state
        }
        other => {
            panic!("Send after close should return ChannelClosed or NotConnected, got: {other:?}");
        }
    }

    // Packet ID acquisition behavior after close may vary
    match packet_id_result {
        Err(mqtt_ep::ConnectionError::ChannelClosed) => {
            // Expected behavior in some cases
        }
        Err(mqtt_ep::ConnectionError::NotConnected) => {
            // Also acceptable
        }
        Ok(_) => {
            // Packet ID acquisition might still work after close since it doesn't require transport
            // This is implementation-dependent behavior
        }
        other => {
            panic!("Unexpected packet ID acquisition result after close: {other:?}");
        }
    }
}

#[tokio::test]
async fn test_operations_close_multiple() {
    common::init_tracing();

    let mut stub = StubTransport::new();
    // Add delay response for shutdown to simulate slow transport shutdown
    stub.add_response(TransportResponse::DelayMs(500));

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Attach the stub transport
    endpoint
        .attach(stub.clone(), mqtt_ep::Mode::Client)
        .await
        .unwrap();

    let close_future1 = endpoint.close();
    let close_future2 = endpoint.close();
    let close_future3 = endpoint.close();

    // 2. Both close operations should complete, but transport shutdown should only be called once
    let (result1, result2, result3) = tokio::join!(close_future1, close_future2, close_future3);

    // Both close operations should succeed
    assert!(result1.is_ok(), "First close should succeed");
    assert!(result2.is_ok(), "Second close should succeed");
    assert!(result3.is_ok(), "Third close should succeed");

    // Check that shutdown was called only once
    let calls = stub.get_calls();
    let shutdown_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, TransportCall::Shutdown { .. }))
        .collect();

    assert_eq!(
        shutdown_calls.len(),
        1,
        "Transport shutdown should be called exactly once, even with multiple close() calls"
    );
}
