# odp-agent

Agent-side discovery and catalog navigation for the Offering Discovery Protocol.

This crate composes Directory discovery with validated Service inspection, Collection and Offering
navigation, resource-class caching, bounded concurrency, and non-invoking Action resolution. Its
asynchronous client provides a Rustls-backed HTTP transport and permits callers to inject a
compatible transport.

## Inspect and navigate one Service

```rust,no_run
use odp_agent::{ServiceClient, TraversalOptions};
use odp_core::Representation;

# #[tokio::main(flavor = "current_thread")]
# async fn main() -> Result<(), Box<dyn std::error::Error>> {
let client = ServiceClient::new("https://demo.inflowpay.ai")?;
let inspection = client.inspect().await?;
println!("{}", inspection.document.name);

let offerings = client
    .list_all_offerings(
        Representation::Terse,
        50,
        TraversalOptions {
            max_items: 100,
            max_pages: 4,
        },
    )
    .await?;
# Ok(())
# }
```

The client checks that a Service advertises an operation before calling it. Every returned resource
is validated. `ServiceClient` uses an in-memory cache by default; callers can inject a shared `Cache`,
set an authentication-aware cache partition, or override the Service Document, Collection, and
Offering fallback lifetimes.

`get_offering_details` bundles an Offering with its validated Attribute Schema, validates the
Offering attributes, and normalizes usable Action targets. `resolve_action` resolves an Action's
request schema or unique OpenAPI 3.1 operation without invoking the target. Supporting documents
are fetched anonymously over HTTPS with independent byte limits and cache entries.

## Search across Services

`Agent::search_offerings_across_services` searches the canonical Directory, then queries the
selected Services with bounded concurrency. Its result order remains deterministic. Each
`DiscoveryEvent` contains either an Offering or a Service-specific issue, allowing one unavailable
Service to be dropped or reported without failing every successful Service.

See the [workspace guide](../../README.md) and the
[ODP specification](https://www.offeringprotocol.org/).
