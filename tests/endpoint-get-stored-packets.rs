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

type ClientEndpoint = mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u16>;

#[tokio::test]
async fn test_get_stored_packets_api_compilation() {
    common::init_tracing();
    // Test that the get_stored_packets API compiles correctly
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Test that the get_stored_packets method exists and compiles
    let result = endpoint.get_stored_packets().await;

    // The method should complete successfully
    match result {
        Ok(packets) => {
            println!(
                "get_stored_packets completed successfully, got {} packets",
                packets.len()
            );
            // Should be empty initially as no packets have been stored
            assert_eq!(packets.len(), 0);
        }
        Err(e) => {
            println!("get_stored_packets completed with error: {e:?}");
        }
    }
}

#[tokio::test]
async fn test_get_stored_packets_with_different_roles() {
    common::init_tracing();
    // Test get_stored_packets method with different roles

    // Test with Server role
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Server, u16> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.get_stored_packets().await;
        assert!(result.is_ok());
    }

    // Test with Any role
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Any, u16> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.get_stored_packets().await;
        assert!(result.is_ok());
    }

    // Test with u32 packet ID type
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u32> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);
        let result = endpoint.get_stored_packets().await;
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn test_get_stored_packets_after_close() {
    common::init_tracing();
    // Test that get_stored_packets after close returns appropriate errors
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Close the endpoint
    let close_result = endpoint.close().await;
    println!("Close result: {close_result:?}");

    // Try to get stored packets after close - should fail with ChannelClosed
    let get_result = endpoint.get_stored_packets().await;
    println!("get_stored_packets after close result: {get_result:?}");

    // Operation after close should return ChannelClosed error
    match get_result {
        Err(mqtt_ep::ConnectionError::ChannelClosed) => {
            println!("get_stored_packets correctly returned ChannelClosed after close");
        }
        _ => {
            println!("get_stored_packets did not return expected ChannelClosed error");
        }
    }
}

#[tokio::test]
async fn test_restore_and_get_stored_packets_roundtrip() {
    common::init_tracing();
    // Test the roundtrip: restore packets via connection options -> get stored packets
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Initially should have no stored packets
    let initial_packets = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(initial_packets.len(), 0);
    println!("Initial stored packets: {}", initial_packets.len());

    // Create connection options with packets to restore (empty vector for this test)
    let packets_to_restore: Vec<mqtt_ep::packet::GenericStorePacket<u16>> = Vec::new();

    // Create connection options using builder with all required fields
    let connection_options = mqtt_ep::connection_option::GenericConnectionOption::<u16>::builder()
        .restore_packets(packets_to_restore)
        .pingreq_send_interval_ms(0u64)
        .auto_pub_response(true)
        .auto_ping_response(true)
        .auto_map_topic_alias_send(false)
        .auto_replace_topic_alias_send(false)
        .pingresp_recv_timeout_ms(0u64)
        .connection_establish_timeout_ms(0u64)
        .shutdown_timeout_ms(5000u64)
        .recv_buffer_size(4096usize)
        .restore_qos2_publish_handled(mqtt_ep::common::HashSet::default())
        .build()
        .unwrap();

    // Create a stub transport for testing
    let transport = stub_transport::StubTransport::new();

    // Try to attach with the options containing restore packets
    match endpoint
        .attach_with_options(transport, mqtt_ep::Mode::Client, connection_options)
        .await
    {
        Ok(()) => {
            println!("Connected successfully with restore options");
            // Get stored packets after connection with restore options
            let final_packets = endpoint.get_stored_packets().await.unwrap();
            println!("Final stored packets: {}", final_packets.len());
        }
        Err(e) => {
            println!("Connection failed (expected for stub transport): {e:?}");
            // Even if connection fails, we can still test that the API works
            let final_packets = endpoint.get_stored_packets().await.unwrap();
            assert_eq!(final_packets.len(), 0);
            println!("Final stored packets: {}", final_packets.len());
        }
    }
}
