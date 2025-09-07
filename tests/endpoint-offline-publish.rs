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

use stub_transport::{StubTransport, TransportResponse};

type ClientEndpoint = Arc<mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u16>>;

#[tokio::test]
async fn test_offline_publish() {
    common::init_tracing();

    let mut stub = StubTransport::new();

    // Create endpoint and connection options with queuing enabled
    let connection_options = mqtt_ep::connection_option::GenericConnectionOption::<u16>::builder()
        .queuing_receive_maximum(true)
        .build()
        .unwrap();

    let endpoint: ClientEndpoint = Arc::new(mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0));
    endpoint.set_offline_publish(true).await;

    // First publish - should succeed immediately
    let packet_id_1 = endpoint.acquire_packet_id().await.unwrap();
    assert_eq!(packet_id_1, 1, "First packet ID should be 1");

    let publish1 = mqtt_ep::packet::v5_0::GenericPublish::builder()
        .topic_name("test/topic")
        .unwrap()
        .payload("payload1")
        .packet_id(packet_id_1)
        .qos(mqtt_ep::packet::Qos::AtLeastOnce)
        .build()
        .unwrap();
    stub.add_response(TransportResponse::SendOk); // For first PUBLISH packet
    let send_result = endpoint.send(publish1).await;
    assert!(
        send_result.is_ok(),
        "PUBLISH should be sent successfully: {send_result:?}"
    );

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
        .clean_start(false)
        .build()
        .unwrap();
    stub.add_response(TransportResponse::SendOk); // For CONNECT packet
    let send_result = endpoint.send(connect_packet).await;
    assert!(
        send_result.is_ok(),
        "CONNECT should be sent successfully: {send_result:?}"
    );

    // Receive CONNACK
    let connack = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(true)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
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

}