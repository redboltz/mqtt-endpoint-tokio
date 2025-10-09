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

use mqtt_endpoint_tokio::mqtt_ep;

mod common;

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

#[tokio::test]
async fn test_set_auto_pub_response() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Test setting auto pub response to false
    let result = endpoint.set_auto_pub_response(false).await;
    assert!(
        result.is_ok(),
        "set_auto_pub_response(false) should succeed: {result:?}"
    );

    // Test setting auto pub response to true
    let result = endpoint.set_auto_pub_response(true).await;
    assert!(
        result.is_ok(),
        "set_auto_pub_response(true) should succeed: {result:?}"
    );
}

#[tokio::test]
async fn test_set_auto_ping_response() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Test setting auto ping response to false
    let result = endpoint.set_auto_ping_response(false).await;
    assert!(
        result.is_ok(),
        "set_auto_ping_response(false) should succeed: {result:?}"
    );

    // Test setting auto ping response to true
    let result = endpoint.set_auto_ping_response(true).await;
    assert!(
        result.is_ok(),
        "set_auto_ping_response(true) should succeed: {result:?}"
    );
}

#[tokio::test]
async fn test_set_auto_map_topic_alias_send() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Test setting auto map topic alias send to true
    let result = endpoint.set_auto_map_topic_alias_send(true).await;
    assert!(
        result.is_ok(),
        "set_auto_map_topic_alias_send(true) should succeed: {result:?}"
    );

    // Test setting auto map topic alias send to false
    let result = endpoint.set_auto_map_topic_alias_send(false).await;
    assert!(
        result.is_ok(),
        "set_auto_map_topic_alias_send(false) should succeed: {result:?}"
    );
}

#[tokio::test]
async fn test_set_auto_replace_topic_alias_send() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Test setting auto replace topic alias send to true
    let result = endpoint.set_auto_replace_topic_alias_send(true).await;
    assert!(
        result.is_ok(),
        "set_auto_replace_topic_alias_send(true) should succeed: {result:?}"
    );

    // Test setting auto replace topic alias send to false
    let result = endpoint.set_auto_replace_topic_alias_send(false).await;
    assert!(
        result.is_ok(),
        "set_auto_replace_topic_alias_send(false) should succeed: {result:?}"
    );
}

#[tokio::test]
async fn test_all_set_auto_methods() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Test calling all set_auto_* methods in sequence
    let result = endpoint.set_auto_pub_response(false).await;
    assert!(result.is_ok(), "set_auto_pub_response failed: {result:?}");

    let result = endpoint.set_auto_ping_response(false).await;
    assert!(result.is_ok(), "set_auto_ping_response failed: {result:?}");

    let result = endpoint.set_auto_map_topic_alias_send(true).await;
    assert!(
        result.is_ok(),
        "set_auto_map_topic_alias_send failed: {result:?}"
    );

    let result = endpoint.set_auto_replace_topic_alias_send(true).await;
    assert!(
        result.is_ok(),
        "set_auto_replace_topic_alias_send failed: {result:?}"
    );
}

#[tokio::test]
async fn test_set_auto_methods_after_close() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Close the endpoint
    let close_result = endpoint.close().await;
    assert!(
        close_result.is_ok(),
        "Close should succeed: {close_result:?}"
    );

    // Try to set auto methods after close - should still succeed as event loop is running
    // The settings will be applied even though no transport is attached
    let result = endpoint.set_auto_pub_response(false).await;
    assert!(
        result.is_ok(),
        "set_auto_pub_response should succeed after close: {result:?}"
    );

    let result = endpoint.set_auto_ping_response(false).await;
    assert!(
        result.is_ok(),
        "set_auto_ping_response should succeed after close: {result:?}"
    );

    let result = endpoint.set_auto_map_topic_alias_send(true).await;
    assert!(
        result.is_ok(),
        "set_auto_map_topic_alias_send should succeed after close: {result:?}"
    );

    let result = endpoint.set_auto_replace_topic_alias_send(true).await;
    assert!(
        result.is_ok(),
        "set_auto_replace_topic_alias_send should succeed after close: {result:?}"
    );
}

#[tokio::test]
async fn test_set_auto_methods_with_different_versions() {
    common::init_tracing();

    // Test with MQTT v3.1.1
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

        let result = endpoint.set_auto_pub_response(true).await;
        assert!(result.is_ok(), "v3.1.1: set_auto_pub_response failed");

        let result = endpoint.set_auto_ping_response(true).await;
        assert!(result.is_ok(), "v3.1.1: set_auto_ping_response failed");

        // Topic alias is v5.0 feature, but setting should not fail
        let result = endpoint.set_auto_map_topic_alias_send(true).await;
        assert!(
            result.is_ok(),
            "v3.1.1: set_auto_map_topic_alias_send should not fail"
        );

        let result = endpoint.set_auto_replace_topic_alias_send(true).await;
        assert!(
            result.is_ok(),
            "v3.1.1: set_auto_replace_topic_alias_send should not fail"
        );
    }

    // Test with MQTT v5.0
    {
        let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

        let result = endpoint.set_auto_pub_response(true).await;
        assert!(result.is_ok(), "v5.0: set_auto_pub_response failed");

        let result = endpoint.set_auto_ping_response(true).await;
        assert!(result.is_ok(), "v5.0: set_auto_ping_response failed");

        let result = endpoint.set_auto_map_topic_alias_send(true).await;
        assert!(result.is_ok(), "v5.0: set_auto_map_topic_alias_send failed");

        let result = endpoint.set_auto_replace_topic_alias_send(true).await;
        assert!(
            result.is_ok(),
            "v5.0: set_auto_replace_topic_alias_send failed"
        );
    }
}

#[tokio::test]
async fn test_set_auto_methods_toggle() {
    common::init_tracing();
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Toggle auto_pub_response multiple times
    for enabled in [false, true, false, true, false] {
        let result = endpoint.set_auto_pub_response(enabled).await;
        assert!(
            result.is_ok(),
            "set_auto_pub_response({enabled}) failed: {result:?}"
        );
    }

    // Toggle auto_ping_response multiple times
    for enabled in [false, true, false, true, false] {
        let result = endpoint.set_auto_ping_response(enabled).await;
        assert!(
            result.is_ok(),
            "set_auto_ping_response({enabled}) failed: {result:?}"
        );
    }

    // Toggle auto_map_topic_alias_send multiple times
    for enabled in [true, false, true, false, true] {
        let result = endpoint.set_auto_map_topic_alias_send(enabled).await;
        assert!(
            result.is_ok(),
            "set_auto_map_topic_alias_send({enabled}) failed: {result:?}"
        );
    }

    // Toggle auto_replace_topic_alias_send multiple times
    for enabled in [true, false, true, false, true] {
        let result = endpoint.set_auto_replace_topic_alias_send(enabled).await;
        assert!(
            result.is_ok(),
            "set_auto_replace_topic_alias_send({enabled}) failed: {result:?}"
        );
    }
}
