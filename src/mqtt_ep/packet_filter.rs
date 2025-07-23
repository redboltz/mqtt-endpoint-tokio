use mqtt_protocol_core::mqtt::packet::{GenericPacket, PacketType};
use mqtt_protocol_core::mqtt::types::IsPacketId;
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
use serde::Serialize;

/// Packet filter for selective receiving
#[derive(Debug, Clone)]
pub enum PacketFilter {
    /// Accept packets of any of these types
    Include(Vec<PacketType>),
    /// Reject packets of any of these types (accept all others)
    Exclude(Vec<PacketType>),
    /// Accept all packets (no filtering)
    Any,
}

impl PacketFilter {
    /// Check if a packet matches this filter
    pub fn matches<PacketIdType>(&self, packet: &GenericPacket<PacketIdType>) -> bool
    where
        PacketIdType: IsPacketId + Serialize,
    {
        match self {
            PacketFilter::Include(types) => types.contains(&packet.packet_type()),
            PacketFilter::Exclude(types) => !types.contains(&packet.packet_type()),
            PacketFilter::Any => true,
        }
    }

    /// Create an include filter for specific packet types
    pub fn include(types: impl Into<Vec<PacketType>>) -> Self {
        PacketFilter::Include(types.into())
    }

    /// Create an exclude filter for specific packet types
    pub fn exclude(types: impl Into<Vec<PacketType>>) -> Self {
        PacketFilter::Exclude(types.into())
    }
}
