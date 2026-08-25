.PHONY: docs format format-check lint package test verify

docs:
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked

format:
	cargo fmt --all

format-check:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

package:
	cargo package --workspace --allow-dirty --locked

test:
	cargo test --workspace --all-features --locked

verify: format-check lint test docs package
