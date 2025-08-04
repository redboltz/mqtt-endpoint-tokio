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
use crate::mqtt_ep::packet::{GenericPacket, IsPacketId, PacketType};

/// MQTT Packet Filter for Selective Packet Reception
///
/// This enum provides filtering capabilities for MQTT packet reception, allowing applications
/// to selectively receive only specific types of packets while automatically discarding others.
/// When used with [`GenericEndpoint::recv_filtered()`], packets that don't match the filter
/// are automatically discarded, and the endpoint continues waiting for the next packet until
/// a matching packet is received or an error occurs.
///
/// # Filtering Behavior
///
/// - **Include Mode**: Only packets of the specified types are accepted, all others are discarded
/// - **Exclude Mode**: Packets of the specified types are discarded, all others are accepted
/// - **Any Mode**: All packets are accepted (equivalent to no filtering)
///
/// # Automatic Filtering Process
///
/// When a filter is applied via `recv_filtered()`, the following process occurs:
/// 1. A packet is received from the transport
/// 2. The packet type is checked against the filter criteria
/// 3. If the packet matches the filter, it is returned to the caller
/// 4. If the packet does not match, it is discarded and the endpoint automatically
///    waits for the next packet
/// 5. This process continues until a matching packet is found or an error occurs
///
/// # Supported Packet Types
///
/// The filter supports all MQTT packet types as defined in [`PacketType`]:
/// - `Connect`, `Connack` - Connection establishment
/// - `Publish`, `Puback`, `Pubrec`, `Pubrel`, `Pubcomp` - Message publishing (QoS 0/1/2)
/// - `Subscribe`, `Suback`, `Unsubscribe`, `Unsuback` - Subscription management
/// - `Pingreq`, `Pingresp` - Keep-alive mechanism
/// - `Disconnect` - Connection termination
/// - `Auth` - Authentication exchange (MQTT v5.0 only)
///
/// # Examples
///
/// ```ignore
/// use mqtt_endpoint_tokio::mqtt_ep::{PacketFilter, PacketType};
///
/// // Accept only PUBLISH packets
/// let publish_filter = PacketFilter::include(vec![PacketType::Publish]);
///
/// // Accept only connection-related packets
/// let connection_filter = PacketFilter::include(vec![
///     PacketType::Connect,
///     PacketType::Connack,
///     PacketType::Disconnect,
/// ]);
///
/// // Exclude keep-alive packets (accept all others)
/// let no_ping_filter = PacketFilter::exclude(vec![
///     PacketType::Pingreq,
///     PacketType::Pingresp,
/// ]);
///
/// // Accept all packets (no filtering)
/// let any_filter = PacketFilter::Any;
///
/// // Use with endpoint
/// let packet = endpoint.recv_filtered(publish_filter).await?;
/// ```
///
/// # Performance Considerations
///
/// - Filtered packets are discarded at the protocol level, reducing memory usage
/// - The filtering process is synchronous and adds minimal overhead
/// - Applications should use specific filters rather than `Any` when possible
///   to avoid processing unwanted packets
///
/// [`GenericEndpoint::recv_filtered()`]: crate::mqtt_ep::endpoint::GenericEndpoint::recv_filtered
/// [`PacketType`]: crate::mqtt_ep::packet::PacketType
#[derive(Debug, Clone)]
pub enum PacketFilter {
    /// Accept only packets of the specified types
    ///
    /// All other packet types will be automatically discarded, and the endpoint
    /// will continue waiting for matching packets.
    ///
    /// # Example
    /// ```ignore
    /// // Only accept PUBLISH and SUBSCRIBE packets
    /// let filter = PacketFilter::Include(vec![
    ///     PacketType::Publish,
    ///     PacketType::Subscribe,
    /// ]);
    /// ```
    Include(Vec<PacketType>),

    /// Reject packets of the specified types, accept all others
    ///
    /// Packets of the specified types will be automatically discarded,
    /// while all other packet types will be accepted.
    ///
    /// # Example
    /// ```ignore
    /// // Reject ping packets, accept everything else
    /// let filter = PacketFilter::Exclude(vec![
    ///     PacketType::Pingreq,
    ///     PacketType::Pingresp,
    /// ]);
    /// ```
    Exclude(Vec<PacketType>),

    /// Accept all packets without filtering
    ///
    /// This is equivalent to not applying any filter and is the default
    /// behavior used by [`GenericEndpoint::recv()`].
    ///
    /// [`GenericEndpoint::recv()`]: crate::mqtt_ep::endpoint::GenericEndpoint::recv
    Any,
}

impl PacketFilter {
    /// Check if a packet matches this filter criteria
    ///
    /// This method determines whether a given packet should be accepted or rejected
    /// based on the filter configuration. It's used internally by the endpoint's
    /// filtering mechanism to decide packet acceptance.
    ///
    /// # Arguments
    ///
    /// * `packet` - The MQTT packet to test against the filter
    ///
    /// # Returns
    ///
    /// * `true` - The packet matches the filter and should be accepted
    /// * `false` - The packet does not match the filter and should be discarded
    ///
    /// # Filter Logic
    ///
    /// - **Include**: Returns `true` only if the packet type is in the include list
    /// - **Exclude**: Returns `true` unless the packet type is in the exclude list
    /// - **Any**: Always returns `true` (accepts all packets)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep::{PacketFilter, PacketType};
    ///
    /// let filter = PacketFilter::include(vec![PacketType::Publish]);
    ///
    /// // Assuming we have a publish packet
    /// if filter.matches(&some_packet) {
    ///     println!("Packet accepted by filter");
    /// } else {
    ///     println!("Packet rejected by filter");
    /// }
    /// ```
    pub fn matches<PacketIdType>(&self, packet: &GenericPacket<PacketIdType>) -> bool
    where
        PacketIdType: IsPacketId,
    {
        match self {
            PacketFilter::Include(types) => types.contains(&packet.packet_type()),
            PacketFilter::Exclude(types) => !types.contains(&packet.packet_type()),
            PacketFilter::Any => true,
        }
    }

    /// Create an include filter for specific packet types
    ///
    /// Creates a filter that accepts only packets of the specified types.
    /// All other packet types will be automatically discarded when this filter
    /// is used with [`GenericEndpoint::recv_filtered()`].
    ///
    /// # Arguments
    ///
    /// * `types` - A collection of packet types to include. Can be a `Vec<PacketType>`,
    ///   array, or any type that implements `Into<Vec<PacketType>>`
    ///
    /// # Returns
    ///
    /// A `PacketFilter::Include` variant configured with the specified packet types
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep::{PacketFilter, PacketType};
    ///
    /// // Accept only PUBLISH packets
    /// let publish_only = PacketFilter::include(vec![PacketType::Publish]);
    ///
    /// // Accept connection establishment packets
    /// let connection_packets = PacketFilter::include([
    ///     PacketType::Connect,
    ///     PacketType::Connack,
    /// ]);
    ///
    /// // Accept QoS 1 publish flow packets
    /// let qos1_flow = PacketFilter::include(vec![
    ///     PacketType::Publish,
    ///     PacketType::Puback,
    /// ]);
    /// ```
    ///
    /// # Usage with recv_filtered
    ///
    /// ```ignore
    /// let filter = PacketFilter::include(vec![PacketType::Connack]);
    /// let connack = endpoint.recv_filtered(filter).await?;
    /// ```
    ///
    /// [`GenericEndpoint::recv_filtered()`]: crate::mqtt_ep::endpoint::GenericEndpoint::recv_filtered
    pub fn include(types: impl Into<Vec<PacketType>>) -> Self {
        PacketFilter::Include(types.into())
    }

    /// Create an exclude filter for specific packet types
    ///
    /// Creates a filter that rejects packets of the specified types while accepting
    /// all others. When this filter is used with [`GenericEndpoint::recv_filtered()`],
    /// packets of the excluded types will be automatically discarded.
    ///
    /// # Arguments
    ///
    /// * `types` - A collection of packet types to exclude. Can be a `Vec<PacketType>`,
    ///   array, or any type that implements `Into<Vec<PacketType>>`
    ///
    /// # Returns
    ///
    /// A `PacketFilter::Exclude` variant configured with the specified packet types
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep::{PacketFilter, PacketType};
    ///
    /// // Exclude keep-alive packets
    /// let no_ping = PacketFilter::exclude(vec![
    ///     PacketType::Pingreq,
    ///     PacketType::Pingresp,
    /// ]);
    ///
    /// // Exclude connection packets (useful for established connections)
    /// let no_connection = PacketFilter::exclude([
    ///     PacketType::Connect,
    ///     PacketType::Connack,
    ///     PacketType::Disconnect,
    /// ]);
    ///
    /// // Exclude acknowledgment packets
    /// let no_acks = PacketFilter::exclude(vec![
    ///     PacketType::Puback,
    ///     PacketType::Pubrec,
    ///     PacketType::Pubrel,
    ///     PacketType::Pubcomp,
    ///     PacketType::Suback,
    ///     PacketType::Unsuback,
    /// ]);
    /// ```
    ///
    /// # Usage with recv_filtered
    ///
    /// ```ignore
    /// let filter = PacketFilter::exclude(vec![PacketType::Pingreq, PacketType::Pingresp]);
    /// let packet = endpoint.recv_filtered(filter).await?; // Will not return ping packets
    /// ```
    ///
    /// [`GenericEndpoint::recv_filtered()`]: crate::mqtt_ep::endpoint::GenericEndpoint::recv_filtered
    pub fn exclude(types: impl Into<Vec<PacketType>>) -> Self {
        PacketFilter::Exclude(types.into())
    }
}
