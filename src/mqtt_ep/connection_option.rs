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

/// Connection options that can be set dynamically for each connection/reconnection
#[derive(Debug, Clone)]
pub struct ConnectionOption {
    /// PINGREQ send interval in milliseconds (for v3.1.1 and v5.0)
    pub pingreq_send_interval: Option<u64>,
    /// Keep alive interval in seconds (for v3.1.1 and v5.0)
    pub keep_alive_interval: Option<u16>,
    /// Packet ID exhaust retry interval in milliseconds
    pub packet_id_exhaust_retry_interval: Option<u64>,
}

impl Default for ConnectionOption {
    fn default() -> Self {
        Self {
            pingreq_send_interval: None,
            keep_alive_interval: None,
            packet_id_exhaust_retry_interval: None,
        }
    }
}

impl ConnectionOption {
    /// Create a new ConnectionOption with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the PINGREQ send interval in milliseconds
    pub fn pingreq_send_interval(mut self, interval: u64) -> Self {
        self.pingreq_send_interval = Some(interval);
        self
    }

    /// Set the keep alive interval in seconds
    pub fn keep_alive_interval(mut self, interval: u16) -> Self {
        self.keep_alive_interval = Some(interval);
        self
    }

    /// Set the packet ID exhaust retry interval in milliseconds
    pub fn packet_id_exhaust_retry_interval(mut self, interval: u64) -> Self {
        self.packet_id_exhaust_retry_interval = Some(interval);
        self
    }
}
