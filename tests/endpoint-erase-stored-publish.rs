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
use mqtt_endpoint_tokio::mqtt_ep::prelude::*;

mod common;
mod stub_transport;

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

async fn setup_endpoint_with_connection(version: mqtt_ep::Version) -> ClientEndpoint {
    let endpoint = ClientEndpoint::new(version);

    let mut stub = stub_transport::StubTransport::new();

    let connack_packet = match version {
        mqtt_ep::Version::V3_1_1 => {
            let connack = mqtt_ep::packet::v3_1_1::Connack::builder()
                .session_present(false)
                .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
                .build()
                .unwrap();
            mqtt_ep::packet::Packet::V3_1_1Connack(connack)
        }
        mqtt_ep::Version::V5_0 => {
            let connack = mqtt_ep::packet::v5_0::Connack::builder()
                .session_present(false)
                .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
                .build()
                .unwrap();
            mqtt_ep::packet::Packet::V5_0Connack(connack)
        }
        mqtt_ep::Version::Undetermined => unreachable!("Undetermined version not supported"),
    };

    let connack_bytes = connack_packet.to_continuous_buffer();

    stub.add_response(stub_transport::TransportResponse::SendOk);
    stub.add_response(stub_transport::TransportResponse::RecvOk(connack_bytes));

    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    match version {
        mqtt_ep::Version::V3_1_1 => {
            let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
                .client_id("test_client")
                .unwrap()
                .clean_session(false)
                .build()
                .unwrap();
            let send_result = endpoint.send(connect_packet).await;
            assert!(send_result.is_ok(), "Send CONNECT should succeed");
        }
        mqtt_ep::Version::V5_0 => {
            let connect_packet = mqtt_ep::packet::v5_0::Connect::builder()
                .client_id("test_client")
                .unwrap()
                .clean_start(false)
                .props(vec![mqtt_ep::packet::SessionExpiryInterval::new(3600)
                    .unwrap()
                    .into()])
                .build()
                .unwrap();
            let send_result = endpoint.send(connect_packet).await;
            assert!(send_result.is_ok(), "Send CONNECT should succeed");
        }
        mqtt_ep::Version::Undetermined => unreachable!("Undetermined version not supported"),
    }

    let recv_result = endpoint.recv().await;
    assert!(recv_result.is_ok(), "Receive CONNACK should succeed");

    endpoint
}

#[tokio::test]
async fn test_erase_stored_publish_v3_1_1_qos1() {
    common::init_tracing();
    let endpoint = setup_endpoint_with_connection(mqtt_ep::Version::V3_1_1).await;

    let packet_id = endpoint.acquire_packet_id().await.unwrap();

    let publish = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name("test/topic")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtLeastOnce)
        .packet_id(Some(packet_id))
        .payload(b"test payload")
        .build()
        .unwrap();

    let send_result = endpoint.send(publish).await;
    assert!(send_result.is_ok(), "Send PUBLISH should succeed");

    let stored = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(stored.len(), 1, "Should have one stored packet");

    let erase_result = endpoint.erase_stored_publish(packet_id).await;
    assert!(
        erase_result.is_ok(),
        "erase_stored_publish should succeed: {erase_result:?}"
    );

    let stored_after = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(
        stored_after.len(),
        0,
        "Should have no stored packets after erase"
    );

    let new_packet_id = endpoint.acquire_packet_id().await.unwrap();
    assert_eq!(
        new_packet_id, packet_id,
        "Should be able to reacquire the same packet ID"
    );
}

#[tokio::test]
async fn test_erase_stored_publish_v3_1_1_qos2() {
    common::init_tracing();
    let endpoint = setup_endpoint_with_connection(mqtt_ep::Version::V3_1_1).await;

    let packet_id = endpoint.acquire_packet_id().await.unwrap();

    let publish = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name("test/topic")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::ExactlyOnce)
        .packet_id(Some(packet_id))
        .payload(b"test payload")
        .build()
        .unwrap();

    let send_result = endpoint.send(publish).await;
    assert!(send_result.is_ok(), "Send PUBLISH should succeed");

    let stored = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(stored.len(), 1, "Should have one stored packet");

    let erase_result = endpoint.erase_stored_publish(packet_id).await;
    assert!(
        erase_result.is_ok(),
        "erase_stored_publish should succeed: {erase_result:?}"
    );

    let stored_after = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(
        stored_after.len(),
        0,
        "Should have no stored packets after erase"
    );

    let new_packet_id = endpoint.acquire_packet_id().await.unwrap();
    assert_eq!(
        new_packet_id, packet_id,
        "Should be able to reacquire the same packet ID"
    );
}

#[tokio::test]
async fn test_erase_stored_publish_v5_0_qos1() {
    common::init_tracing();
    let endpoint = setup_endpoint_with_connection(mqtt_ep::Version::V5_0).await;

    let packet_id = endpoint.acquire_packet_id().await.unwrap();

    let publish = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/topic")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::AtLeastOnce)
        .packet_id(Some(packet_id))
        .payload(b"test payload")
        .build()
        .unwrap();

    let send_result = endpoint.send(publish).await;
    assert!(send_result.is_ok(), "Send PUBLISH should succeed");

    let stored = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(stored.len(), 1, "Should have one stored packet");

    let erase_result = endpoint.erase_stored_publish(packet_id).await;
    assert!(
        erase_result.is_ok(),
        "erase_stored_publish should succeed: {erase_result:?}"
    );

    let stored_after = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(
        stored_after.len(),
        0,
        "Should have no stored packets after erase"
    );

    let new_packet_id = endpoint.acquire_packet_id().await.unwrap();
    assert_eq!(
        new_packet_id, packet_id,
        "Should be able to reacquire the same packet ID"
    );
}

#[tokio::test]
async fn test_erase_stored_publish_v5_0_qos2() {
    common::init_tracing();
    let endpoint = setup_endpoint_with_connection(mqtt_ep::Version::V5_0).await;

    let packet_id = endpoint.acquire_packet_id().await.unwrap();

    let publish = mqtt_ep::packet::v5_0::Publish::builder()
        .topic_name("test/topic")
        .unwrap()
        .qos(mqtt_ep::packet::Qos::ExactlyOnce)
        .packet_id(Some(packet_id))
        .payload(b"test payload")
        .build()
        .unwrap();

    let send_result = endpoint.send(publish).await;
    assert!(send_result.is_ok(), "Send PUBLISH should succeed");

    let stored = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(stored.len(), 1, "Should have one stored packet");

    let erase_result = endpoint.erase_stored_publish(packet_id).await;
    assert!(
        erase_result.is_ok(),
        "erase_stored_publish should succeed: {erase_result:?}"
    );

    let stored_after = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(
        stored_after.len(),
        0,
        "Should have no stored packets after erase"
    );

    let new_packet_id = endpoint.acquire_packet_id().await.unwrap();
    assert_eq!(
        new_packet_id, packet_id,
        "Should be able to reacquire the same packet ID"
    );
}

#[tokio::test]
async fn test_erase_stored_publish_nonexistent() {
    common::init_tracing();
    let endpoint = setup_endpoint_with_connection(mqtt_ep::Version::V5_0).await;

    let erase_result = endpoint.erase_stored_publish(42).await;
    assert!(
        erase_result.is_ok(),
        "erase_stored_publish should succeed even if packet doesn't exist: {erase_result:?}"
    );

    let stored = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(
        stored.len(),
        0,
        "Should have no stored packets when erasing nonexistent packet"
    );
}

#[tokio::test]
async fn test_erase_stored_publish_api_compilation() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    let result = endpoint.erase_stored_publish(1).await;

    assert!(
        result.is_ok(),
        "erase_stored_publish should succeed: {result:?}"
    );
}
