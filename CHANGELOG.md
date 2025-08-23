# 0.3.0

## Breaking changes

* connection_option recv_buffer_size is now Option. If omitted, internal value 4096 is used. #4

# 0.2.0

## Breaking changes

* Update mqtt-protocol-core to 0.3.0. It supports no-std, but this crate is not. #3
  * HashSet and HashMap are now in `mqtt_ep::` mod instead of `std::*`

## Other updates

* Refine CI. #2

# 0.1.0

* Initial import.
