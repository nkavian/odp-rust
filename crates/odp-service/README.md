# odp-service

Service-side integration for the Offering Discovery Protocol.

This crate owns validated Service Documents, catalog operations, static catalogs for small
Services, and storage-backed operation boundaries for large catalogs. Its request and response
surface remains independent of Axum, Actix Web, and other application frameworks.

Implement `Catalog` for a database-backed or remote catalog. Small Services can use `StaticCatalog`,
which validates all resources during construction and supplies bounded, expiring, stateless
continuations.

```rust,no_run
use std::sync::Arc;

use odp_core::{parse_offering, parse_service_document};
use odp_service::{Service, StaticCatalog, StaticCatalogOptions};

let document = parse_service_document(br#"{
  "description": "An AI-enabled plant store.",
  "http": { "endpoint_base": "/odp" },
  "language": "en",
  "localizations": ["en"],
  "name": "Indica Flowers",
  "odp_version": "1.0",
  "operations": []
}"#)?;
let offering = parse_offering(
    br#"{"id":"rubber-plant","name":"Rubber Plant","odp_version":"1.0"}"#,
)?;
let catalog = StaticCatalog::new(StaticCatalogOptions {
    collections: Vec::new(),
    offerings: vec![offering],
})?;
let service = Service::new(document, Arc::new(catalog))?;
assert_eq!(service.document().name, "Indica Flowers");
# Ok::<(), Box<dyn std::error::Error>>(())
```

Adapt the framework-neutral `Request` and `Response` values at the application boundary. The
Service owns fixed methods, paths, media types, request bounds, response validation, and structured
problem responses; the host application retains authentication, payment, routing, and persistence.

See the [workspace guide](../../README.md) and the
[ODP specification](https://www.offeringprotocol.org/).
