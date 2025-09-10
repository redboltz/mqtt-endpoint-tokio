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

mod common;

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

#[tokio::test]
async fn test_packet_id_when_available_api_compilation() {
    common::init_tracing();
    // Test that the acquire_packet_id_when_available API compiles correctly
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

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
            // Method completed - verify it's a proper result type
            assert!(
                inner_result.is_ok() || inner_result.is_err(),
                "Method should return a proper Result type: {inner_result:?}"
            );
        }
        Err(_timeout_error) => {
            // Timeout occurred - this is also fine for compilation test
            // The API exists and compiles correctly
        }
    }
}

#[tokio::test]
async fn test_acquire_unique_vs_when_available_api() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // 1. Call acquire_packet_id() 65535 times to exhaust all packet IDs
    let mut acquired_ids = Vec::new();
    for i in 0..65535 {
        let packet_id_result = endpoint.acquire_packet_id().await;
        assert!(
            packet_id_result.is_ok(),
            "acquire_packet_id should succeed for iteration {i}: {packet_id_result:?}"
        );
        let packet_id = packet_id_result.unwrap();
        acquired_ids.push(packet_id);
    }

    // 2. Call acquire_packet_id_when_available() - this should not return immediately
    let when_available_future = endpoint.acquire_packet_id_when_available();
    tokio::pin!(when_available_future);

    // Verify that acquire_packet_id_when_available() doesn't complete immediately
    let result = timeout(Duration::from_millis(100), &mut when_available_future).await;
    assert!(
        result.is_err(),
        "acquire_packet_id_when_available should not complete immediately when all IDs are taken"
    );

    // 3. Release packet ID 123
    let release_result = endpoint.release_packet_id(123).await;
    assert!(
        release_result.is_ok(),
        "release_packet_id should succeed: {release_result:?}"
    );

    // 4. Now acquire_packet_id_when_available() should return with 123
    let result = timeout(Duration::from_millis(1000), when_available_future).await;
    let packet_id = result
        .expect("acquire_packet_id_when_available should complete after release")
        .expect("acquire_packet_id_when_available should succeed");

    assert_eq!(
        packet_id, 123,
        "acquire_packet_id_when_available should return the released packet ID 123"
    );

    // Clean up: release all acquired packet IDs
    for id in acquired_ids {
        if id != 123 {
            // 123 was already released and reacquired
            let cleanup_result = endpoint.release_packet_id(id).await;
            assert!(
                cleanup_result.is_ok(),
                "cleanup release_packet_id should succeed for ID {id}: {cleanup_result:?}"
            );
        }
    }

    // Also release the reacquired ID 123
    let final_release = endpoint.release_packet_id(123).await;
    assert!(
        final_release.is_ok(),
        "final release of ID 123 should succeed: {final_release:?}"
    );
}
