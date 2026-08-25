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
	cargo package -p odp-core --allow-dirty --locked
	cargo package -p odp-directory --allow-dirty --locked --list
	cargo package -p odp-agent --allow-dirty --locked --list
	cargo package -p odp-service --allow-dirty --locked --list

test:
	cargo test --workspace --all-features --locked

verify: format-check lint test docs package
