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
// Simple MQTT Publisher Example
//
// This is a simplified example that demonstrates basic MQTT endpoint usage.
//
// Usage:
// ```bash
// cargo run --example simple_publisher -- <hostname> <port> <topic> <qos> <payload>
// ```
//
// Example:
// ```bash
// cargo run --example simple_publisher -- localhost 1883 "test/topic" 1 "Hello, MQTT!"
// ```
use std::env;
use std::process;

use mqtt_endpoint_tokio::mqtt_ep;

type ClientEndpoint = mqtt_ep::GenericEndpoint<mqtt_ep::role::Client, u16>;

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    let (hostname, port, topic, qos, payload) = if args.len() != 6 {
        eprintln!(
            "Usage: {} <hostname> <port> <topic> <qos> <payload>",
            args[0]
        );
        eprintln!(
            "Example: {} localhost 1883 \"test/topic\" 1 \"Hello, MQTT!\"",
            args[0]
        );
        eprintln!();
        eprintln!("Using default values: 127.0.0.1 1883 t1 1 hello");
        eprintln!();
        (
            "127.0.0.1".to_string(),
            1883u16,
            "t1".to_string(),
            1u8,
            "hello".to_string(),
        )
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
        let payload = args[5].clone();
        (hostname, port, topic, qos, payload)
    };

    let qos_level = mqtt_ep::packet::Qos::try_from(qos)
        .expect("Error: Invalid QoS level '{qos}'. Must be 0, 1, or 2");

    println!("Simple MQTT Publisher");
    println!("Broker: {hostname}:{port}");
    println!("Topic: {topic}");
    println!("QoS: {qos_level:?}");
    println!("Payload: {payload}");

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
        .client_id("rust_publisher")
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

    // Create PUBLISH packet
    let publish_builder = mqtt_ep::packet::v3_1_1::Publish::builder()
        .topic_name(topic)
        .unwrap()
        .qos(qos_level)
        .retain(false)
        .payload(payload.as_bytes());

    // Add packet ID for QoS 1 and 211
    let publish_packet = if qos_level != mqtt_ep::packet::Qos::AtMostOnce {
        let packet_id = match endpoint.acquire_packet_id().await {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Error: Failed to acquire packet ID: {e:?}");
                process::exit(1);
            }
        };
        publish_builder.packet_id(packet_id).build().unwrap()
    } else {
        publish_builder.build().unwrap()
    };

    println!("Publishing message... {publish_packet}");
    if let Err(e) = endpoint.send(publish_packet.clone()).await {
        eprintln!("Error: Failed to send PUBLISH packet: {e:?}");
        process::exit(1);
    }

    // Handle QoS acknowledgements
    match qos_level {
        mqtt_ep::packet::Qos::AtMostOnce => {
            println!("Message published successfully (QoS 0 - fire and forget)");
        }
        mqtt_ep::packet::Qos::AtLeastOnce => {
            // Wait for PUBACK
            println!("Waiting for PUBACK...");
            match endpoint.recv().await {
                Ok(packet) => match packet {
                    mqtt_ep::packet::GenericPacket::V3_1_1Puback(puback) => {
                        println!(
                            "Message published successfully (QoS 1 - received PUBACK for packet ID {})",
                            puback.packet_id()
                        );
                    }
                    _ => {
                        eprintln!(
                            "Error: Expected PUBACK, received: {:?}",
                            packet.packet_type()
                        );
                        process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("Error: Failed to receive PUBACK: {e:?}");
                    process::exit(1);
                }
            }
        }
        mqtt_ep::packet::Qos::ExactlyOnce => {
            // Wait for PUBREC
            println!("Waiting for PUBREC...");
            match endpoint.recv().await {
                Ok(packet) => {
                    match packet {
                        mqtt_ep::packet::GenericPacket::V3_1_1Pubrec(pubrec) => {
                            println!("Received PUBREC for packet ID {}", pubrec.packet_id());

                            // Send PUBREL
                            let pubrel = mqtt_ep::packet::v3_1_1::Pubrel::builder()
                                .packet_id(pubrec.packet_id())
                                .build()
                                .unwrap();
                            if let Err(e) = endpoint.send(pubrel).await {
                                eprintln!("Error: Failed to send PUBREL: {e:?}");
                                process::exit(1);
                            }

                            // Wait for PUBCOMP
                            println!("Waiting for PUBCOMP...");
                            match endpoint.recv().await {
                                Ok(packet) => match packet {
                                    mqtt_ep::packet::GenericPacket::V3_1_1Pubcomp(pubcomp) => {
                                        println!(
                                            "Message published successfully (QoS 2 - received PUBCOMP for packet ID {})",
                                            pubcomp.packet_id()
                                        );
                                    }
                                    _ => {
                                        eprintln!(
                                            "Error: Expected PUBCOMP, received: {:?}",
                                            packet.packet_type()
                                        );
                                        process::exit(1);
                                    }
                                },
                                Err(e) => {
                                    eprintln!("Error: Failed to receive PUBCOMP: {e:?}");
                                    process::exit(1);
                                }
                            }
                        }
                        _ => {
                            eprintln!(
                                "Error: Expected PUBREC, received: {:?}",
                                packet.packet_type()
                            );
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: Failed to receive PUBREC: {e:?}");
                    process::exit(1);
                }
            }
        }
    }

    // Send DISCONNECT packet (good practice)
    println!("Sending DISCONNECT packet...");
    let disconnect_packet = mqtt_ep::packet::v3_1_1::Disconnect::new();
    if let Err(e) = endpoint.send(disconnect_packet).await {
        eprintln!("Warning: Failed to send DISCONNECT packet: {e:?}");
    }

    // Close connection gracefully
    if let Err(e) = endpoint.close().await {
        eprintln!("Warning: Failed to close connection properly: {e:?}");
    }

    println!("Publisher finished successfully.");
}
