# Every target in this file that CI also runs is kept in 1:1 parity with the
# job steps in .github/workflows/ci.yml.  `make ci` reproduces CI locally: if
# it passes on your machine, it should pass in the remote.  If the two ever
# drift, fix the Makefile AND CI.yml together — never one without the other.
.PHONY: build test lint fuzz clean deploy-testnet deploy-mainnet verify reproducible \
        check size-check docs-check shellcheck \
        oracle-build oracle-test oracle-lint oracle-ci \
        ci all

# ---------------------------------------------------------------------------
# Core Rust contract targets
# ---------------------------------------------------------------------------

# `stellar contract build` targets wasm32v1-none — the same target the deploy
# scripts and CI use. Keep every build path going through it (issue #841); the
# artifact paths are defined once in scripts/common.sh.
build:
	stellar contract build

# Fast compile gate (matches CI's `check` job and the build_and_test cargo-check step).
check:
	cargo check --workspace --all-targets

# 128KB cap + per-artifact delta vs the baseline in scripts/check_wasm_sizes.py.
size-check: build
	python3 scripts/check_wasm_sizes.py

# Regenerate ERRORS.md / EVENTS.md then fail the target if either file changed.
# This mirrors CI's "Check Error Docs Sync" and "Check Events Docs Sync" steps.
docs-check:
	python3 scripts/generate_error_docs.py
	@git diff --exit-code docs/ERRORS.md >/dev/null || { \
		echo "ERROR: docs/ERRORS.md out of sync. Run 'python3 scripts/generate_error_docs.py'."; \
		exit 1; \
	}
	python3 scripts/generate_event_docs.py
	@git diff --exit-code docs/EVENTS.md >/dev/null || { \
		echo "ERROR: docs/EVENTS.md out of sync. Run 'python3 scripts/generate_event_docs.py'."; \
		exit 1; \
	}

test:
	cargo test --workspace

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

FUZZ_TARGETS := fuzz_buy_ticket fuzz_finalize_raffle fuzz_winner_selection fuzz_refund_cancel fuzz_commit_reveal
FUZZ_TIME ?= 300

fuzz:
	@for target in $(FUZZ_TARGETS); do \
		echo "==> fuzzing $$target ($${FUZZ_TIME}s)"; \
		cargo fuzz run $$target -- -max_total_time=$(FUZZ_TIME); \
	done

deploy-testnet:
	./scripts/deploy-testnet.sh

deploy-mainnet:
	./scripts/deploy-mainnet.sh

verify:
	./scripts/verify.sh

reproducible:
	./scripts/build-reproducible.sh

clean:
	cargo clean

# ---------------------------------------------------------------------------
# Scripts
# ---------------------------------------------------------------------------

shellcheck:
	@command -v shellcheck >/dev/null 2>&1 || { \
		echo "shellcheck not found. Install via: apt-get install shellcheck / brew install shellcheck"; \
		exit 1; \
	}
	shellcheck scripts/*.sh

# ---------------------------------------------------------------------------
# Oracle TypeScript service
# ---------------------------------------------------------------------------

oracle-build:
	cd oracle && npm ci && npm run build

oracle-test:
	cd oracle && npm run test:ci

# Lint + typecheck + format check combined (used by pre-commit fast path).
# `oracle-ci` below is the full CI job run including npm ci + tests.
oracle-lint:
	cd oracle && npm run format:check && npm run lint && npm run typecheck

oracle-ci:
	cd oracle && npm ci && npm run format:check && npm run lint && npm run typecheck && npm run test:ci

# ---------------------------------------------------------------------------
# Aggregate targets
# ---------------------------------------------------------------------------

# Lightweight local pre-build. NOT the CI gate — use `make ci` for that.
all: lint test build

# Exactly the same pipeline CI runs. Passing this locally implies passing in
# CI. CONTRIBUTING.md documents this as the required pre-push command.
# Order matters: cheap checks first, expensive builds and tests last.
ci:
	@echo "==> 1/11 cargo fmt --check"
	cargo fmt --all -- --check
	@echo "==> 2/11 cargo check --workspace --all-targets"
	cargo check --workspace --all-targets
	@echo "==> 3/11 orphan-module check"
	python3 scripts/check_orphan_modules.py
	@echo "==> 4/11 stellar contract build (wasm32v1-none)"
	stellar contract build
	@echo "==> 5/11 WASM size baseline delta"
	python3 scripts/check_wasm_sizes.py
	@echo "==> 6/11 docs sync (ERRORS.md + EVENTS.md)"
	$(MAKE) docs-check
	@echo "==> 7/11 cargo clippy -D warnings"
	cargo clippy --all-targets --all-features -- -D warnings
	@echo "==> 8/11 cargo test --workspace"
	cargo test --workspace
	@echo "==> 9/11 cargo test --workspace --doc"
	cargo test --workspace --doc
	@echo "==> 10/11 shellcheck scripts/*.sh"
	$(MAKE) shellcheck
	@echo "==> 11/11 oracle-ci (format:check + lint + typecheck + test:ci)"
	$(MAKE) oracle-ci
