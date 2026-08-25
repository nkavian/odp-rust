# Small ODP Service

This example adapts `odp-service` to a small HTTP server. It keeps two Offerings and one Collection
in memory, publishes the required Offering operations, automatically advertises Collection
operations, and exposes a working free download Action.

Run it from the repository root:

```sh
cargo run -p odp-examples --bin odp-service-small
```

The Service listens on `127.0.0.1:4104` by default. Set `PORT` to select another local port. Startup
output identifies the Service Document and catalog endpoint; every request logs its method, path,
and response status.
