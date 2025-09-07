# 0.4.0

## Breaking changes

* Remove GenericEndpointBuilder. #11

## Other updates

* Refine tests. #12
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
