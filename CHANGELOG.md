# 0.6.1

* Add Unix Domain Socket support. #31
  * `unix-socket` feature flag is introduced (by default enabled).
* Make underlying transportation optional. #30
  * `tls`, `ws`, and `quic` feature flags are introduced (by default enabled).
* Add erase_stored_publish() method to Connection. #29

# 0.6.0

## Breaking changes

* Remove inappropriate fields from ConnectionOption. #26
   * The fields restore_packets and restore_qos2_publish_handled have been removed.
   * Whether to restore or not can now be determined after the server receives the CONNECT packet.

* Remove inappropriate fields from ConnectionOption. #26
   * restore_packets and restore_qos2_publish_handled are removed.
   * restore or not restore can be judged after CONNECT packet is received on server.

## Other updates

* Add async fn restore_stored_packets() and async fn restore_qos2_publish_handled_pids(). #26
   * These functions can be used as replacements for the removed ConnectionOption fields.
* Addasync fn set_auto_pub_response(), set_auto_ping_response(), set_auto_map_topic_alias(), and set_auto_replace_topic_alias().
   * These settings can now be configured either through ConnectionOption or by calling the respective set_*() async functions.

# 0.5.1

* Add re-export mqtt-protocol-core types. #25
   * MqttBinary, MqttString, PayloadFormat, RetainHandling, VariableByteInteger are in mqtt_ep::packet mod.
   * ArcPayload, HashMap, HashSet, IntoPayload are in mqtt_ep::common mod.

# 0.5.0

## Breaking changes

* Update TLS connection function. #23
   * Due to results and tokio-results updating.

## Other updates

* Add QUIC transport support. #23
* Refine connection_helper. #23
* Refine examples. #23

# 0.4.2

* Introduce context. #22
   * async loop local variables are now the fields of context.
   * The function calls are simplified.

# 0.4.1

* Add rust-version 1.86.0 to Cargo.toml. #21

# 0.4.0

## Breaking changes

* Update connection_option::pingreq_send_interval_ms default value from 0 to None. #19
* Remove is_publish_processing(). #18
* Remove GenericEndpointBuilder. #11

## Other updates

* Add badges to the top page. #15
* Refine tests. #12, #14, #19
* Refine CI. #13
* Fix offline publish. #12
* Optimize packet receive process. #10

# 0.3.2

* Update mqtt-protocol-core to 0.6. #9

# 0.3.1

* Add queuing publish. #8
  * When connection option `queuing_receive_maximum` is set to `true`,
    PUBLISH (QoS1, QoS2) on ReceiveMaximum reached are queuing, and when
    the PacketId is released (Receiving PUBACK, PUBREC with error, or PUBCOMP),
    the queuing PUBLISH packet would be sent.
* Fix default value handling of the `GenericConnectionOption`. #8

# 0.3.0

## Breaking changes

* update mqtt-protocol-core to 0.5.0. The following feature flags are re-exported. #7
  * `sso-min-32bit = ["mqtt-protocol-core/sso-min-32bit"]`
  * `sso-min-64bit = ["mqtt-protocol-core/sso-min-64bit"]`
  * `sso-lv10 = ["mqtt-protocol-core/sso-lv10"]`
  * `sso-lv20 = ["mqtt-protocol-core/sso-lv20"]`

* mqtt_ep::common::HashSet::default() should be called instead of mqtt::common::HashSet::new(). #5
* connection_option recv_buffer_size is now Option. If omitted, internal value 4096 is used. #4

# 0.2.0

## Breaking changes

* Update mqtt-protocol-core to 0.3.0. It supports no-std, but this crate is not. #3
  * HashSet and HashMap are now in `mqtt_ep::` mod instead of `std::*`

## Other updates

* Refine CI. #2

# 0.1.0

* Initial import.
