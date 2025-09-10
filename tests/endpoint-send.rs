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

#[tokio::test]
async fn test_send_concrete_packet() {
    common::init_tracing();
    // Create a mock stream using duplex

    // Create mqtt_ep::Endpoint for Client role
    let endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);

    // Create a concrete PINGREQ packet
    let pingreq = mqtt_ep::packet::v3_1_1::Pingreq::new();

    // Test sending concrete packet - should compile due to Sendable trait
    let _result = endpoint.send(pingreq).await;

    // The result should be Ok (even though connection is not established)
    // This test focuses on compilation, not runtime behavior
}

#[tokio::test]
async fn test_send_enum_packet() {
    common::init_tracing();
    // Create a mock stream using duplex

    // Create mqtt_ep::Endpoint for Client role
    let endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);

    // Create a Packet from a concrete packet
    let pingreq = mqtt_ep::packet::v3_1_1::Pingreq::new();
    let enum_packet: mqtt_ep::packet::Packet = pingreq.into();

    let _result = endpoint.send(enum_packet).await;

    // The result should be Ok (even though connection is not established)
    // This test focuses on compilation, not runtime behavior
}

#[tokio::test]
async fn test_send_with_different_roles() {
    common::init_tracing();
    // Test with Server role
    {
        let endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Server> =
            mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);

        let pingresp = mqtt_ep::packet::v3_1_1::Pingresp::new();
        let _result = endpoint.send(pingresp).await;
    }

    // Test with Any role
    {
        let endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Any> =
            mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);

        let pingreq = mqtt_ep::packet::v3_1_1::Pingreq::new();
        let enum_packet: mqtt_ep::packet::Packet = pingreq.into();
        let _result = endpoint.send(enum_packet).await;
    }
}

#[tokio::test]
async fn test_packet_id_management() {
    common::init_tracing();
    // Create a mock stream using duplex

    // Create mqtt_ep::Endpoint for Client role with u16 packet ID
    let endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V3_1_1);

    // Test packet ID management methods
    let _packet_id_result = endpoint.acquire_packet_id().await;
    let _register_result = endpoint.register_packet_id(1).await;
    let _release_result = endpoint.release_packet_id(1).await;

    // These should compile without errors
}

#[tokio::test]
async fn test_send_with_u32_packet_id() {
    common::init_tracing();
    // Test with u32 packet ID type (for broker clustering)

    // Create mqtt_ep::Endpoint for Client role with u32 packet ID
    let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u32> =
        mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Create a concrete packet
    let pingreq = mqtt_ep::packet::v3_1_1::Pingreq::new();
    let _result = endpoint.send(pingreq).await;

    // Create a Packet
    let pingreq2 = mqtt_ep::packet::v3_1_1::Pingreq::new();
    let enum_packet: mqtt_ep::packet::GenericPacket<u32> = pingreq2.into();
    let _result2 = endpoint.send(enum_packet).await;

    // Test packet ID management with u32
    let _packet_id_result = endpoint.acquire_packet_id().await;
    let _register_result = endpoint.register_packet_id(1u32).await;
    let _release_result = endpoint.release_packet_id(1u32).await;
}

#[tokio::test]
async fn test_send_acquired_packet_and_error() {
    common::init_tracing();

    let mut stub = stub_transport::StubTransport::new();
    let endpoint: mqtt_ep::Endpoint<mqtt_ep::role::Client> =
        mqtt_ep::Endpoint::new(mqtt_ep::Version::V5_0);

    // Prepare CONNACK response bytes
    let connack_packet = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .build()
        .unwrap();
    let connack_bytes = connack_packet.to_continuous_buffer();

    // Configure stub responses in order:
    stub.add_response(stub_transport::TransportResponse::SendOk); // For CONNECT
    stub.add_response(stub_transport::TransportResponse::RecvOk(connack_bytes)); // CONNACK response
                                                                                 // Note: We're not adding a response for SUBSCRIBE send, which should cause the stub to return an error
                                                                                 // due to running out of responses

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
        .clean_start(true)
        .build()
        .unwrap();

    let send_result = endpoint.send(connect_packet).await;
    assert!(send_result.is_ok(), "CONNECT should be sent successfully");

    // Receive CONNACK to establish connection
    let connack_result = endpoint.recv().await;
    assert!(
        connack_result.is_ok(),
        "CONNACK should be received successfully"
    );

    // Now acquire a packet ID - should get 1
    let first_packet_id = endpoint.acquire_packet_id().await;
    assert!(
        first_packet_id.is_ok(),
        "First packet ID acquisition should succeed"
    );
    let packet_id_1 = first_packet_id.unwrap();
    assert_eq!(packet_id_1, 1, "First acquired packet ID should be 1");

    // Create SUBSCRIBE packet with the acquired packet ID
    let sub_opts = mqtt_ep::packet::SubOpts::new().set_qos(mqtt_ep::packet::Qos::AtLeastOnce);
    let sub_entry = mqtt_ep::packet::SubEntry::new("test/topic", sub_opts)
        .expect("Failed to create subscription entry");

    let subscribe_packet = mqtt_ep::packet::v5_0::Subscribe::builder()
        .packet_id(packet_id_1)
        .entries(vec![sub_entry])
        .build()
        .unwrap();

    // Send SUBSCRIBE packet - may succeed or fail depending on transport behavior
    let subscribe_send_result = endpoint.send(subscribe_packet).await;

    if subscribe_send_result.is_ok() {
        // SUBSCRIBE succeeded - close endpoint to trigger packet ID release
        let close_result = endpoint.close().await;
        assert!(close_result.is_ok(), "Endpoint close should succeed");
    }
    // If send failed, packet ID should already be released by the error handling

    // Verify error type if send failed
    if let Err(err) = &subscribe_send_result {
        match err {
            mqtt_ep::connection_error::ConnectionError::Transport(_)
            | mqtt_ep::connection_error::ConnectionError::NotConnected => {
                // Expected error types that should trigger packet ID release
            }
            other => panic!("Unexpected error type: {other:?}"),
        }
    }

    // Now acquire packet ID again - should get 1 again (packet ID was released)
    let second_packet_id = endpoint.acquire_packet_id().await;
    assert!(
        second_packet_id.is_ok(),
        "Second packet ID acquisition should succeed"
    );
    let reused_packet_id = second_packet_id.unwrap();
    assert_eq!(
        reused_packet_id, 1,
        "Reused packet ID should also be 1 (packet ID was properly released)"
    );

    // Verify that packet ID 1 is indeed reusable by successfully acquiring it twice
    assert_eq!(
        packet_id_1, reused_packet_id,
        "Both packet IDs should be the same (1)"
    );
}
