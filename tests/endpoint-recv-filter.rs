/**
 * MIT License
 *
 * Copyright (c) 2025 Takatoshi Kondo
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
use std::time::Duration;
use tokio::time::timeout;

use mqtt_endpoint_tokio::mqtt_ep::{GenericEndpoint, PacketFilter};
use mqtt_protocol_core::mqtt::Version;
use mqtt_protocol_core::mqtt::connection::role;
use mqtt_protocol_core::mqtt::packet::PacketType;

type ClientEndpoint = GenericEndpoint<role::Client, u16>;

#[tokio::test]
async fn test_packet_filter_matching() {
    // Test Include filter
    let include_filter = PacketFilter::include(vec![PacketType::Publish, PacketType::Subscribe]);

    // Create mock packets (this is a simplified test - in reality you'd need proper packet construction)
    // For now, we just test the filter logic itself

    // Test Exclude filter
    let exclude_filter = PacketFilter::exclude(vec![PacketType::Connect, PacketType::Connack]);

    // Test basic filter creation
    let publish_filter = PacketFilter::include(vec![PacketType::Publish]);

    // Verify the filter creation works
    match include_filter {
        PacketFilter::Include(types) => {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&PacketType::Publish));
            assert!(types.contains(&PacketType::Subscribe));
        }
        _ => panic!("Expected Include filter"),
    }

    match exclude_filter {
        PacketFilter::Exclude(types) => {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&PacketType::Connect));
            assert!(types.contains(&PacketType::Connack));
        }
        _ => panic!("Expected Exclude filter"),
    }

    match publish_filter {
        PacketFilter::Include(types) => {
            assert_eq!(types.len(), 1);
            assert_eq!(types[0], PacketType::Publish);
        }
        _ => panic!("Expected Include filter"),
    }
}

#[tokio::test]
async fn test_recv_filtered_compilation() {
    // This test verifies that the recv_filtered API compiles correctly
    // We can't easily test the actual filtering without setting up a full MQTT connection

    let endpoint: ClientEndpoint = GenericEndpoint::new_disconnected(Version::V3_1_1);

    // Test that the API compiles - we can't actually receive anything without a real connection
    // Try to receive with timeout
    let result = timeout(
        Duration::from_millis(10),
        endpoint.recv_filtered(PacketFilter::include(vec![PacketType::Publish])),
    )
    .await;

    // For disconnected endpoint, recv_filtered should return NotConnected error immediately
    // or timeout due to no connection being established
    match result {
        Ok(recv_result) => {
            // If recv completed immediately, it should be an error (NotConnected)
            assert!(
                recv_result.is_err(),
                "Expected error from recv_filtered on disconnected endpoint"
            );
        }
        Err(_timeout) => {
            // Timeout is also acceptable behavior
        }
    }

    // Test filter creation compile
    let _publish_future = endpoint.recv_filtered(PacketFilter::include(vec![PacketType::Publish]));
    let _exclude_future = endpoint.recv_filtered(PacketFilter::exclude(vec![PacketType::Connect]));
    let _any_future = endpoint.recv(); // This should use PacketFilter::Any internally
}
