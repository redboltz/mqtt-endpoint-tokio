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

use mqtt_endpoint_tokio::mqtt_ep::connection_error::ConnectionError;
use mqtt_endpoint_tokio::mqtt_ep::result_code::MqttError;
use mqtt_endpoint_tokio::mqtt_ep::transport::TransportError;
use std::error::Error;

/// Test ConnectionError::Mqtt variant creation and properties
#[test]
fn test_connection_error_mqtt_variant() {
    // Create a mock MQTT error
    let mqtt_err = MqttError::MalformedPacket;
    let conn_err = ConnectionError::Mqtt(mqtt_err);

    // Test Debug format
    let debug_str = format!("{:?}", conn_err);
    assert!(debug_str.contains("Mqtt"));
    assert!(debug_str.contains("MalformedPacket"));

    // Test Display format
    let display_str = format!("{}", conn_err);
    assert!(display_str.contains("MQTT protocol error"));
    assert!(display_str.contains("MalformedPacket"));

    // Test Error trait implementation
    assert!(conn_err.source().is_none());
}

/// Test ConnectionError::Transport variant creation and properties
#[test]
fn test_connection_error_transport_variant() {
    let transport_err = TransportError::Timeout;
    let conn_err = ConnectionError::Transport(transport_err);

    // Test Debug format
    let debug_str = format!("{:?}", conn_err);
    assert!(debug_str.contains("Transport"));
    assert!(debug_str.contains("Timeout"));

    // Test Display format
    let display_str = format!("{}", conn_err);
    assert!(display_str.contains("Transport error"));
    assert!(display_str.contains("Operation timed out"));

    // Test Error trait implementation
    assert!(conn_err.source().is_none());
}

/// Test ConnectionError::ChannelClosed variant
#[test]
fn test_connection_error_channel_closed_variant() {
    let conn_err = ConnectionError::ChannelClosed;

    // Test Debug format
    let debug_str = format!("{:?}", conn_err);
    assert_eq!(debug_str, "ChannelClosed");

    // Test Display format
    let display_str = format!("{}", conn_err);
    assert_eq!(display_str, "Internal channel closed");

    // Test Error trait implementation
    assert!(conn_err.source().is_none());
}

/// Test ConnectionError::NotConnected variant
#[test]
fn test_connection_error_not_connected_variant() {
    let conn_err = ConnectionError::NotConnected;

    // Test Debug format
    let debug_str = format!("{:?}", conn_err);
    assert_eq!(debug_str, "NotConnected");

    // Test Display format
    let display_str = format!("{}", conn_err);
    assert_eq!(display_str, "Not connected");

    // Test Error trait implementation
    assert!(conn_err.source().is_none());
}

/// Test ConnectionError::AlreadyConnected variant
#[test]
fn test_connection_error_already_connected_variant() {
    let conn_err = ConnectionError::AlreadyConnected;

    // Test Debug format
    let debug_str = format!("{:?}", conn_err);
    assert_eq!(debug_str, "AlreadyConnected");

    // Test Display format
    let display_str = format!("{}", conn_err);
    assert_eq!(display_str, "Already connected");

    // Test Error trait implementation
    assert!(conn_err.source().is_none());
}

/// Test ConnectionError::AlreadyDisconnected variant
#[test]
fn test_connection_error_already_disconnected_variant() {
    let conn_err = ConnectionError::AlreadyDisconnected;

    // Test Debug format
    let debug_str = format!("{:?}", conn_err);
    assert_eq!(debug_str, "AlreadyDisconnected");

    // Test Display format
    let display_str = format!("{}", conn_err);
    assert_eq!(display_str, "Already disconnected");

    // Test Error trait implementation
    assert!(conn_err.source().is_none());
}

/// Test From<MqttError> for ConnectionError conversion
#[test]
fn test_connection_error_from_mqtt_error() {
    let mqtt_err = MqttError::MalformedPacket;
    let conn_err: ConnectionError = mqtt_err.into();

    // Verify it's the Mqtt variant
    match conn_err {
        ConnectionError::Mqtt(MqttError::MalformedPacket) => {}
        _ => panic!("Expected ConnectionError::Mqtt(MqttError::MalformedPacket)"),
    }

    // Test with different MqttError variant
    let mqtt_err2 = MqttError::ProtocolError;
    let conn_err2 = ConnectionError::from(mqtt_err2);

    match conn_err2 {
        ConnectionError::Mqtt(MqttError::ProtocolError) => {}
        _ => panic!("Expected ConnectionError::Mqtt(MqttError::ProtocolError)"),
    }
}

/// Test From<TransportError> for ConnectionError conversion
#[test]
fn test_connection_error_from_transport_error() {
    let transport_err = TransportError::Timeout;
    let conn_err: ConnectionError = transport_err.into();

    // Verify it's the Transport variant
    match conn_err {
        ConnectionError::Transport(TransportError::Timeout) => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::Timeout)"),
    }

    // Test with different TransportError variant
    let transport_err2 = TransportError::NotConnected;
    let conn_err2 = ConnectionError::from(transport_err2);

    match conn_err2 {
        ConnectionError::Transport(TransportError::NotConnected) => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::NotConnected)"),
    }
}

/// Test all TransportError variants conversion
#[test]
fn test_all_transport_error_variants() {
    // Test IO error
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "Connection refused");
    let transport_err = TransportError::Io(io_err);
    let conn_err = ConnectionError::from(transport_err);
    match conn_err {
        ConnectionError::Transport(TransportError::Io(_)) => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::Io(_))"),
    }

    // Test TLS error
    let tls_err: Box<dyn std::error::Error + Send + Sync> = "TLS handshake failed".into();
    let transport_err = TransportError::Tls(tls_err);
    let conn_err = ConnectionError::from(transport_err);
    match conn_err {
        ConnectionError::Transport(TransportError::Tls(_)) => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::Tls(_))"),
    }

    // Test WebSocket error
    let ws_err: Box<dyn std::error::Error + Send + Sync> = "WebSocket upgrade failed".into();
    let transport_err = TransportError::WebSocket(ws_err);
    let conn_err = ConnectionError::from(transport_err);
    match conn_err {
        ConnectionError::Transport(TransportError::WebSocket(_)) => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::WebSocket(_))"),
    }

    // Test Timeout
    let transport_err = TransportError::Timeout;
    let conn_err = ConnectionError::from(transport_err);
    match conn_err {
        ConnectionError::Transport(TransportError::Timeout) => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::Timeout)"),
    }

    // Test Connect error
    let transport_err = TransportError::Connect("Failed to connect to broker".to_string());
    let conn_err = ConnectionError::from(transport_err);
    match conn_err {
        ConnectionError::Transport(TransportError::Connect(msg))
            if msg == "Failed to connect to broker" => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::Connect(_))"),
    }

    // Test NotConnected
    let transport_err = TransportError::NotConnected;
    let conn_err = ConnectionError::from(transport_err);
    match conn_err {
        ConnectionError::Transport(TransportError::NotConnected) => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::NotConnected)"),
    }
}

/// Test various MqttError variants conversion
#[test]
fn test_various_mqtt_error_variants() {
    // Test MalformedPacket
    let mqtt_err = MqttError::MalformedPacket;
    let conn_err = ConnectionError::from(mqtt_err);
    match conn_err {
        ConnectionError::Mqtt(MqttError::MalformedPacket) => {}
        _ => panic!("Expected ConnectionError::Mqtt(MqttError::MalformedPacket)"),
    }

    // Test ProtocolError
    let mqtt_err = MqttError::ProtocolError;
    let conn_err = ConnectionError::from(mqtt_err);
    match conn_err {
        ConnectionError::Mqtt(MqttError::ProtocolError) => {}
        _ => panic!("Expected ConnectionError::Mqtt(MqttError::ProtocolError)"),
    }

    // Test UnspecifiedError
    let mqtt_err = MqttError::UnspecifiedError;
    let conn_err = ConnectionError::from(mqtt_err);
    match conn_err {
        ConnectionError::Mqtt(MqttError::UnspecifiedError) => {}
        _ => panic!("Expected ConnectionError::Mqtt(MqttError::UnspecifiedError)"),
    }

    // Test ImplementationSpecificError
    let mqtt_err = MqttError::ImplementationSpecificError;
    let conn_err = ConnectionError::from(mqtt_err);
    match conn_err {
        ConnectionError::Mqtt(MqttError::ImplementationSpecificError) => {}
        _ => panic!("Expected ConnectionError::Mqtt(MqttError::ImplementationSpecificError)"),
    }
}

/// Test error chaining and debugging support
#[test]
fn test_error_trait_implementation() {
    // Test that ConnectionError implements Error trait properly
    let conn_err = ConnectionError::NotConnected;

    // Should implement Error trait
    let _: &dyn Error = &conn_err;

    // source() should return None for state errors
    assert!(conn_err.source().is_none());

    // Test with nested errors
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Broken pipe");
    let transport_err = TransportError::Io(io_err);
    let conn_err = ConnectionError::Transport(transport_err);

    // Should still implement Error trait
    let _: &dyn Error = &conn_err;
    assert!(conn_err.source().is_none()); // ConnectionError doesn't expose inner sources
}

/// Test Display formatting for different error combinations
#[test]
fn test_display_formatting() {
    // Test with IO error
    let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "Operation timed out");
    let transport_err = TransportError::Io(io_err);
    let conn_err = ConnectionError::Transport(transport_err);
    let display_str = format!("{}", conn_err);
    assert!(display_str.contains("Transport error"));
    assert!(display_str.contains("IO error"));

    // Test with Connect error
    let transport_err = TransportError::Connect("DNS resolution failed".to_string());
    let conn_err = ConnectionError::Transport(transport_err);
    let display_str = format!("{}", conn_err);
    assert!(display_str.contains("Transport error"));
    assert!(display_str.contains("Connection failed"));
    assert!(display_str.contains("DNS resolution failed"));

    // Test with MQTT error
    let mqtt_err = MqttError::ProtocolError;
    let conn_err = ConnectionError::Mqtt(mqtt_err);
    let display_str = format!("{}", conn_err);
    assert!(display_str.contains("MQTT protocol error"));
    assert!(display_str.contains("ProtocolError"));
}

/// Test Debug formatting comprehensively
#[test]
fn test_debug_formatting() {
    // Test each variant's debug output
    let variants = vec![
        ConnectionError::ChannelClosed,
        ConnectionError::NotConnected,
        ConnectionError::AlreadyConnected,
        ConnectionError::AlreadyDisconnected,
        ConnectionError::Mqtt(MqttError::MalformedPacket),
        ConnectionError::Transport(TransportError::Timeout),
    ];

    for variant in variants {
        let debug_str = format!("{:?}", variant);
        // Debug should not be empty and should contain variant name
        assert!(!debug_str.is_empty());

        match variant {
            ConnectionError::ChannelClosed => assert!(debug_str.contains("ChannelClosed")),
            ConnectionError::NotConnected => assert!(debug_str.contains("NotConnected")),
            ConnectionError::AlreadyConnected => assert!(debug_str.contains("AlreadyConnected")),
            ConnectionError::AlreadyDisconnected => {
                assert!(debug_str.contains("AlreadyDisconnected"))
            }
            ConnectionError::Mqtt(_) => assert!(debug_str.contains("Mqtt")),
            ConnectionError::Transport(_) => assert!(debug_str.contains("Transport")),
        }
    }
}

/// Test error conversion through ? operator simulation
#[test]
fn test_question_mark_operator_conversion() {
    // Simulate function that returns Result with ? operator
    fn simulate_mqtt_operation() -> Result<(), ConnectionError> {
        let mqtt_err = MqttError::UnspecifiedError;
        Err(mqtt_err)?; // This should convert automatically
        Ok(())
    }

    fn simulate_transport_operation() -> Result<(), ConnectionError> {
        let transport_err = TransportError::Timeout;
        Err(transport_err)?; // This should convert automatically
        Ok(())
    }

    // Test MQTT error conversion
    let result = simulate_mqtt_operation();
    assert!(result.is_err());
    match result.unwrap_err() {
        ConnectionError::Mqtt(MqttError::UnspecifiedError) => {}
        _ => panic!("Expected ConnectionError::Mqtt(MqttError::UnspecifiedError)"),
    }

    // Test Transport error conversion
    let result = simulate_transport_operation();
    assert!(result.is_err());
    match result.unwrap_err() {
        ConnectionError::Transport(TransportError::Timeout) => {}
        _ => panic!("Expected ConnectionError::Transport(TransportError::Timeout)"),
    }
}
