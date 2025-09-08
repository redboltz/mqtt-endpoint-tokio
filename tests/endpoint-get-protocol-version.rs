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
use mqtt_endpoint_tokio::mqtt_ep;

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;
type ServerEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Server>;

#[tokio::test]
async fn test_get_protocol_version_v3_1_1() {
    common::init_tracing();
    // Test that the get_stored_packets API compiles correctly
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    let result = endpoint.get_protocol_version().await.unwrap();
    assert_eq!(result, mqtt_ep::Version::V3_1_1);
}

#[tokio::test]
async fn test_get_protocol_version_v5_0() {
    common::init_tracing();
    // Test that the get_stored_packets API compiles correctly
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V5_0);

    let result = endpoint.get_protocol_version().await.unwrap();
    assert_eq!(result, mqtt_ep::Version::V5_0);
}

#[tokio::test]
async fn test_get_protocol_version_undetermined() {
    common::init_tracing();
    use std::time::Duration;
    use stub_transport::{StubTransport, TransportResponse};
    use tokio::time::timeout;

    // Create a server endpoint with undetermined version
    let endpoint: ServerEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::Undetermined);

    // Initially should be undetermined
    let result = endpoint.get_protocol_version().await.unwrap();
    assert_eq!(result, mqtt_ep::Version::Undetermined);

    // Create stub transport and attach it
    let mut stub = StubTransport::new();

    let attach_result = endpoint.attach(stub.clone(), mqtt_ep::Mode::Server).await;
    assert!(
        attach_result.is_ok(),
        "Attach should succeed: {attach_result:?}"
    );

    // Create a V3.1.1 CONNECT packet using builder
    let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .keep_alive(60)
        .clean_session(true)
        .build()
        .unwrap();

    // Convert to continuous buffer
    let connect_bytes = connect_packet.to_continuous_buffer();

    // Add the CONNECT packet as response for recv
    stub.add_response(TransportResponse::RecvOk(connect_bytes));

    // Receive the CONNECT packet
    let recv_result = timeout(Duration::from_millis(1000), endpoint.recv()).await;
    assert!(recv_result.is_ok(), "Should receive CONNECT packet");
    let received_packet = recv_result.unwrap();
    assert!(
        received_packet.is_ok(),
        "CONNECT packet should be received successfully"
    );

    // Now check that protocol version has been determined as V3.1.1
    let result = endpoint.get_protocol_version().await.unwrap();
    assert_eq!(result, mqtt_ep::Version::V3_1_1);
}
