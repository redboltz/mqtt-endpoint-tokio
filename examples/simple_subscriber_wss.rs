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

// Simple MQTT Subscriber Example over WebSocket Secure (WSS)
//
// This is a simplified example that demonstrates basic MQTT endpoint usage over WSS transport.
//
// Usage:
// ```bash
// cargo run --example simple_subscriber_wss -- <hostname> <port> <path> <topic> <qos> [cacert_pem_file]
// ```
//
// Example:
// ```bash
// # Insecure connection (development only)
// cargo run --example simple_subscriber_wss -- localhost 8084 "/mqtt" "test/topic" 1
//
// # Secure connection with certificate file
// cargo run --example simple_subscriber_wss -- localhost 8084 "/mqtt" "test/topic" 1 /path/to/ca.pem
// ```
use std::env;
use std::process;
use tokio::time::Duration;

use mqtt_endpoint_tokio::mqtt_ep;

fn init_tracing() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let filter = if let Ok(rust_log) = std::env::var("RUST_LOG") {
            tracing_subscriber::EnvFilter::new(rust_log)
        } else {
            let level = std::env::var("MQTT_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string());
            tracing_subscriber::EnvFilter::new(format!("mqtt_endpoint_tokio={level}"))
        };

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .init();
    });
}

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;

#[tokio::main]
async fn main() {
    init_tracing();

    // Initialize crypto provider for rustls
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    let (hostname, port, path, topic, qos, cacert_pem_file) = if args.len() < 6 || args.len() > 7 {
        eprintln!(
            "Usage: {} <hostname> <port> <path> <topic> <qos> [cacert_pem_file]",
            args[0]
        );
        eprintln!(
            "Example: {} localhost 8084 \"/mqtt\" \"test/topic\" 1",
            args[0]
        );
        eprintln!(
            "Example: {} localhost 8084 \"/mqtt\" \"test/topic\" 1 /path/to/ca.pem",
            args[0]
        );
        eprintln!();
        eprintln!("Using default values: 127.0.0.1 8084 /mqtt t1 1");
        eprintln!();
        (
            "127.0.0.1".to_string(),
            8084u16,
            "/mqtt".to_string(),
            "t1".to_string(),
            1u8,
            None,
        )
    } else {
        let hostname = args[1].clone();
        let port: u16 = args[2].parse().unwrap_or_else(|_| {
            eprintln!("Error: Invalid port number '{}'", args[2]);
            process::exit(1);
        });
        let path = args[3].clone();
        let topic = args[4].clone();
        let qos: u8 = args[5].parse().unwrap_or_else(|_| {
            eprintln!("Error: Invalid QoS level '{}'. Must be 0, 1, or 2", args[5]);
            process::exit(1);
        });
        let cacert_pem_file = args.get(6).cloned();
        (hostname, port, path, topic, qos, cacert_pem_file)
    };

    println!("Simple MQTT Subscriber over WebSocket Secure (WSS)");
    println!("Broker: {hostname}:{port}");
    println!("Path: {path}");
    println!("Topic: {topic}");
    println!("QoS: {qos}");
    if let Some(ref cert_file) = cacert_pem_file {
        println!("CA Certificate: {cert_file}");
    } else {
        println!("CA Certificate: None (insecure connection)");
    }

    // Create MQTT endpoint
    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);

    // Connect to the broker using WebSocket over TLS
    println!("Establishing WSS connection to broker...");

    let tls_config = if cacert_pem_file.is_some() {
        // Use system root certificates for verification
        None
    } else {
        // Use custom TLS config that accepts any certificate (insecure)
        Some(std::sync::Arc::new({
            rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(std::sync::Arc::new(NoVerificationCertVerifier))
                .with_no_client_auth()
        }))
    };

    let wss_stream = match mqtt_ep::transport::connect_helper::connect_tcp_tls_ws(
        &format!("{hostname}:{port}"),
        &hostname,
        &path,
        tls_config,
        None, // No custom headers
        Some(Duration::from_secs(30)),
    )
    .await
    {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("Error: Failed to establish WSS connection to broker: {e:?}");
            process::exit(1);
        }
    };

    let transport = mqtt_ep::transport::WebSocketTransport::from_tls_client_stream(wss_stream);

    // Attach the connected transport to the endpoint
    if let Err(e) = endpoint
        .attach(transport, mqtt_endpoint_tokio::mqtt_ep::Mode::Client)
        .await
    {
        eprintln!("Error: Failed to attach transport to endpoint: {e:?}");
        process::exit(1);
    }
    println!("WSS transport connected and attached successfully!");

    // Send MQTT CONNECT packet
    println!("Sending CONNECT packet...");
    let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("rust_subscriber_wss")
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
            mqtt_ep::packet::Packet::V3_1_1Connack(connack) => {
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
            mqtt_ep::packet::Packet::V3_1_1Suback(suback) => {
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
                    mqtt_ep::packet::Packet::V3_1_1Publish(publish) => {
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

// Custom certificate verifier that accepts any certificate (insecure)
#[derive(Debug)]
struct NoVerificationCertVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerificationCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA1,
            rustls::SignatureScheme::ECDSA_SHA1_Legacy,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}
