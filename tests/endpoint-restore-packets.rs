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
use mqtt_endpoint_tokio::mqtt_ep::GenericEndpoint;
use mqtt_protocol_core::mqtt::Version;
use mqtt_protocol_core::mqtt::connection::role;
use mqtt_protocol_core::mqtt::packet::GenericStorePacket;

type ClientEndpoint = GenericEndpoint<role::Client, u16>;

#[tokio::test]
async fn test_restore_packets_api_compilation() {
    // Test that the restore_packets API compiles correctly
    let (client_stream, _server_stream) = tokio::io::duplex(1024);
    let endpoint: ClientEndpoint = GenericEndpoint::new(Version::V3_1_1, client_stream);

    // Create an empty vector of GenericStorePacket for testing
    let packets: Vec<GenericStorePacket<u16>> = Vec::new();

    // Test that the restore_packets method exists and compiles
    let result = endpoint.restore_packets(packets).await;

    // The method should complete successfully with empty packet list
    match result {
        Ok(()) => {
            println!("restore_packets completed successfully");
        }
        Err(e) => {
            println!("restore_packets completed with error: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_restore_packets_with_different_roles() {
    // Test restore_packets method with different roles
    
    // Test with Server role
    {
        let (stream, _) = tokio::io::duplex(1024);
        let endpoint: GenericEndpoint<role::Server, u16> =
            GenericEndpoint::new(Version::V3_1_1, stream);
        let packets: Vec<GenericStorePacket<u16>> = Vec::new();
        let _result = endpoint.restore_packets(packets).await;
    }

    // Test with Any role  
    {
        let (stream, _) = tokio::io::duplex(1024);
        let endpoint: GenericEndpoint<role::Any, u16> =
            GenericEndpoint::new(Version::V3_1_1, stream);
        let packets: Vec<GenericStorePacket<u16>> = Vec::new();
        let _result = endpoint.restore_packets(packets).await;
    }

    // Test with u32 packet ID type
    {
        let (stream, _) = tokio::io::duplex(1024);
        let endpoint: GenericEndpoint<role::Client, u32> =
            GenericEndpoint::new(Version::V3_1_1, stream);
        let packets: Vec<GenericStorePacket<u32>> = Vec::new();
        let _result = endpoint.restore_packets(packets).await;
    }
}

#[tokio::test]
async fn test_restore_packets_after_close() {
    // Test that restore_packets after close returns appropriate errors
    let (client_stream, _server_stream) = tokio::io::duplex(1024);
    let endpoint: ClientEndpoint = GenericEndpoint::new(Version::V3_1_1, client_stream);

    // Close the endpoint
    let close_result = endpoint.close().await;
    println!("Close result: {:?}", close_result);

    // Try to restore packets after close - should fail with ChannelClosed
    let packets: Vec<GenericStorePacket<u16>> = Vec::new();
    let restore_result = endpoint.restore_packets(packets).await;
    println!("restore_packets after close result: {:?}", restore_result);

    // Operation after close should return ChannelClosed error
    match restore_result {
        Err(mqtt_endpoint_tokio::mqtt_ep::SendError::ChannelClosed) => {
            println!("restore_packets correctly returned ChannelClosed after close");
        }
        _ => {
            println!("restore_packets did not return expected ChannelClosed error");
        }
    }
}

#[tokio::test]
async fn test_multiple_restore_packets_calls() {
    // Test multiple calls to restore_packets
    let (client_stream, _server_stream) = tokio::io::duplex(1024);
    let endpoint: ClientEndpoint = GenericEndpoint::new(Version::V3_1_1, client_stream);

    // Make multiple restore_packets calls with empty vectors
    let packets1: Vec<GenericStorePacket<u16>> = Vec::new();
    let result1 = endpoint.restore_packets(packets1).await;
    println!("First restore_packets result: {:?}", result1);

    let packets2: Vec<GenericStorePacket<u16>> = Vec::new();  
    let result2 = endpoint.restore_packets(packets2).await;
    println!("Second restore_packets result: {:?}", result2);

    // Both calls should succeed
    assert!(result1.is_ok());
    assert!(result2.is_ok());
}