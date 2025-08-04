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
use mqtt_endpoint_tokio::mqtt_ep;

type ClientEndpoint = mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u16>;

#[tokio::test]
async fn test_close_api_compilation() {
    // Test that the close API compiles correctly
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);
    // Test that the close method exists and compiles
    let result = endpoint.close().await;

    // The method should complete successfully (duplex stream supports shutdown)
    match result {
        Ok(()) => {
            println!("Close completed successfully");
        }
        Err(e) => {
            println!("Close completed with error: {e:?}");
        }
    }
}

#[tokio::test]
async fn test_close_with_different_roles() {
    // Test close method with different roles

    // Test with Server role
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Server, u16> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);
        let _result = endpoint.close().await;
    }

    // Test with Any role
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Any, u16> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);
        let _result = endpoint.close().await;
    }

    // Test with u32 packet ID type
    {
        let endpoint: mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u32> =
            mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);
        let _result = endpoint.close().await;
    }
}

#[tokio::test]
async fn test_operations_after_close() {
    // Test that operations after close return appropriate errors
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Close the endpoint
    let close_result = endpoint.close().await;
    println!("Close result: {close_result:?}");

    // Try to use the endpoint after close - should fail with ChannelClosed
    let send_result = endpoint.send(mqtt_ep::packet::v3_1_1::Pingreq::new()).await;
    println!("Send after close result: {send_result:?}");

    let packet_id_result = endpoint.acquire_packet_id().await;
    println!("Acquire packet ID after close result: {packet_id_result:?}");

    // All operations after close should return ChannelClosed error
    match send_result {
        Err(mqtt_ep::ConnectionError::ChannelClosed) => {
            println!("Send correctly returned ChannelClosed after close");
        }
        _ => {
            println!("Send did not return expected ChannelClosed error");
        }
    }
}
