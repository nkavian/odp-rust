# Offering Discovery Protocol for Rust

[![CI](https://github.com/offering-protocol/odp-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/offering-protocol/odp-rust/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

Official Rust software development kit for the
[Offering Discovery Protocol](https://www.offeringprotocol.org/), the open protocol for discovering
Services and navigating their Offerings.

ODP separates Service discovery from catalog discovery. An Agent searches the canonical directory
for candidate Services, inspects each Service's live ODP document, and then navigates or searches
that Service's Collections and Offerings.

## Workspace

| Goal                                               | Crate           | Guide                                      |
| -------------------------------------------------- | --------------- | ------------------------------------------ |
| Work with protocol models and validation           | `odp-core`      | [Protocol core](./crates/odp-core)         |
| Search the canonical directory                     | `odp-directory` | [Directory client](./crates/odp-directory) |
| Discover Services and navigate their catalogs      | `odp-agent`     | [Agent integration](./crates/odp-agent)    |
| Publish an ODP Service                             | `odp-service`   | [Service integration](./crates/odp-service) |

The dependency direction is intentionally narrow: Core remains transport-independent, Directory
depends toward Core, Agent composes Core and Directory, and Service depends toward Core without
depending on Agent behavior.

Agent and Directory networking is asynchronous. The role crates provide an ergonomic Rustls-backed
HTTP transport while preserving an injectable transport boundary. Service integration remains
independent of a particular Rust web framework.

## Installation

Add only the crates required by the integration role:

```toml
[dependencies]
odp-agent = "0.1.0"
odp-core = "0.1.0"
odp-directory = "0.1.0"
odp-service = "0.1.0"
```

An Agent normally uses `odp-agent`, which brings in Core and Directory. A Service normally uses
`odp-service`, which brings in Core. The individual crate guides contain executable API examples.

## Protocol behavior

- The Directory client has fixed production and sandbox origins. Callers cannot supply an arbitrary
  Directory endpoint.
- Agent responses are schema-validated before being returned. Service Documents, Collections, and
  Offerings use separate default cache lifetimes and support HTTP revalidation.
- Directory and Agent traversal follow opaque `next` references with explicit bounds of 16
  pages and 10,000 resources.
- Federated Agent discovery searches Services concurrently while yielding results in Directory
  order. A failure from one Service becomes an issue event instead of discarding other results.
- The Service crate validates configuration and responses but does not impose a web framework or
  storage model. `StaticCatalog` provides the small-catalog integration path.

## Development

Rust 1.85 or newer is required. The repository toolchain follows current stable Rust while continuous
integration also verifies the minimum supported compiler.

Run the complete merge gate with:

```sh
make verify
```

Format source files with:

```sh
make format
```

The merge gate checks formatting, Clippy with warnings denied, tests, Rust documentation, and the
contents and metadata of every publishable crate.

See [`odp-specs`](https://github.com/offering-protocol/odp-specs) for the normative draft, schemas,
examples, and test vectors.

## Security

See [SECURITY.md](./SECURITY.md) for vulnerability reporting.

## License

MIT.
