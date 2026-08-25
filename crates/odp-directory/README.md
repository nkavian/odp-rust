# odp-directory

Canonical directory client for the Offering Discovery Protocol.

This crate owns fixed production and sandbox Directory access, validated Service search results,
facets, suggestions, and opaque continuation traversal. Its asynchronous client provides a
Rustls-backed HTTP transport and permits callers to inject a compatible transport.

See the [workspace guide](../../README.md) and the
[ODP specification](https://www.offeringprotocol.org/).
