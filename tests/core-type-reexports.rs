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

#[test]
fn test_mqtt_string_reexport() {
    let s = mqtt_ep::packet::MqttString::new("test").unwrap();
    assert_eq!(s.as_str(), "test");
}

#[test]
fn test_mqtt_binary_reexport() {
    let b = mqtt_ep::packet::MqttBinary::new(b"test").unwrap();
    assert_eq!(b.as_slice(), b"test");
}

#[test]
fn test_arc_payload_reexport() {
    let p: mqtt_ep::common::ArcPayload = mqtt_ep::common::IntoPayload::into_payload("test");
    assert_eq!(p.as_slice(), b"test");
}

#[test]
fn test_into_payload_trait_reexport() {
    fn accept_payload<T: mqtt_ep::common::IntoPayload>(t: T) -> mqtt_ep::common::ArcPayload {
        t.into_payload()
    }
    let p = accept_payload("test");
    assert_eq!(p.as_slice(), b"test");
}

#[test]
fn test_retain_handling_reexport() {
    let rh = mqtt_ep::packet::RetainHandling::SendRetained;
    assert_eq!(rh as u8, 0);
}

#[test]
fn test_payload_format_reexport() {
    let pf = mqtt_ep::packet::PayloadFormat::Binary;
    assert_eq!(pf as u8, 0);
}

#[test]
fn test_variable_byte_integer_reexport() {
    let vbi = mqtt_ep::packet::VariableByteInteger::from_u32(100).unwrap();
    assert_eq!(vbi.to_u32(), 100);
}
