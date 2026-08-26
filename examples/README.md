# Runnable examples

The examples demonstrate both sides of a minimal ODP integration:

| Example | Purpose |
| --- | --- |
| [`odp-service-small`](odp-service-small/) | Publishes a validated in-memory catalog through the framework-neutral Service runtime. |
| [`odp-agent-discovery`](odp-agent-discovery/) | Injects a mock Directory into the top-level Agent, performs federated discovery, and navigates Collections, Offerings, and Actions. |

Run the small Service in one terminal, then the Agent in another:

```sh
cargo run -p odp-examples --bin odp-service-small
cargo run -p odp-examples --bin odp-agent-discovery
```

The Agent also accepts Service origins as positional arguments. It converts the compatible origins
into an in-memory Directory response, while all Service requests continue to use the real HTTP
endpoints.
