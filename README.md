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
