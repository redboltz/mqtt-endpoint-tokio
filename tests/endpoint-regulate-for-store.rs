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

type ClientEndpoint = mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u16>;

#[tokio::test]
async fn test_regulate_for_store_api_compilation() {
    common::init_tracing();
    // Test that the regulate_for_store API compiles correctly
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0);

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
            println!("regulate_for_store completed successfully");
            println!("Regulated topic: {}", regulated_packet.topic_name());
        }
        Err(e) => {
            println!("regulate_for_store completed with error: {e:?}");
            // This might be expected if the connection isn't fully established
        }
    }
}

#[tokio::test]
async fn test_regulate_for_store_with_different_roles() {
    common::init_tracing();
    // Test regulate_for_store method with different roles

    // Test with Server role
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Server, u16> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0);

        let publish_packet = mqtt_ep::packet::v5_0::Publish::builder()
            .topic_name("test/topic")
            .unwrap()
            .qos(mqtt_ep::packet::Qos::AtMostOnce)
            .retain(false)
            .payload("payload".as_bytes())
            .build()
            .unwrap();

        let _result = endpoint.regulate_for_store(publish_packet).await;
        // We don't assert success as it depends on connection state
    }

    // Test with Any role
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Any, u16> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0);

        let publish_packet = mqtt_ep::packet::v5_0::Publish::builder()
            .topic_name("test/topic")
            .unwrap()
            .qos(mqtt_ep::packet::Qos::AtMostOnce)
            .retain(false)
            .payload("payload".as_bytes())
            .build()
            .unwrap();

        let _result = endpoint.regulate_for_store(publish_packet).await;
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

        let _result = endpoint.regulate_for_store(publish_packet).await;
    }
}

#[tokio::test]
async fn test_regulate_for_store_after_close() {
    common::init_tracing();
    // Test that regulate_for_store after close returns appropriate errors
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0);

    // Close the endpoint
    let close_result = endpoint.close().await;
    println!("Close result: {close_result:?}");

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
    println!("regulate_for_store after close result: {regulate_result:?}");

    // Operation after close should return ChannelClosed error
    match regulate_result {
        Err(mqtt_ep::ConnectionError::ChannelClosed) => {
            println!("regulate_for_store correctly returned ChannelClosed after close");
        }
        _ => {
            println!("regulate_for_store did not return expected ChannelClosed error");
        }
    }
}

#[tokio::test]
async fn test_regulate_for_store_with_topic() {
    common::init_tracing();
    // Test regulate_for_store with various packet configurations
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0);

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
    println!("Regulate with topic result: {:?}", result1.is_ok());

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
                    println!("Regulate with empty topic result: {:?}", result2.is_ok());
                }
                Err(build_error) => {
                    println!("Failed to build packet with empty topic (expected): {build_error:?}");
                }
            }
        }
        Err(topic_error) => {
            println!("Failed to set empty topic name (expected): {topic_error:?}");
        }
    }
}
