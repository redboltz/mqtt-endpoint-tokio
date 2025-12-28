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

mod common;
mod stub_transport;
use mqtt_endpoint_tokio::mqtt_ep;

type ClientEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Client>;
type ServerEndpoint = mqtt_ep::Endpoint<mqtt_ep::role::Server>;

#[tokio::test]
async fn test_pingreq_send_timeout() {
    common::init_tracing();
    let mut stub = stub_transport::StubTransport::new();

    // Prepare CONNACK response bytes
    let connack_packet = mqtt_ep::packet::v3_1_1::Connack::builder()
        .session_present(false)
        .return_code(mqtt_ep::result_code::ConnectReturnCode::Accepted)
        .build()
        .unwrap();
    let connack_bytes = connack_packet.to_continuous_buffer();

    // Configure stub responses in order:
    stub.add_response(stub_transport::TransportResponse::SendOk); // For CONNECT
    stub.add_response(stub_transport::TransportResponse::RecvOk(connack_bytes)); // CONNACK response
    stub.add_response(stub_transport::TransportResponse::SendOk); // For PINGREQ

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V3_1_1);
    let attach_result: Result<(), mqtt_ep::ConnectionError> =
        endpoint.attach(stub.clone(), mqtt_ep::Mode::Client).await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    let connect_packet = mqtt_ep::packet::v3_1_1::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .keep_alive(1) // 1 second keep alive
        .clean_session(true)
        .build()
        .unwrap();

    // Send CONNECT packet
    let send_result = endpoint.send(connect_packet).await;
    assert!(
        send_result.is_ok(),
        "CONNECT packet should be sent successfully"
    );

    // Receive CONNACK to establish the connection and start keep-alive timer
    let connack_result =
        tokio::time::timeout(tokio::time::Duration::from_millis(1000), endpoint.recv()).await;
    assert!(
        connack_result.is_ok(),
        "Should receive CONNACK within timeout"
    );
    let received_packet = connack_result.unwrap();
    assert!(
        received_packet.is_ok(),
        "CONNACK should be received successfully"
    );

    // Wait for 2 seconds - this should trigger PINGREQ send timeout after 1 second
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Check that CONNECT and PINGREQ packets were sent
    let calls = stub.get_calls();
    let send_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Send { .. }))
        .collect();

    assert!(
        send_calls.len() >= 2,
        "Should have sent at least CONNECT and PINGREQ packets"
    );

    // Verify the first call is CONNECT (packet type 0x10)
    if let stub_transport::TransportCall::Send { ref data } = send_calls[0] {
        assert_eq!(
            data[0] & 0xF0,
            0x10,
            "First packet should be CONNECT (0x10)"
        );
    }

    // Verify PINGREQ packet (packet type 0xC0) was sent
    let pingreq_found = send_calls.iter().any(|call| {
        if let stub_transport::TransportCall::Send { ref data } = call {
            !data.is_empty() && (data[0] & 0xF0) == 0xC0
        } else {
            false
        }
    });

    assert!(pingreq_found, "PINGREQ packet (0xC0) should have been sent");
}

#[tokio::test]
async fn test_pingreq_recv_timeout() {
    common::init_tracing();
    let mut stub = stub_transport::StubTransport::new();

    // Prepare CONNECT packet bytes
    let connect_packet = mqtt_ep::packet::v5_0::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .keep_alive(1) // 1 second keep alive
        .clean_start(true)
        .build()
        .unwrap();
    let connect_bytes = connect_packet.to_continuous_buffer();

    // Configure stub responses in order:
    stub.add_response(stub_transport::TransportResponse::RecvOk(connect_bytes)); // CONNECT packet from client
    stub.add_response(stub_transport::TransportResponse::SendOk); // For CONNACK
    stub.add_response(stub_transport::TransportResponse::SendOk); // For automatic DISCONNECT (if any)

    let endpoint = ServerEndpoint::new(mqtt_ep::Version::V5_0);
    let attach_result: Result<(), mqtt_ep::ConnectionError> =
        endpoint.attach(stub.clone(), mqtt_ep::Mode::Server).await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    // Receive CONNECT packet from client
    let connect_result =
        tokio::time::timeout(tokio::time::Duration::from_millis(1000), endpoint.recv()).await;
    assert!(
        connect_result.is_ok(),
        "Should receive CONNECT within timeout"
    );
    let received_packet = connect_result.unwrap();
    assert!(
        received_packet.is_ok(),
        "CONNECT should be received successfully"
    );

    // Send CONNACK response
    let connack_packet = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .build()
        .unwrap();
    let send_result = endpoint.send(connack_packet).await;
    assert!(
        send_result.is_ok(),
        "CONNACK packet should be sent successfully"
    );

    // Wait for 2 seconds - PINGREQ should not arrive, triggering timeout after 1.5 seconds (1 + 0.5 * 1)
    // The server should automatically send DISCONNECT and close the socket
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Verify that the transport has been shutdown (by checking the calls)
    let calls = stub.get_calls();
    let send_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Send { .. }))
        .collect();
    let shutdown_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Shutdown { .. }))
        .collect();

    // Verify CONNACK packet was sent (packet type 0x20)
    assert!(
        !send_calls.is_empty(),
        "Should have sent at least CONNACK packet"
    );
    if let stub_transport::TransportCall::Send { ref data } = send_calls[0] {
        assert_eq!(
            data[0] & 0xF0,
            0x20,
            "First packet should be CONNACK (0x20)"
        );
    }

    // Check if automatic DISCONNECT packet was sent due to PINGREQ timeout
    let disconnect_found = send_calls.iter().any(|call| {
        if let stub_transport::TransportCall::Send { ref data } = call {
            !data.is_empty() && (data[0] & 0xF0) == 0xE0 // DISCONNECT packet type
        } else {
            false
        }
    });

    // Assert that either DISCONNECT was sent or socket was shutdown due to PINGREQ timeout
    assert!(
        disconnect_found || !shutdown_calls.is_empty(),
        "Either automatic DISCONNECT packet should be sent or socket should be shutdown due to PINGREQ timeout"
    );

    // If shutdown was called, verify it was called properly
    if !shutdown_calls.is_empty() {
        // Verify shutdown was called (timeout parameter is always present as Duration)
        assert!(
            matches!(
                shutdown_calls[0],
                stub_transport::TransportCall::Shutdown { .. }
            ),
            "Shutdown should be called properly"
        );
    }

    // The close endpoint manually to clean up resources
    let close_result = endpoint.close().await;
    assert!(close_result.is_ok(), "Close should succeed");

    // After manual close, there should be a shutdown call
    let final_calls = stub.get_calls();
    let final_shutdown_calls: Vec<_> = final_calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Shutdown { .. }))
        .collect();

    assert!(
        !final_shutdown_calls.is_empty(),
        "Should have called shutdown at least once (either automatic or manual)"
    );
}

#[tokio::test]
async fn test_pingresp_recv_timeout() {
    common::init_tracing();
    let mut stub = stub_transport::StubTransport::new();

    // Prepare CONNACK response bytes
    let connack_packet = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .build()
        .unwrap();
    let connack_bytes = connack_packet.to_continuous_buffer();

    // Configure stub responses in order:
    stub.add_response(stub_transport::TransportResponse::SendOk); // For CONNECT
    stub.add_response(stub_transport::TransportResponse::RecvOk(connack_bytes)); // CONNACK response
    stub.add_response(stub_transport::TransportResponse::SendOk); // For manual PINGREQ
    stub.add_response(stub_transport::TransportResponse::SendOk); // For automatic DISCONNECT

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Set pingresp_recv_timeout_ms to 1000ms
    let connection_option = mqtt_ep::ConnectionOption::builder()
        .pingresp_recv_timeout_ms(1000u64)
        .build()
        .unwrap();

    let attach_result: Result<(), mqtt_ep::ConnectionError> = endpoint
        .attach_with_options(stub.clone(), mqtt_ep::Mode::Client, connection_option)
        .await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    let connect_packet = mqtt_ep::packet::v5_0::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .clean_start(true)
        .build()
        .unwrap();

    // Send CONNECT packet
    let send_result = endpoint.send(connect_packet).await;
    assert!(
        send_result.is_ok(),
        "CONNECT packet should be sent successfully"
    );

    // Receive CONNACK to establish the connection
    let connack_result =
        tokio::time::timeout(tokio::time::Duration::from_millis(1000), endpoint.recv()).await;
    assert!(
        connack_result.is_ok(),
        "Should receive CONNACK within timeout"
    );
    let received_packet = connack_result.unwrap();
    assert!(
        received_packet.is_ok(),
        "CONNACK should be received successfully"
    );

    // Manually send PINGREQ packet after CONNACK
    let pingreq_packet = mqtt_ep::packet::v5_0::Pingreq::builder().build().unwrap();
    let send_pingreq_result = endpoint.send(pingreq_packet).await;
    assert!(
        send_pingreq_result.is_ok(),
        "PINGREQ packet should be sent successfully"
    );

    // Wait for 2 seconds - PINGRESP will not arrive, triggering timeout after 1 second
    // The endpoint should automatically send DISCONNECT and close the connection
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Check the stub transport calls
    let calls = stub.get_calls();
    let send_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Send { .. }))
        .collect();
    let shutdown_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Shutdown { .. }))
        .collect();

    // Verify packets sent: CONNECT, PINGREQ, and automatic DISCONNECT
    assert!(
        send_calls.len() >= 3,
        "Should have sent at least CONNECT, PINGREQ, and DISCONNECT packets"
    );

    // Verify the first call is CONNECT (packet type 0x10)
    if let stub_transport::TransportCall::Send { ref data } = send_calls[0] {
        assert_eq!(
            data[0] & 0xF0,
            0x10,
            "First packet should be CONNECT (0x10)"
        );
    }

    // Verify PINGREQ packet (packet type 0xC0) was sent
    let pingreq_found = send_calls.iter().any(|call| {
        if let stub_transport::TransportCall::Send { ref data } = call {
            !data.is_empty() && (data[0] & 0xF0) == 0xC0
        } else {
            false
        }
    });
    assert!(pingreq_found, "PINGREQ packet (0xC0) should have been sent");

    // Verify automatic DISCONNECT packet (packet type 0xE0) was sent due to PINGRESP timeout
    let disconnect_found = send_calls.iter().any(|call| {
        if let stub_transport::TransportCall::Send { ref data } = call {
            !data.is_empty() && (data[0] & 0xF0) == 0xE0
        } else {
            false
        }
    });
    assert!(
        disconnect_found,
        "Automatic DISCONNECT packet (0xE0) should have been sent due to PINGRESP timeout"
    );

    // Verify that the transport was shutdown (connection closed)
    assert!(
        !shutdown_calls.is_empty(),
        "Socket should have been closed (shutdown called)"
    );

    // Verify shutdown was called properly
    assert!(
        matches!(
            shutdown_calls[0],
            stub_transport::TransportCall::Shutdown { .. }
        ),
        "Shutdown should be called with proper parameters"
    );
}

#[tokio::test]
async fn test_pingresp_recv_timeout_cancel() {
    common::init_tracing();
    let mut stub = stub_transport::StubTransport::new();

    // Prepare CONNACK response bytes
    let connack_packet = mqtt_ep::packet::v5_0::Connack::builder()
        .session_present(false)
        .reason_code(mqtt_ep::result_code::ConnectReasonCode::Success)
        .build()
        .unwrap();
    let connack_bytes = connack_packet.to_continuous_buffer();

    // Prepare PINGRESP response bytes
    let pingresp_packet = mqtt_ep::packet::v5_0::Pingresp::builder().build().unwrap();
    let pingresp_bytes = pingresp_packet.to_continuous_buffer();

    // Configure stub responses in order:
    stub.add_response(stub_transport::TransportResponse::SendOk); // For CONNECT
    stub.add_response(stub_transport::TransportResponse::RecvOk(connack_bytes)); // CONNACK response
    stub.add_response(stub_transport::TransportResponse::SendOk); // For manual PINGREQ
    stub.add_response(stub_transport::TransportResponse::RecvOk(pingresp_bytes)); // PINGRESP response

    let endpoint = ClientEndpoint::new(mqtt_ep::Version::V5_0);

    // Set pingresp_recv_timeout_ms to 2000ms (2 seconds)
    let connection_option = mqtt_ep::ConnectionOption::builder()
        .pingresp_recv_timeout_ms(2000u64)
        .build()
        .unwrap();

    let attach_result: Result<(), mqtt_ep::ConnectionError> = endpoint
        .attach_with_options(stub.clone(), mqtt_ep::Mode::Client, connection_option)
        .await;
    assert!(attach_result.is_ok(), "Attach should succeed");

    let connect_packet = mqtt_ep::packet::v5_0::Connect::builder()
        .client_id("test_client")
        .unwrap()
        .clean_start(true)
        .build()
        .unwrap();

    // Send CONNECT packet
    let send_result = endpoint.send(connect_packet).await;
    assert!(
        send_result.is_ok(),
        "CONNECT packet should be sent successfully"
    );

    // Receive CONNACK to establish the connection
    let connack_result =
        tokio::time::timeout(tokio::time::Duration::from_millis(1000), endpoint.recv()).await;
    assert!(
        connack_result.is_ok(),
        "Should receive CONNACK within timeout"
    );
    let received_packet = connack_result.unwrap();
    assert!(
        received_packet.is_ok(),
        "CONNACK should be received successfully"
    );

    // Manually send PINGREQ packet after CONNACK
    let pingreq_packet = mqtt_ep::packet::v5_0::Pingreq::builder().build().unwrap();
    let send_pingreq_result = endpoint.send(pingreq_packet).await;
    assert!(
        send_pingreq_result.is_ok(),
        "PINGREQ packet should be sent successfully"
    );

    // Wait for 1 second (before the 2-second timeout), then receive PINGRESP
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Receive PINGRESP packet - this should cancel the PINGRESP timeout timer
    let pingresp_result =
        tokio::time::timeout(tokio::time::Duration::from_millis(1000), endpoint.recv()).await;
    assert!(
        pingresp_result.is_ok(),
        "Should receive PINGRESP within timeout"
    );
    let received_pingresp = pingresp_result.unwrap();
    assert!(
        received_pingresp.is_ok(),
        "PINGRESP should be received successfully"
    );

    // Wait another 2 seconds (total 3 seconds) - no timeout should occur since PINGRESP was received
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Check the stub transport calls
    let calls = stub.get_calls();
    let send_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Send { .. }))
        .collect();
    let shutdown_calls: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Shutdown { .. }))
        .collect();

    // Verify packets sent: only CONNECT and PINGREQ (no DISCONNECT should be sent)
    assert_eq!(
        send_calls.len(),
        2,
        "Should have sent only CONNECT and PINGREQ packets (no automatic DISCONNECT)"
    );

    // Verify the first call is CONNECT (packet type 0x10)
    if let stub_transport::TransportCall::Send { ref data } = send_calls[0] {
        assert_eq!(
            data[0] & 0xF0,
            0x10,
            "First packet should be CONNECT (0x10)"
        );
    }

    // Verify PINGREQ packet (packet type 0xC0) was sent
    if let stub_transport::TransportCall::Send { ref data } = send_calls[1] {
        assert_eq!(
            data[0] & 0xF0,
            0xC0,
            "Second packet should be PINGREQ (0xC0)"
        );
    }

    // Verify NO automatic DISCONNECT packet was sent (timeout was cancelled by PINGRESP)
    let disconnect_found = send_calls.iter().any(|call| {
        if let stub_transport::TransportCall::Send { ref data } = call {
            !data.is_empty() && (data[0] & 0xF0) == 0xE0
        } else {
            false
        }
    });
    assert!(
        !disconnect_found,
        "No automatic DISCONNECT packet should be sent since PINGRESP was received in time"
    );

    // Verify that the transport was NOT shutdown automatically (timer was cancelled)
    assert!(
        shutdown_calls.is_empty(),
        "Socket should NOT have been closed automatically since PINGRESP arrived in time"
    );

    // Manually close the endpoint to clean up resources
    let close_result = endpoint.close().await;
    assert!(close_result.is_ok(), "Manual close should succeed");

    // After manual close, there should be a shutdown call
    let final_calls = stub.get_calls();
    let final_shutdown_calls: Vec<_> = final_calls
        .iter()
        .filter(|call| matches!(call, stub_transport::TransportCall::Shutdown { .. }))
        .collect();

    assert!(
        !final_shutdown_calls.is_empty(),
        "Should have called shutdown at least once (due to manual close)"
    );
}
