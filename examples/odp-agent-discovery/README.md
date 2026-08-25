# ODP Agent discovery

This example makes the two discovery stages explicit. It first builds a **mock directory** from the
configured origins that are currently reachable; it does not call the canonical ODP Directory. It
then uses `odp-agent` to inspect every discovered Service, list terse Offerings, and fetch each full
Offering.

Start one or more compatible Services, then run:

```sh
cargo run -p odp-examples --bin odp-agent-discovery
```

The default candidates are the Node, Go, Java, and Rust example ports (`4101` through `4104`). Pass
origins to replace that list:

```sh
cargo run -p odp-examples --bin odp-agent-discovery -- \
  http://127.0.0.1:4104 \
  http://127.0.0.1:4101
```

Unreachable candidates are omitted. If no compatible Service is reachable, the example exits with
a direct error rather than presenting an empty directory.
