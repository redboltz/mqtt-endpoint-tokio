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
use std::time::Duration;
use stub_transport::{StubTransport, TransportResponse};
use tokio::time::timeout;

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;
type ServerEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Server>;

fn connect_bytes() -> Vec<u8> {
    mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client_with_long_id")
        .unwrap()
        .keep_alive(60)
        .clean_session(true)
        .build()
        .unwrap()
        .to_continuous_buffer()
}

#[tokio::test]
async fn test_with_options_default_behaves_like_new() {
    common::init_tracing();
    let endpoint = ClientEndpoint::with_options(
        mqtt_ep::Version::V5_0,
        mqtt_ep::connection::ConnectionOptions::default(),
    );
    let result = endpoint.get_protocol_version().await.unwrap();
    assert_eq!(result, mqtt_ep::Version::V5_0);
}

#[tokio::test]
async fn test_with_options_reexported_at_top_level() {
    common::init_tracing();
    let options = mqtt_ep::ConnectionOptions::new().receive_maximum(10);
    let endpoint = ClientEndpoint::with_options(mqtt_ep::Version::V3_1_1, options);
    let result = endpoint.get_protocol_version().await.unwrap();
    assert_eq!(result, mqtt_ep::Version::V3_1_1);
}

#[tokio::test]
async fn test_with_options_maximum_packet_size_recv_rejects_large_packet() {
    common::init_tracing();
    let bytes = connect_bytes();
    let limit = (bytes.len() - 1) as u32;

    let options = mqtt_ep::connection::ConnectionOptions::new().maximum_packet_size_recv(limit);
    let endpoint = ServerEndpoint::with_options(mqtt_ep::Version::Undetermined, options);

    let mut stub = StubTransport::new();
    endpoint
        .attach(stub.clone(), mqtt_ep::Mode::Server)
        .await
        .unwrap();
    stub.add_response(TransportResponse::RecvOk(bytes));

    let recv_result = timeout(Duration::from_millis(1000), endpoint.recv())
        .await
        .expect("recv should complete");
    assert!(
        recv_result.is_err(),
        "CONNECT exceeding maximum_packet_size_recv must be rejected: {recv_result:?}"
    );
}

#[tokio::test]
async fn test_with_options_maximum_packet_size_recv_accepts_fitting_packet() {
    common::init_tracing();
    let bytes = connect_bytes();
    let limit = bytes.len() as u32;

    let options = mqtt_ep::connection::ConnectionOptions::new().maximum_packet_size_recv(limit);
    let endpoint = ServerEndpoint::with_options(mqtt_ep::Version::Undetermined, options);

    let mut stub = StubTransport::new();
    endpoint
        .attach(stub.clone(), mqtt_ep::Mode::Server)
        .await
        .unwrap();
    stub.add_response(TransportResponse::RecvOk(bytes));

    let recv_result = timeout(Duration::from_millis(1000), endpoint.recv())
        .await
        .expect("recv should complete");
    assert!(recv_result.is_ok(), "CONNECT within limit: {recv_result:?}");
    assert_eq!(
        endpoint.get_protocol_version().await.unwrap(),
        mqtt_ep::Version::V3_1_1
    );
}
