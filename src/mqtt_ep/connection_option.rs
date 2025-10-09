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

use derive_builder::Builder;
use getset::{CopyGetters, Getters};

/// MQTT Connection Options - Configuration for MQTT endpoint connections
///
/// This struct contains configuration options that can be set dynamically for each
/// connection or reconnection attempt. It provides fine-grained control over various
/// aspects of MQTT protocol behavior and connection management.
///
/// # Key Features
///
/// - **Dynamic Configuration**: Options can be changed between connections
/// - **Protocol Behavior Control**: Configure automatic responses and protocol features
/// - **Timeout Management**: Set various timeout values for different operations
/// - **State Restoration**: Support for restoring connection state after reconnection
///
/// # Usage
///
/// ```ignore
/// use mqtt_endpoint_tokio::mqtt_ep::ConnectionOption;
///
/// let options = ConnectionOption::builder()
///     .pingreq_send_interval_ms(30000)
///     .auto_pub_response(true)
///     .connection_establish_timeout_ms(10000)
///     .build()
///     .unwrap();
/// ```
#[derive(Debug, Clone, Builder, Getters, CopyGetters)]
#[builder(derive(Debug), pattern = "owned", setter(into))]
pub struct ConnectionOption {
    /// PINGREQ send interval override in milliseconds
    ///
    /// Overrides the interval for sending PINGREQ packets to maintain the connection alive.
    /// PINGREQ is sent after the interval has elapsed since the last packet of any type was sent.
    ///
    /// The send interval is determined by the following priority order:
    /// 1. User setting by this option
    /// 2. ServerKeepAlive property value in received CONNACK packet
    /// 3. KeepAlive value in sent CONNECT packet
    ///
    /// # Values
    ///
    /// * `Some(value)` - Override with specified milliseconds. If 0, no PINGREQ is sent.
    /// * `None` - Do not override, use CONNACK ServerKeepAlive or CONNECT KeepAlive instead.
    ///
    /// # Default
    /// None (no override)
    #[builder(default = "None", setter(into, strip_option))]
    #[getset(get = "pub")]
    pingreq_send_interval_ms: Option<u64>,

    /// Enable automatic PUBLISH response handling
    ///
    /// When enabled, the endpoint automatically sends PUBACK, PUBREC, and PUBCOMP
    /// responses for received PUBLISH packets according to their QoS level.
    ///
    /// # Default
    /// true
    #[builder(default = "true", setter(into, strip_option))]
    #[getset(get = "pub")]
    auto_pub_response: bool,

    /// Enable automatic PING response handling
    ///
    /// When enabled, the endpoint automatically responds to PINGREQ packets
    /// with PINGRESP packets.
    ///
    /// # Default
    /// true
    #[builder(default = "true", setter(into, strip_option))]
    #[getset(get = "pub")]
    auto_ping_response: bool,

    /// Enable automatic topic alias mapping for outgoing messages
    ///
    /// When enabled, the endpoint automatically maps frequently used topics
    /// to topic aliases in outgoing PUBLISH packets to reduce bandwidth usage.
    /// Only available in MQTT v5.0.
    ///
    /// # Default
    /// false
    #[builder(default = "false", setter(into, strip_option))]
    #[getset(get = "pub")]
    auto_map_topic_alias_send: bool,

    /// Enable automatic topic alias replacement for outgoing messages
    ///
    /// When enabled, the endpoint automatically replaces topic names with
    /// topic aliases in outgoing PUBLISH packets when aliases are available.
    /// Only available in MQTT v5.0.
    ///
    /// # Default
    /// false
    #[builder(default = "false", setter(into, strip_option))]
    #[getset(get = "pub")]
    auto_replace_topic_alias_send: bool,

    /// PING response receive timeout in milliseconds
    ///
    /// Maximum time to wait for a PINGRESP after sending a PINGREQ.
    /// If no response is received within this timeout, the connection is
    /// considered failed. A value of 0 disables the timeout.
    ///
    /// # Default
    /// 0 (disabled)
    #[builder(default = "0", setter(into, strip_option))]
    #[getset(get = "pub")]
    pingresp_recv_timeout_ms: u64,

    /// Connection establishment timeout in milliseconds
    ///
    /// Maximum time to wait for the connection to be established,
    /// including TCP connection and MQTT CONNACK reception.
    /// A value of 0 disables the timeout.
    ///
    /// # Default
    /// 0 (disabled)
    #[builder(default = "0", setter(into, strip_option))]
    #[getset(get = "pub")]
    connection_establish_timeout_ms: u64,

    /// Connection shutdown timeout in milliseconds
    ///
    /// Maximum time to wait for graceful connection shutdown.
    /// After this timeout, the connection will be forcibly closed.
    ///
    /// # Default
    /// 5000 (5 seconds)
    #[builder(default = "5000", setter(into, strip_option))]
    #[getset(get = "pub")]
    shutdown_timeout_ms: u64,

    /// Receive buffer size in bytes
    ///
    /// Size of the buffer used for receiving data from the network.
    /// Larger buffers can improve performance for high-throughput scenarios
    /// but consume more memory.
    ///
    /// # Default
    /// None (maintains current buffer size, initially 4096 bytes)
    #[builder(setter(into, strip_option), default)]
    #[getset(get = "pub")]
    recv_buffer_size: Option<usize>,

    /// Enable queueing when ReceiveMaximum limit is reached
    ///
    /// When the ReceiveMaximum property limit is reached for QoS 1 and QoS 2 PUBLISH packets,
    /// this flag determines whether to queue or return an error. If false, returns an error
    /// without queueing. If true, packets are queued and sent when the limit allows,
    /// returning an asynchronous response.
    ///
    /// # Default
    /// false
    #[builder(default = "false", setter(into, strip_option))]
    #[getset(get = "pub")]
    queuing_receive_maximum: bool,
}

/// Default implementation for ConnectionOption
///
/// Provides sensible defaults for all configuration options that balance
/// functionality with resource usage.
impl Default for ConnectionOption {
    fn default() -> Self {
        Self::builder()
            .auto_pub_response(true)
            .auto_ping_response(true)
            .auto_map_topic_alias_send(false)
            .auto_replace_topic_alias_send(false)
            .pingresp_recv_timeout_ms(0u64)
            .connection_establish_timeout_ms(0u64)
            .shutdown_timeout_ms(5000u64)
            .queuing_receive_maximum(false)
            .build()
            .expect("Default ConnectionOption should be valid")
    }
}

/// Implementation methods for ConnectionOption
impl ConnectionOption {
    /// Create a new builder for ConnectionOption
    ///
    /// Returns a builder that can be used to configure and construct
    /// a ConnectionOption instance with custom settings.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let options = ConnectionOption::builder()
    ///     .pingreq_send_interval_ms(30000)
    ///     .auto_pub_response(true)
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn builder() -> ConnectionOptionBuilder {
        ConnectionOptionBuilder::default()
    }
}
