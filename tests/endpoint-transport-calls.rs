/**
 * MIT License
 *
 * Copyright (c) 2025 Takatoshi Kondo
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

mod stub_transport;

use std::time::Duration;
use tokio::time::timeout;

use mqtt_endpoint_tokio::mqtt_ep::{GenericEndpoint, PacketFilter};
use mqtt_protocol_core::mqtt::Version;
use mqtt_protocol_core::mqtt::connection::role;
use mqtt_protocol_core::mqtt::packet::PacketType;

use stub_transport::{StubTransport, TransportCall, TransportResponse};

type ClientEndpoint = GenericEndpoint<role::Client, u16>;

#[tokio::test]
async fn test_connect_calls_handshake() {
    let mut stub = StubTransport::new();
    stub.add_response(TransportResponse::HandshakeOk);

    let endpoint: ClientEndpoint = GenericEndpoint::new_disconnected(Version::V3_1_1);
    
    // Connect should call handshake
    let result = endpoint.connect(stub.clone()).await;
    assert!(result.is_ok(), "Connect should succeed");

    // Verify handshake was called
    let calls = stub.get_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], TransportCall::Handshake);
}

#[tokio::test]
async fn test_connect_handshake_failure() {
    let mut stub = StubTransport::new();
    stub.add_response(TransportResponse::HandshakeErr(
        mqtt_endpoint_tokio::mqtt_ep::TransportError::NotConnected,
    ));

    let endpoint: ClientEndpoint = GenericEndpoint::new_disconnected(Version::V3_1_1);
    
    // Connect should fail when handshake fails
    let result = endpoint.connect(stub.clone()).await;
    assert!(result.is_err(), "Connect should fail when handshake fails");

    // Verify handshake was called
    let calls = stub.get_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], TransportCall::Handshake);
}

#[tokio::test]
async fn test_recv_calls_transport_recv() {
    let mut stub = StubTransport::new();
    
    // Setup responses
    stub.add_response(TransportResponse::HandshakeOk);
    // Add a MQTT CONNACK packet as response (simplified)
    let connack_packet = vec![0x20, 0x02, 0x00, 0x00]; // CONNACK with success
    stub.add_response(TransportResponse::RecvOk(connack_packet));

    let endpoint: ClientEndpoint = GenericEndpoint::new_disconnected(Version::V3_1_1);
    
    // Connect first
    let connect_result = endpoint.connect(stub.clone()).await;
    assert!(connect_result.is_ok(), "Connect should succeed");

    // Try to receive a packet (with timeout to avoid hanging)
    let recv_future = endpoint.recv_filtered(PacketFilter::Any);
    let _result = timeout(Duration::from_millis(100), recv_future).await;

    // Check the calls made to transport
    let calls = stub.get_calls();
    println!("Transport calls: {:?}", calls);
    
    // Should have handshake call and at least one recv call
    assert!(calls.len() >= 2, "Should have handshake and recv calls");
    assert_eq!(calls[0], TransportCall::Handshake);
    
    // Check if there's a recv call
    let has_recv = calls.iter().any(|call| matches!(call, TransportCall::Recv { .. }));
    assert!(has_recv, "Should have made recv call to transport");
}

#[tokio::test]
async fn test_close_calls_shutdown() {
    let mut stub = StubTransport::new();
    stub.add_response(TransportResponse::HandshakeOk);
    stub.add_response(TransportResponse::Shutdown);

    let endpoint: ClientEndpoint = GenericEndpoint::new_disconnected(Version::V3_1_1);
    
    // Connect first
    let connect_result = endpoint.connect(stub.clone()).await;
    assert!(connect_result.is_ok(), "Connect should succeed");

    // Close the connection
    let close_result = endpoint.close().await;
    assert!(close_result.is_ok(), "Close should succeed");

    // Verify shutdown was called
    let calls = stub.get_calls();
    println!("Transport calls: {:?}", calls);
    
    assert!(calls.len() >= 2, "Should have handshake and shutdown calls");
    assert_eq!(calls[0], TransportCall::Handshake);
    
    // Check if there's a shutdown call
    let has_shutdown = calls.iter().any(|call| matches!(call, TransportCall::Shutdown { .. }));
    assert!(has_shutdown, "Should have made shutdown call to transport");
}

#[tokio::test]
async fn test_multiple_recv_attempts_for_unmatched_packets() {
    let mut stub = StubTransport::new();
    
    // Setup responses
    stub.add_response(TransportResponse::HandshakeOk);
    
    // Add multiple different packets that don't match our filter
    let ping_packet = vec![0xC0, 0x00]; // PINGREQ
    let pingresp_packet = vec![0xD0, 0x00]; // PINGRESP  
    let connack_packet = vec![0x20, 0x02, 0x00, 0x00]; // CONNACK (what we want)
    
    stub.add_response(TransportResponse::RecvOk(ping_packet));
    stub.add_response(TransportResponse::RecvOk(pingresp_packet));
    stub.add_response(TransportResponse::RecvOk(connack_packet));

    let endpoint: ClientEndpoint = GenericEndpoint::new_disconnected(Version::V3_1_1);
    
    // Connect first
    let connect_result = endpoint.connect(stub.clone()).await;
    assert!(connect_result.is_ok(), "Connect should succeed");

    // Try to receive only CONNACK packets
    let recv_future = endpoint.recv_filtered(PacketFilter::include(vec![PacketType::Connack]));
    let _result = timeout(Duration::from_millis(500), recv_future).await;

    // Check the calls made to transport
    let calls = stub.get_calls();
    println!("Transport calls: {:?}", calls);
    
    // Should have multiple recv calls as it tries to find matching packet
    let recv_calls: Vec<_> = calls.iter().filter(|call| matches!(call, TransportCall::Recv { .. })).collect();
    assert!(recv_calls.len() >= 2, "Should have made multiple recv calls to find matching packet");
}