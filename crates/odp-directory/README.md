# odp-directory

Canonical directory client for the Offering Discovery Protocol.

This crate owns fixed production and sandbox Directory access, validated Service search results,
facets, suggestions, and opaque continuation traversal. Its asynchronous client provides a
Rustls-backed HTTP transport and permits callers to inject a compatible transport.

Use `Environment::Production` for `https://api.inflowpay.ai` or `Environment::Sandbox` for
`https://sandbox.inflowpay.ai`. These are the only Directory origins accepted by the client.

```rust,no_run
use odp_directory::{DirectoryClient, Environment, IterationOptions, SearchRequest};

# #[tokio::main(flavor = "current_thread")]
# async fn main() -> Result<(), Box<dyn std::error::Error>> {
let directory = DirectoryClient::new(Environment::Production)?;
let services = directory
    .search_services(
        &SearchRequest {
            query: "plants".to_owned(),
            ..SearchRequest::default()
        },
        IterationOptions {
            max_items: 10,
            max_pages: 2,
        },
    )
    .await?;

for service in services {
    println!("{}: {}", service.name, service.service_origin);
}
# Ok(())
# }
```

`search` returns one page. `continue_search` follows one opaque `next` reference.
`search_pages` and `search_services` perform bounded traversal for callers that want aggregation.

See the [workspace guide](../../README.md) and the
[ODP specification](https://www.offeringprotocol.org/).
