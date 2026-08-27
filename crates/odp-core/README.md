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

`ServiceProtocols::trust` carries generic trust-protocol advertisements. TAP support is represented
by `TrustProtocol { name: Protocol::Tap }`.

Validation failures retain structured paths and schema keywords so applications can present more
than a generic parse failure:

```rust
use odp_core::{ParseError, parse_service_document};

let error = parse_service_document(br#"{}"#).unwrap_err();
if let ParseError::Validation(validation) = error {
    for issue in validation.issues {
        println!("{}: {}", issue.path, issue.message);
    }
}
```

Resource identifiers are local to a Service. Compose a stable identity with the canonical Service
origin, and resolve origin-relative resource links against that same origin:

```rust
use odp_core::{ResourceIdentity, ResourceType, resolve_resource_reference};

let identity = ResourceIdentity::new(
    "https://shop.example/.well-known/odp",
    ResourceType::Offering,
    "rubber-plant",
)?;
let image = resolve_resource_reference("/images/rubber-plant.webp", &identity.service)?;

assert_eq!(identity.service, "https://shop.example");
assert_eq!(image.as_str(), "https://shop.example/images/rubber-plant.webp");
# Ok::<(), odp_core::ReferenceError>(())
```

Unknown additive members are retained in each model's `additional` map. Use
`parse_problem_response` for an ODP error response when the HTTP status must agree with the Problem
Details body.

See the [workspace guide](../../README.md) and the
[ODP specification](https://www.offeringprotocol.org/).
