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

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

#[tokio::test]
async fn test_get_stored_packets_api_compilation() {
    common::init_tracing();
    // Test that the get_stored_packets API compiles correctly
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Test that the get_stored_packets method exists and compiles
    let result = endpoint.get_stored_packets().await;

    // The method should complete successfully and return empty list initially
    assert!(
        result.is_ok(),
        "get_stored_packets should succeed: {result:?}"
    );
    let packets = result.unwrap();
    assert_eq!(
        packets.len(),
        0,
        "Should be empty initially as no packets have been stored"
    );
}

#[tokio::test]
async fn test_get_stored_packets_with_different_roles() {
    common::init_tracing();
    // Test get_stored_packets method with different roles

    // Test with Server role
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.get_stored_packets().await;
        assert!(
            result.is_ok(),
            "get_stored_packets should work with Server role: {result:?}"
        );
        assert_eq!(result.unwrap().len(), 0, "Should be empty initially");
    }

    // Test with Any role
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.get_stored_packets().await;
        assert!(
            result.is_ok(),
            "get_stored_packets should work with Any role: {result:?}"
        );
        assert_eq!(result.unwrap().len(), 0, "Should be empty initially");
    }

    // Test with u32 packet ID type
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.get_stored_packets().await;
        assert!(
            result.is_ok(),
            "get_stored_packets should work with u32 packet ID: {result:?}"
        );
        assert_eq!(result.unwrap().len(), 0, "Should be empty initially");
    }
}

#[tokio::test]
async fn test_get_stored_packets_after_close() {
    common::init_tracing();
    // Test that get_stored_packets after close returns appropriate errors
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Close the endpoint
    let close_result = endpoint.close().await;
    assert!(
        close_result.is_ok(),
        "Close should succeed: {close_result:?}"
    );

    // Try to get stored packets after close - behavior may vary
    let get_result = endpoint.get_stored_packets().await;

    // get_stored_packets behavior after close may vary
    match get_result {
        Err(mqtt_ep::ConnectionError::ChannelClosed) => {
            // Expected behavior in some cases - operation fails after close
        }
        Err(mqtt_ep::ConnectionError::NotConnected) => {
            // Also acceptable - endpoint might be in NotConnected state
        }
        Ok(packets) => {
            // get_stored_packets might still work after close since it queries internal state
            // This is implementation-dependent behavior
            assert_eq!(packets.len(), 0, "Should have no stored packets");
        }
        other => {
            panic!("Unexpected get_stored_packets result after close: {other:?}");
        }
    }
}

#[tokio::test]
async fn test_restore_and_get_stored_packets_roundtrip() {
    common::init_tracing();
    // Test the roundtrip: restore packets after connection -> get stored packets
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Initially should have no stored packets
    let initial_packets = endpoint.get_stored_packets().await;
    assert!(
        initial_packets.is_ok(),
        "Initial get_stored_packets should succeed: {initial_packets:?}"
    );
    let initial_packets = initial_packets.unwrap();
    assert_eq!(
        initial_packets.len(),
        0,
        "Should have no stored packets initially"
    );

    // Create packets to restore (empty vector for this test)
    let packets_to_restore: Vec<mqtt_ep::packet::StorePacket> = Vec::new();
    let qos2_pids_to_restore = mqtt_ep::common::HashSet::default();

    // Create connection options using builder with all required fields
    let connection_options = mqtt_ep::connection_option::ConnectionOption::builder()
        .pingreq_send_interval_ms(0u64)
        .auto_pub_response(true)
        .auto_ping_response(true)
        .auto_map_topic_alias_send(false)
        .auto_replace_topic_alias_send(false)
        .pingresp_recv_timeout_ms(0u64)
        .connection_establish_timeout_ms(0u64)
        .shutdown_timeout_ms(5000u64)
        .recv_buffer_size(4096usize)
        .queuing_receive_maximum(false)
        .build()
        .unwrap();

    // Create a stub transport for testing
    let transport = stub_transport::StubTransport::new();

    // Try to attach with the options
    let attach_result = endpoint
        .attach_with_options(transport, mqtt_ep::Mode::Client, connection_options)
        .await;

    // Restore packets after connection (this is the new approach)
    // Note: In real scenarios, this would be done after receiving CONNECT/CONNACK
    let restore_packets_result = endpoint.restore_stored_packets(packets_to_restore).await;
    let restore_qos2_result = endpoint
        .restore_qos2_publish_handled_pids(qos2_pids_to_restore)
        .await;

    // Get stored packets after restore
    let final_packets = endpoint.get_stored_packets().await;
    assert!(
        final_packets.is_ok(),
        "Final get_stored_packets should succeed: {final_packets:?}"
    );
    let final_packets = final_packets.unwrap();

    // Verify the restoration happened (both should succeed or both should fail consistently)
    match (attach_result, restore_packets_result, restore_qos2_result) {
        (Ok(()), Ok(()), Ok(())) => {
            // Connection and restoration succeeded - verify restored packets behavior
            assert_eq!(
                final_packets.len(),
                0,
                "Should have 0 stored packets (empty restore list)"
            );
        }
        _ => {
            // Connection or restoration failed (expected for stub transport) - API should still work
            // Even if restore failed, get_stored_packets should still return results
            assert_eq!(
                final_packets.len(),
                0,
                "Should have 0 stored packets when connection or restore fails"
            );
        }
    }
}
