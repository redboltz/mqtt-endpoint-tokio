use mqtt_endpoint_tokio::mqtt_ep::endpoint::GenericEndpoint;
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
use mqtt_protocol_core::mqtt;
use mqtt_protocol_core::mqtt::prelude::*;
use tokio::io::duplex;

#[tokio::test]
async fn test_send_concrete_packet() {
    // Create a mock stream using duplex
    let (client_stream, _server_stream) = duplex(1024);

    // Create GenericEndpoint for Client role with u16 packet ID
    let endpoint: GenericEndpoint<mqtt::connection::role::Client, u16> =
        GenericEndpoint::new(mqtt::Version::V3_1_1, client_stream);

    // Create a concrete PINGREQ packet
    let pingreq = mqtt::packet::v3_1_1::Pingreq::new();

    // Test sending concrete packet - should compile due to Sendable trait
    let _result = endpoint.send(pingreq).await;

    // The result should be Ok (even though connection is not established)
    // This test focuses on compilation, not runtime behavior
}

#[tokio::test]
async fn test_send_generic_packet() {
    // Create a mock stream using duplex
    let (client_stream, _server_stream) = duplex(1024);

    // Create GenericEndpoint for Client role with u16 packet ID
    let endpoint: GenericEndpoint<mqtt::connection::role::Client, u16> =
        GenericEndpoint::new(mqtt::Version::V3_1_1, client_stream);

    // Create a GenericPacket from a concrete packet
    let pingreq = mqtt::packet::v3_1_1::Pingreq::new();
    let generic_packet: mqtt::packet::GenericPacket<u16> = pingreq.into();

    // Test sending GenericPacket - should compile due to Sendable<Client, u16> impl
    let _result = endpoint.send(generic_packet).await;

    // The result should be Ok (even though connection is not established)
    // This test focuses on compilation, not runtime behavior
}

#[tokio::test]
async fn test_send_with_different_roles() {
    // Test with Server role
    {
        let (stream, _) = duplex(1024);
        let endpoint: GenericEndpoint<mqtt::connection::role::Server, u16> =
            GenericEndpoint::new(mqtt::Version::V3_1_1, stream);

        let pingresp = mqtt::packet::v3_1_1::Pingresp::new();
        let _result = endpoint.send(pingresp).await;
    }

    // Test with Any role
    {
        let (stream, _) = duplex(1024);
        let endpoint: GenericEndpoint<mqtt::connection::role::Any, u16> =
            GenericEndpoint::new(mqtt::Version::V3_1_1, stream);

        let pingreq = mqtt::packet::v3_1_1::Pingreq::new();
        let generic_packet: mqtt::packet::GenericPacket<u16> = pingreq.into();
        let _result = endpoint.send(generic_packet).await;
    }
}

#[tokio::test]
async fn test_packet_id_management() {
    // Create a mock stream using duplex
    let (client_stream, _server_stream) = duplex(1024);

    // Create GenericEndpoint for Client role with u16 packet ID
    let endpoint: GenericEndpoint<mqtt::connection::role::Client, u16> =
        GenericEndpoint::new(mqtt::Version::V3_1_1, client_stream);

    // Test packet ID management methods
    let _packet_id_result = endpoint.acquire_packet_id().await;
    let _register_result = endpoint.register_packet_id(1).await;
    let _release_result = endpoint.release_packet_id(1).await;

    // These should compile without errors
}

#[tokio::test]
async fn test_send_with_u32_packet_id() {
    // Test with u32 packet ID type (for broker clustering)
    let (client_stream, _server_stream) = duplex(1024);

    // Create GenericEndpoint for Client role with u32 packet ID
    let endpoint: GenericEndpoint<mqtt::connection::role::Client, u32> =
        GenericEndpoint::new(mqtt::Version::V3_1_1, client_stream);

    // Create a concrete packet
    let pingreq = mqtt::packet::v3_1_1::Pingreq::new();
    let _result = endpoint.send(pingreq).await;

    // Create a GenericPacket
    let pingreq2 = mqtt::packet::v3_1_1::Pingreq::new();
    let generic_packet: mqtt::packet::GenericPacket<u32> = pingreq2.into();
    let _result2 = endpoint.send(generic_packet).await;

    // Test packet ID management with u32
    let _packet_id_result = endpoint.acquire_packet_id().await;
    let _register_result = endpoint.register_packet_id(1u32).await;
    let _release_result = endpoint.release_packet_id(1u32).await;
}
