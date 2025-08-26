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

// Re-export mqtt-protocol-core types
// Core protocol types and traits
pub use mqtt_protocol_core::mqtt::Version;

// Packet module with traits and types
pub mod packet {
    // Essential traits
    pub use mqtt_protocol_core::mqtt::packet::{GenericPacketTrait, IsPacketId};

    // Generic packet types
    pub use mqtt_protocol_core::mqtt::packet::{
        GenericPacket, GenericStorePacket, Packet, PacketType, Property, Qos, SubEntry, SubOpts,
        SubscriptionIdentifier,
    };

    // Version-specific packets
    pub mod v5_0 {
        pub use mqtt_protocol_core::mqtt::packet::v5_0::*;
    }

    pub mod v3_1_1 {
        pub use mqtt_protocol_core::mqtt::packet::v3_1_1::*;
    }
}

pub mod common {
    pub use mqtt_protocol_core::mqtt::common::{HashMap, HashSet};
}

// Role module
pub mod role {
    pub use mqtt_protocol_core::mqtt::role::*;
}

// Result code module
pub mod result_code {
    pub use mqtt_protocol_core::mqtt::result_code::*;
}

// Prelude module
pub mod prelude {
    pub use mqtt_protocol_core::mqtt::prelude::*;
}

// Our own modules
pub mod connection_error;
pub mod connection_option;
pub mod endpoint;
pub mod packet_filter;
pub mod request_response;
pub mod transport;

pub use connection_error::ConnectionError;
pub use connection_option::ConnectionOption;
pub use endpoint::{Endpoint, GenericEndpoint, Mode};
pub use packet_filter::PacketFilter;
pub use transport::{TransportError, TransportOps};
