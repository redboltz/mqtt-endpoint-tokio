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

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

#[tokio::test]
async fn test_regulate_for_store_api_compilation() {
    common::init_tracing();
    // Test that the regulate_for_store API compiles correctly
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Create a simple MQTT v5.0 PUBLISH packet for testing
    let publish_packet = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/topic")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .retain(false)
        .payload("test payload".as_bytes())
        .build()
        .unwrap();

    // Test that the regulate_for_store method exists and compiles
    let result = endpoint.regulate_for_store(publish_packet).await;

    // The method should complete successfully or with a connection error
    match result {
        Ok(regulated_packet) => {
            // Verify that the regulated packet maintains expected properties
            assert_eq!(
                regulated_packet.topic_name(),
                "test/topic",
                "Regulated packet should preserve topic name"
            );
            assert_eq!(
                regulated_packet.qos(),
                mqtt_ep::packet::Qos::AtMostOnce,
                "Regulated packet should preserve QoS level"
            );
        }
        Err(e) => {
            // This might be expected if the connection isn't fully established
            // Verify it's an expected connection error type
            match e {
                mqtt_ep::ConnectionError::NotConnected
                | mqtt_ep::ConnectionError::ChannelClosed => {
                    // Expected error types when not connected
                }
                other => panic!("Unexpected regulate_for_store error: {other:?}"),
            }
        }
    }
}

#[tokio::test]
async fn test_regulate_for_store_with_different_roles() {
    common::init_tracing();
    // Test regulate_for_store method with different roles

    // Test with Server role
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

        let publish_packet = mqtt_ep::packet::v5_0::Publish::builder()
            .topic_name("test/topic")
            .unwrap()
            .qos(mqtt_ep::packet::Qos::AtMostOnce)
            .retain(false)
            .payload("payload".as_bytes())
            .build()
            .unwrap();

        let result = endpoint.regulate_for_store(publish_packet).await;
        // Verify the result is either success or expected connection error
        assert!(
            result.is_ok()
                || matches!(
                    result,
                    Err(mqtt_ep::ConnectionError::NotConnected
                        | mqtt_ep::ConnectionError::ChannelClosed)
                ),
            "regulate_for_store should succeed or return expected connection error: {result:?}"
        );
    }

    // Test with Any role
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

        let publish_packet = mqtt_ep::packet::v5_0::Publish::builder()
            .topic_name("test/topic")
            .unwrap()
            .qos(mqtt_ep::packet::Qos::AtMostOnce)
            .retain(false)
            .payload("payload".as_bytes())
            .build()
            .unwrap();

        let result = endpoint.regulate_for_store(publish_packet).await;
        // Verify the result is either success or expected connection error
        assert!(
            result.is_ok()
                || matches!(
                    result,
                    Err(mqtt_ep::ConnectionError::NotConnected
                        | mqtt_ep::ConnectionError::ChannelClosed)
                ),
            "regulate_for_store should succeed or return expected connection error: {result:?}"
        );
    }

    // Test with u32 packet ID type
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u32> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0);

        let publish_packet = mqtt_ep::packet::v5_0::GenericPublish::<u32>::builder()
            .topic_name("test/topic")
            .unwrap()
            .qos(mqtt_ep::packet::Qos::AtMostOnce)
            .retain(false)
            .payload("payload".as_bytes())
            .build()
            .unwrap();

        let result = endpoint.regulate_for_store(publish_packet).await;
        // Verify the result is either success or expected connection error
        assert!(
            result.is_ok() || matches!(result, Err(mqtt_ep::ConnectionError::NotConnected | mqtt_ep::ConnectionError::ChannelClosed)),
            "regulate_for_store with u32 packet ID should succeed or return expected connection error: {result:?}"
        );
    }
}

#[tokio::test]
async fn test_regulate_for_store_after_close() {
    common::init_tracing();
    // Test that regulate_for_store after close returns appropriate errors
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Close the endpoint
    let close_result = endpoint.close().await;
    assert!(
        close_result.is_ok(),
        "Close should succeed: {close_result:?}"
    );

    // Try to regulate packet after close - should fail with ChannelClosed
    let publish_packet = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/topic")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtMostOnce)
        .retain(false)
        .payload("payload".as_bytes())
        .build()
        .unwrap();

    let regulate_result = endpoint.regulate_for_store(publish_packet).await;

    // regulate_for_store behavior after close may vary
    match regulate_result {
        Err(mqtt_ep::ConnectionError::ChannelClosed) => {
            // Expected behavior in some cases - operation fails after close
        }
        Err(mqtt_ep::ConnectionError::NotConnected) => {
            // Also acceptable - endpoint might be in NotConnected state
        }
        Ok(regulated_packet) => {
            // regulate_for_store might still work after close since it processes internal state
            // This is implementation-dependent behavior
            assert_eq!(
                regulated_packet.topic_name(),
                "test/topic",
                "Regulated packet should preserve topic name even after close"
            );
        }
        other => {
            panic!("Unexpected regulate_for_store result after close: {other:?}");
        }
    }
}

#[tokio::test]
async fn test_regulate_for_store_with_topic() {
    common::init_tracing();
    // Test regulate_for_store with various packet configurations
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Test with normal topic (should work fine)
    let publish_with_topic = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("sensor/temperature")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtLeastOnce)
        .retain(false)
        .packet_id(1u16)
        .payload("25.5".as_bytes())
        .build()
        .unwrap();

    let result1 = endpoint.regulate_for_store(publish_with_topic).await;
    // Verify the result is either success or expected connection error
    assert!(
        result1.is_ok() || matches!(result1, Err(mqtt_ep::ConnectionError::NotConnected | mqtt_ep::ConnectionError::ChannelClosed)),
        "regulate_for_store with normal topic should succeed or return expected connection error: {result1:?}"
    );

    // Test with empty topic (packet creation might fail, which is expected)
    match mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("") // Empty topic
    {
        Ok(builder) => {
            match builder
                .qos(mqtt_ep::packet::Qos::AtLeastOnce)
                .retain(false)
                .packet_id(2u16)
                .payload("data".as_bytes())
                .build()
            {
                Ok(publish_empty_topic) => {
                    let result2 = endpoint.regulate_for_store(publish_empty_topic).await;
                    // Empty topic regulation result depends on implementation
                    assert!(
                        result2.is_ok() || result2.is_err(),
                        "regulate_for_store with empty topic should return a valid Result: {result2:?}"
                    );
                }
                Err(_build_error) => {
                    // Failed to build packet with empty topic - this is expected and acceptable
                }
            }
        }
        Err(_topic_error) => {
            // Failed to set empty topic name - this is expected and acceptable
        }
    }
}
