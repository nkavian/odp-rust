.PHONY: conformance consumer-smoke docs examples format format-check interoperability lint package test verify

conformance:
	./scripts/run-conformance.sh

consumer-smoke:
	./scripts/verify-consumer.sh

docs:
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked

format:
	cargo fmt --all

format-check:
	cargo fmt --all --check

interoperability:
	./scripts/run-node-interoperability.sh

lint:
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

package:
	cargo package -p odp-core --allow-dirty --locked
	cargo package -p odp-directory --allow-dirty --locked --list
	cargo package -p odp-agent --allow-dirty --locked --list
	cargo package -p odp-service --allow-dirty --locked --list

examples:
	cargo build -p odp-examples --locked

test:
	cargo test --workspace --all-features --locked

verify: format-check lint test docs package
