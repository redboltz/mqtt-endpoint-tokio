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

use std::time::Duration;
use tokio::time::timeout;

use mqtt_endpoint_tokio::mqtt_ep;

type ClientEndpoint = mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u16>;

#[tokio::test]
async fn test_packet_id_when_available_api_compilation() {
    // Test that the acquire_packet_id_when_available API compiles correctly
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Test that the method exists and compiles
    // Use timeout to prevent indefinite waiting
    let result = timeout(
        Duration::from_millis(50),
        endpoint.acquire_packet_id_when_available(),
    )
    .await;

    // The test is primarily about compilation, result can vary
    // Most importantly, the API exists and compiles correctly
    match result {
        Ok(inner_result) => {
            // Method completed - could be success or error
            println!("Method completed with result: {inner_result:?}");
        }
        Err(_timeout_error) => {
            // Timeout occurred - this is also fine for compilation test
            println!("Method timed out - API compiles correctly");
        }
    }
}

#[tokio::test]
async fn test_acquire_unique_vs_when_available_api() {
    // Test that both packet ID acquisition methods compile
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Both methods should compile and have similar signatures
    let _immediate_future = endpoint.acquire_packet_id();
    let _waiting_future = endpoint.acquire_packet_id_when_available();

    // Test that they both return similar result types (compilation test)
    // Both should return Result<u16, SendError>
    std::mem::drop(Box::pin(_immediate_future)
        as std::pin::Pin<Box<dyn std::future::Future<Output = Result<u16, _>>>>);
    std::mem::drop(Box::pin(_waiting_future)
        as std::pin::Pin<Box<dyn std::future::Future<Output = Result<u16, _>>>>);
}
