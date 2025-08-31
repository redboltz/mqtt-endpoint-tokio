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

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use mqtt_endpoint_tokio::mqtt_ep;
use mqtt_protocol_core::mqtt;

use stub_transport::{StubTransport, TransportResponse};

type ClientEndpoint = Arc<mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u16>>;

#[tokio::test]
async fn test_publish_queuing_with_receive_maximum() {
    common::init_tracing();

    let mut stub = StubTransport::new();

    // Create endpoint and connection options with queuing enabled
    let connection_options = mqtt_ep::connection_option::GenericConnectionOption::<u16>::builder()
        .queuing_receive_maximum(true)
        .build()
        .unwrap();

    let endpoint: ClientEndpoint = Arc::new(mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0));

    // Attach transport with options
    let attach_result = endpoint
        .attach_with_options(stub.clone(), mqtt_ep::Mode::Client, connection_options)
        .await;
    assert!(
        attach_result.is_ok(),
        "Attach should succeed: {attach_result:?}"
    );

    // Send CONNECT packet
    let connect_packet = mqtt_ep::packet::v5_0::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .build()
        .unwrap();
    stub.add_response(TransportResponse::SendOk); // For CONNECT packet
    let send_result = endpoint.send(connect_packet).await;
    assert!(
        send_result.is_ok(),
        "CONNECT should be sent successfully: {send_result:?}"
    );

    // Receive CONNACK
    // Prepare CONNACK response with ReceiveMaximum = 1
    let connack = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .props(vec![mqtt::packet::ReceiveMaximum::new(1).unwrap().into()])
        .build()
        .unwrap();

    let connack_bytes = connack.to_continuous_buffer();
    stub.add_response(TransportResponse::RecvOk(connack_bytes));
    let connack_result = timeout(Duration::from_millis(1000), endpoint.recv()).await;
    assert!(
        connack_result.is_ok(),
        "Should receive CONNACK within timeout"
    );
    let received_packet = connack_result.unwrap();
    assert!(
        received_packet.is_ok(),
        "CONNACK should be received successfully: {received_packet:?}"
    );

    // First publish - should succeed immediately
    let packet_id_1 = endpoint.acquire_packet_id().await.unwrap();
    assert_eq!(packet_id_1, 1, "First packet ID should be 1");
    // Second publish - should be queued due to ReceiveMaximum = 1
    let packet_id_2 = endpoint.acquire_packet_id().await.unwrap();
    assert_eq!(packet_id_2, 2, "Second packet ID should be 2");

    let publish1 = mqtt_ep::packet::v5_0::GenericPublish::builder()
        .topic_name("test/topic")
        .unwrap()
        .payload("payload1")
        .packet_id(packet_id_1)
        .qos(mqtt_ep::packet::Qos::AtLeastOnce)
        .build()
        .unwrap();
    stub.add_response(TransportResponse::SendOk); // For first PUBLISH packet

    // Spawn task to handle subsequent packet reception
    // Add PUBACK response for the first PUBLISH (packet_id = 1)
    let puback = mqtt_ep::packet::v5_0::Puback::builder()
        .packet_id(1)
        .reason_code(mqtt_ep::result_code::PubackReasonCode::Success)
        .build()
        .unwrap();
    let puback_bytes = puback.to_continuous_buffer();
    stub.add_response(TransportResponse::RecvOk(puback_bytes));

    let publish2 = mqtt_ep::packet::v5_0::GenericPublish::builder()
        .topic_name("test/topic")
        .unwrap()
        .payload("payload2")
        .packet_id(packet_id_2)
        .qos(mqtt_ep::packet::Qos::AtLeastOnce)
        .build()
        .unwrap();
    stub.add_response(TransportResponse::SendOk); // For second PUBLISH packet (eventually)

    let puback_task = tokio::spawn({
        let endpoint = endpoint.clone();
        async move {
            let publish1_result = endpoint.send(publish1).await;
            assert!(
                publish1_result.is_ok(),
                "First PUBLISH should succeed: {publish1_result:?}"
            );

            let result = endpoint.send(publish2).await;
            assert!(
                result.is_ok(),
                "Second PUBLISH should eventually succeed: {result:?}"
            );
        }
    });

    // Give some time to ensure second publish is queued
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Receive PUBACK for first publish
    let puback_result = timeout(Duration::from_millis(2000), endpoint.recv()).await;
    assert!(
        puback_result.is_ok(),
        "Should receive PUBACK within timeout"
    );

    // Wait for puback_task to complete and verify it succeeded
    let task_result = timeout(Duration::from_millis(3000), puback_task).await;
    assert!(
        task_result.is_ok(),
        "puback_task should complete within timeout"
    );
    let task_completion = task_result.unwrap();
    assert!(
        task_completion.is_ok(),
        "puback_task should complete successfully: {task_completion:?}"
    );

    // Close endpoint
    let close_result = endpoint.close().await;
    assert!(
        close_result.is_ok(),
        "Close should succeed: {close_result:?}"
    );
}
