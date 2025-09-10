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
async fn test_get_qos2_publish_handled_pids() {
    common::init_tracing();

    // Create a stub transport
    let mut stub = stub_transport::StubTransport::new();

    // Test that the get_stored_packets API compiles correctly
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Test initial state - pids should be empty
    let result = endpoint.get_qos2_publish_handled_pids().await;
    match result {
        Ok(pids) => {
            assert!(pids.is_empty());
        }
        Err(e) => {
            assert!(
                false,
                "get_qos2_publish_handled_pids completed with error: {e:?}"
            );
        }
    }

    // Attach transport to endpoint
    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    // Create a QoS2 PUBLISH packet to simulate incoming message
    let packet_id = 42u16;
    let qos2_publish = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name("test/topic")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::ExactlyOnce)
        .packet_id(packet_id)
        .payload("test payload".as_bytes())
        .build()
        .unwrap();

    // Convert packet to bytes for transport response
    let publish_bytes = qos2_publish.to_continuous_buffer();
    stub.add_response(stub_transport::TransportResponse::RecvOk(publish_bytes));

    // Simulate receiving the QoS2 PUBLISH packet
    let received_packet = endpoint.recv().await;
    assert!(
        received_packet.is_ok(),
        "Should receive PUBLISH packet successfully"
    );

    // Verify we received the expected packet
    match received_packet.unwrap() {
        mqtt_ep::packet::Packet::V3_1_1Publish(publish) => {
            assert_eq!(publish.packet_id(), Some(packet_id));
            assert_eq!(publish.qos(), mqtt_ep::packet::Qos::ExactlyOnce);
        }
        _ => panic!("Expected PUBLISH packet"),
    }

    // Now check that the packet ID is recorded in handled QoS2 publishes
    let result = endpoint.get_qos2_publish_handled_pids().await;
    match result {
        Ok(pids) => {
            assert_eq!(
                pids.len(),
                1,
                "Should have exactly 1 handled QoS2 packet ID"
            );
            assert!(
                pids.contains(&packet_id),
                "Should contain the received packet ID"
            );
        }
        Err(e) => {
            assert!(
                false,
                "get_qos2_publish_handled_pids completed with error: {e:?}"
            );
        }
    }
}
