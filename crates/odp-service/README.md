# odp-service

Service-side integration for the Offering Discovery Protocol.

This crate owns validated Service Documents, catalog operations, static catalogs for small
Services, and storage-backed operation boundaries for large catalogs. Its request and response
surface remains independent of Axum, Actix Web, and other application frameworks.

See the [workspace guide](../../README.md) and the
[ODP specification](https://www.offeringprotocol.org/).
