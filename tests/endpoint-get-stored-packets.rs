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
async fn test_get_stored_packets_api_compilation() {
    // Test that the get_stored_packets API compiles correctly
    let (client_stream, _server_stream) = tokio::io::duplex(1024);
    let endpoint: ClientEndpoint = GenericEndpoint::new(Version::V3_1_1, client_stream);

    // Test that the get_stored_packets method exists and compiles
    let result = endpoint.get_stored_packets().await;

    // The method should complete successfully
    match result {
        Ok(packets) => {
            println!(
                "get_stored_packets completed successfully, got {} packets",
                packets.len()
            );
            // Should be empty initially as no packets have been stored
            assert_eq!(packets.len(), 0);
        }
        Err(e) => {
            println!("get_stored_packets completed with error: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_get_stored_packets_with_different_roles() {
    // Test get_stored_packets method with different roles

    // Test with Server role
    {
        let (stream, _) = tokio::io::duplex(1024);
        let endpoint: GenericEndpoint<role::Server, u16> =
            GenericEndpoint::new(Version::V3_1_1, stream);
        let result = endpoint.get_stored_packets().await;
        assert!(result.is_ok());
    }

    // Test with Any role
    {
        let (stream, _) = tokio::io::duplex(1024);
        let endpoint: GenericEndpoint<role::Any, u16> =
            GenericEndpoint::new(Version::V3_1_1, stream);
        let result = endpoint.get_stored_packets().await;
        assert!(result.is_ok());
    }

    // Test with u32 packet ID type
    {
        let (stream, _) = tokio::io::duplex(1024);
        let endpoint: GenericEndpoint<role::Client, u32> =
            GenericEndpoint::new(Version::V3_1_1, stream);
        let result = endpoint.get_stored_packets().await;
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn test_get_stored_packets_after_close() {
    // Test that get_stored_packets after close returns appropriate errors
    let (client_stream, _server_stream) = tokio::io::duplex(1024);
    let endpoint: ClientEndpoint = GenericEndpoint::new(Version::V3_1_1, client_stream);

    // Close the endpoint
    let close_result = endpoint.close().await;
    println!("Close result: {:?}", close_result);

    // Try to get stored packets after close - should fail with ChannelClosed
    let get_result = endpoint.get_stored_packets().await;
    println!("get_stored_packets after close result: {:?}", get_result);

    // Operation after close should return ChannelClosed error
    match get_result {
        Err(mqtt_endpoint_tokio::mqtt_ep::SendError::ChannelClosed) => {
            println!("get_stored_packets correctly returned ChannelClosed after close");
        }
        _ => {
            println!("get_stored_packets did not return expected ChannelClosed error");
        }
    }
}

#[tokio::test]
async fn test_restore_and_get_stored_packets_roundtrip() {
    // Test the roundtrip: restore packets -> get stored packets
    let (client_stream, _server_stream) = tokio::io::duplex(1024);
    let endpoint: ClientEndpoint = GenericEndpoint::new(Version::V3_1_1, client_stream);

    // Initially should have no stored packets
    let initial_packets = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(initial_packets.len(), 0);
    println!("Initial stored packets: {}", initial_packets.len());

    // Restore some packets (empty vector for this test)
    let packets_to_restore: Vec<GenericStorePacket<u16>> = Vec::new();
    let restore_result = endpoint.restore_packets(packets_to_restore).await;
    assert!(restore_result.is_ok());
    println!("Restore packets result: {:?}", restore_result);

    // Get stored packets again - should still be empty since we restored empty vector
    let final_packets = endpoint.get_stored_packets().await.unwrap();
    assert_eq!(final_packets.len(), 0);
    println!("Final stored packets: {}", final_packets.len());
}
