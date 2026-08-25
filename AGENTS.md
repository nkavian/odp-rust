# AGENTS.md

## Repository

This workspace contains the official Rust crates for ODP:

- `odp-core`: transport-independent protocol primitives.
- `odp-directory`: canonical directory client.
- `odp-agent`: Agent-side composition.
- `odp-service`: Service-side integration.

The normative protocol is maintained in `offering-protocol/odp-specs`. Check that source before
implementing or changing wire behavior.

## Verification

Run `make verify` before merging. Public APIs must be backed by tests and authoritative protocol
behavior.

## Conventions

- Support Rust 1.85 and newer with Rust 2024 Edition; continuous integration covers the minimum
  supported compiler and current stable Rust.
- Keep `odp-core` independent of asynchronous runtimes and HTTP clients.
- Agent and Directory APIs are asynchronous, provide a Rustls-backed default HTTP transport, and
  permit transport injection.
- Keep Service integration independent of Axum, Actix Web, and other application frameworks.
- Forbid unsafe code in every workspace crate.
- Return typed errors rather than logging from library crates.
- Keep dependency direction aligned with the crate responsibilities above.
- Describe current behavior; do not leave speculative or historical comments.
- Keep public APIs small, idiomatic, and backed by tests.
