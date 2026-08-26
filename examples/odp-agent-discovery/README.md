# ODP Agent discovery

This example makes the two discovery stages explicit. It first inspects the configured origins and
uses the compatible Services to construct an in-memory **mock directory**; it does not call the
canonical ODP Directory. The mock is injected into the top-level `Agent`, which performs federated
Offering discovery using the same orchestration API as a production Agent.

The example then uses each discovered Service's real HTTP endpoints to print its Service Document,
list Collections and terse Offerings, fetch full Offering details, report structured issues, and
resolve every usable Action without invoking it.

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

Unreachable or incompatible candidates are omitted. If no compatible Service is reachable, the
example exits with a direct error rather than presenting an empty directory.
