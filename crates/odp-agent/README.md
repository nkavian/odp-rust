# odp-agent

Agent-side discovery and catalog navigation for the Offering Discovery Protocol.

This crate composes Directory discovery with validated Service inspection, Collection and Offering
navigation, resource-class caching, bounded concurrency, and non-invoking Action resolution. Its
asynchronous client provides a Rustls-backed HTTP transport and permits callers to inject a
compatible transport.

See the [workspace guide](../../README.md) and the
[ODP specification](https://www.offeringprotocol.org/).
