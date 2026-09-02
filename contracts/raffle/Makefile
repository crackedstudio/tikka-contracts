# contracts/raffle — sub-Makefile for the factory contract
# Most targets delegate to the workspace root Makefile.
# Run `make -C ../.. ci` from the repo root for the full CI pipeline.

.PHONY: build test fmt clean

build:
	cargo build --target wasm32v1-none --release -p raffle-factory
	@ls -lh ../../target/wasm32v1-none/release/raffle_factory.wasm 2>/dev/null || true

test:
	cargo test -p raffle-factory

fmt:
	cargo fmt -p raffle-factory

clean:
	cargo clean
