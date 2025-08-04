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
// Simple MQTT Subscriber Example
//
// This is a simplified example that demonstrates basic MQTT endpoint usage.
//
// Usage:
// ```bash
// cargo run --example simple_subscriber -- <hostname> <port> <topic> <qos>
// ```
//
// Example:
// ```bash
// cargo run --example simple_subscriber -- localhost 1883 "test/topic" 1
// ```
use std::env;
use std::process;

use mqtt_endpoint_tokio::mqtt_ep;

type ClientEndpoint = mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u16>;

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    let (hostname, port, topic, qos) = if args.len() != 5 {
        eprintln!("Usage: {} <hostname> <port> <topic> <qos>", args[0]);
        eprintln!(
            "Example: {} localhost 1883 \"test/topic\" 1 \"Hello, MQTT!\"",
            args[0]
        );
        eprintln!();
        eprintln!("Using default values: 127.0.0.1 1883 t1 2");
        eprintln!();
        ("127.0.0.1".to_string(), 1883u16, "t1".to_string(), 1u8)
    } else {
        let hostname = args[1].clone();
        let port: u16 = args[2].parse().unwrap_or_else(|_| {
            eprintln!("Error: Invalid port number '{}'", args[2]);
            process::exit(1);
        });
        let topic = args[3].clone();
        let qos: u8 = args[4].parse().unwrap_or_else(|_| {
            eprintln!("Error: Invalid QoS level '{}'. Must be 0, 1, or 2", args[4]);
            process::exit(1);
        });
        (hostname, port, topic, qos)
    };

    println!("Simple MQTT Subscriber");
    println!("Broker: {hostname}:{port}");
    println!("Topic: {topic}");
    println!("QoS: {qos}");

    // Create socket address string
    let addr = format!("{hostname}:{port}");

    // Create MQTT endpoint
    let endpoint: ClientEndpoint = mqtt_ep::GenericEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Connect to the broker and create transport
    println!("Connecting to broker...");
    let tcp_stream = match mqtt_ep::transport::connect_helper::connect_tcp(&addr, None).await {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("Error: Failed to connect to broker: {e:?}");
            process::exit(1);
        }
    };

    let transport = mqtt_ep::transport::TcpTransport::from_stream(tcp_stream);

    // Attach the connected transport to the endpoint
    if let Err(e) = endpoint
        .attach(transport, mqtt_endpoint_tokio::mqtt_ep::Mode::Client)
        .await
    {
        eprintln!("Error: Failed to attach transport to endpoint: {e:?}");
        process::exit(1);
    }
    println!("Transport connected and attached successfully!");

    // Send MQTT CONNECT packet
    println!("Sending CONNECT packet...");
    let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("rust_subscriber")
        .unwrap()
        .keep_alive(60)
        .clean_session(true)
        .build()
        .unwrap();

    if let Err(e) = endpoint.send(connect_packet).await {
        eprintln!("Error: Failed to send CONNECT packet: {e:?}");
        process::exit(1);
    }

    // Wait for CONNACK
    println!("Waiting for CONNACK...");
    match endpoint.recv().await {
        Ok(packet) => match packet {
            mqtt_ep::packet::GenericPacket::V3_1_1Connack(connack) => {
                if connack.return_code() != mqtt_ep::result_code::ConnectReturnCode::Accepted {
                    eprintln!("Error: Connection refused: {:?}", connack.return_code());
                    process::exit(1);
                }
                println!("MQTT connection accepted by broker");
            }
            _ => {
                eprintln!(
                    "Error: Expected CONNACK, received: {:?}",
                    packet.packet_type()
                );
                process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Error: Failed to receive CONNACK: {e:?}");
            process::exit(1);
        }
    }

    // Acquire packet ID for SUBSCRIBE
    println!("Acquiring packet ID for SUBSCRIBE...");
    let packet_id = match endpoint.acquire_packet_id().await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Error: Failed to acquire packet ID: {e:?}");
            process::exit(1);
        }
    };

    // Convert QoS
    let qos_level = mqtt_ep::packet::Qos::try_from(qos)
        .expect("Error: Invalid QoS level '{qos}'. Must be 0, 1, or 2");
    // Create SubEntry for the subscription
    let sub_opts = mqtt_ep::packet::SubOpts::new().set_qos(qos_level);
    let sub_entry = match mqtt_ep::packet::SubEntry::new(&topic, sub_opts) {
        Ok(entry) => entry,
        Err(e) => {
            eprintln!("Error: Failed to create subscription entry: {e:?}");
            process::exit(1);
        }
    };

    // Create SUBSCRIBE packet
    println!("Sending SUBSCRIBE packet for topic '{topic}'...");
    let subscribe_packet = mqtt_ep::packet::v3_1_1::Subscribe::builder()
        .packet_id(packet_id)
        .entries(vec![sub_entry])
        .build()
        .unwrap();

    if let Err(e) = endpoint.send(subscribe_packet).await {
        eprintln!("Error: Failed to send SUBSCRIBE packet: {e:?}");
        process::exit(1);
    }

    // Wait for SUBACK
    println!("Waiting for SUBACK...");
    match endpoint.recv().await {
        Ok(packet) => match packet {
            mqtt_ep::packet::GenericPacket::V3_1_1Suback(suback) => {
                println!("Subscription confirmed for topic: {topic}");
                println!("Granted QoS levels: {:?}", suback.return_codes());
            }
            _ => {
                eprintln!(
                    "Error: Expected SUBACK, received: {:?}",
                    packet.packet_type()
                );
                process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Error: Failed to receive SUBACK: {e:?}");
            process::exit(1);
        }
    }

    println!("Successfully subscribed to topic '{topic}' with QoS {qos_level:?}");
    println!("Listening for messages (Press Ctrl+C to exit)...");

    // Simple receive loop to demonstrate the endpoint API
    loop {
        match endpoint.recv().await {
            Ok(packet) => {
                // Handle different packet types
                match packet {
                    mqtt_ep::packet::GenericPacket::V3_1_1Publish(publish) => {
                        println!("Received message:");
                        println!("  Topic: {}", publish.topic_name());
                        println!("  QoS: {:?}", publish.qos());
                        println!("  Retain: {}", publish.retain());
                        println!(
                            "  Payload: {}",
                            String::from_utf8_lossy(publish.payload().as_slice())
                        );
                        println!("  ---");
                    }
                    _ => {
                        // Other packet types
                        println!("Received packet: {:?}", packet.packet_type());
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: Failed to receive packet: {e:?}");
                break;
            }
        }
    }

    // Close connection
    if let Err(e) = endpoint.close().await {
        eprintln!("Warning: Failed to close connection properly: {e:?}");
    }

    println!("Subscriber stopped.");
}
