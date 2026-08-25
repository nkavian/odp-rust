# Runnable examples

The examples demonstrate both sides of a minimal ODP integration:

| Example | Purpose |
| --- | --- |
| [`odp-service-small`](odp-service-small/) | Publishes a validated in-memory catalog through the framework-neutral Service runtime. |
| [`odp-agent-discovery`](odp-agent-discovery/) | Builds a mock directory from reachable Services, inspects each Service, and navigates its Offerings. |

Run the small Service in one terminal, then the Agent in another:

```sh
cargo run -p odp-examples --bin odp-service-small
cargo run -p odp-examples --bin odp-agent-discovery
```

The Agent also accepts Service origins as positional arguments, so it can inspect any compatible
local ODP Service without changing the example source.
