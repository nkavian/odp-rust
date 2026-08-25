# odp-core

Transport-independent protocol primitives for the Offering Discovery Protocol.

This crate owns ODP wire models, validation, Resource Identity and reference handling, pagination
models, and protocol-level errors. It does not depend on an asynchronous runtime, HTTP client,
Directory behavior, Agent orchestration, or a Service framework.

Normative JSON Schemas are embedded in the crate and compiled without network access. Parsing a
wire document performs JSON Schema validation, typed deserialization, and the semantic checks that
cannot be expressed by JSON Schema alone.

```rust
use odp_core::parse_service_document;

let data = br#"{
  "description": "An AI-enabled plant store.",
  "http": { "endpoint_base": "/odp" },
  "language": "en",
  "localizations": ["en"],
  "name": "Indica Flowers",
  "odp_version": "1.0",
  "operations": [
    { "authentication": "not-required", "name": "get-offering" },
    { "authentication": "not-required", "name": "list-offerings" }
  ]
}"#;

let document = parse_service_document(data)?;
assert_eq!(document.name, "Indica Flowers");
# Ok::<(), odp_core::ParseError>(())
```

See the [workspace guide](../../README.md) and the
[ODP specification](https://www.offeringprotocol.org/).
