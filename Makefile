# =============================================================================
# Tikka Contracts — Root Makefile
#
# This is the single source of truth for all local and CI checks.
# Run `make ci` before pushing to ensure parity with GitHub Actions.
# =============================================================================

.PHONY: default all build test fmt fmt-check check clippy size-check \
        docs-check shellcheck oracle-lint oracle-build ci clean help

# ---------------------------------------------------------------------------
# Tooling paths — override these if your environment differs
# ---------------------------------------------------------------------------
CARGO         := cargo
STELLAR       := stellar
ORACLE_DIR    := oracle
SCRIPTS_DIR   := scripts

# ---------------------------------------------------------------------------
# Default goal
# ---------------------------------------------------------------------------
default: help

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

## build: Compile all workspace contracts to WASM (release profile)
build:
	$(CARGO) build --target wasm32v1-none --release
	@echo "=== WASM sizes ==="
	@ls -lh target/wasm32v1-none/release/*.wasm 2>/dev/null || \
		ls -lh target/wasm32-unknown-unknown/release/*.wasm 2>/dev/null || \
		echo "(no .wasm files found — check your target)"

# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

## test: Run all workspace unit tests
test:
	$(CARGO) test --workspace

## test-instance: Run only raffle-instance tests
test-instance:
	$(CARGO) test -p raffle-instance

## test-factory: Run only raffle-factory tests
test-factory:
	$(CARGO) test -p raffle-factory

# ---------------------------------------------------------------------------
# Formatting & style
# ---------------------------------------------------------------------------

## fmt: Auto-format all Rust source files
fmt:
	$(CARGO) fmt --all

## fmt-check: Fail if Rust source files are not formatted
fmt-check:
	$(CARGO) fmt --all -- --check

# ---------------------------------------------------------------------------
# Static analysis
# ---------------------------------------------------------------------------

## check: Fast type-check (no codegen) — catches compile errors quickly
check:
	$(CARGO) check --workspace --all-targets

## clippy: Run linter; fail on any warning
clippy:
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

# ---------------------------------------------------------------------------
# WASM size check
# ---------------------------------------------------------------------------

## size-check: Build and report WASM binary sizes (fail if > 200 kB per binary)
size-check: build
	@echo "=== WASM size check (limit: 200 kB) ==="
	@failed=0; \
	for f in target/wasm32v1-none/release/*.wasm \
	          target/wasm32-unknown-unknown/release/*.wasm 2>/dev/null; do \
	    [ -f "$$f" ] || continue; \
	    size=$$(wc -c < "$$f"); \
	    kb=$$((size / 1024)); \
	    echo "  $$f: $${kb} kB"; \
	    if [ $$size -gt 204800 ]; then \
	        echo "  FAIL: $${f} exceeds 200 kB limit"; \
	        failed=1; \
	    fi; \
	done; \
	exit $$failed

# ---------------------------------------------------------------------------
# Docs check
# ---------------------------------------------------------------------------

## docs-check: Build rustdoc with -D warnings; fail on missing/broken docs
docs-check:
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --workspace --no-deps --document-private-items

# ---------------------------------------------------------------------------
# Shell scripts
# ---------------------------------------------------------------------------

## shellcheck: Lint all shell scripts in scripts/
shellcheck:
	@command -v shellcheck >/dev/null 2>&1 || { \
	    echo "shellcheck not found — install with: apt-get install shellcheck / brew install shellcheck"; \
	    exit 1; \
	}
	shellcheck $(SCRIPTS_DIR)/*.sh

# ---------------------------------------------------------------------------
# Oracle (TypeScript)
# ---------------------------------------------------------------------------

## oracle-build: Compile oracle TypeScript to JavaScript
oracle-build:
	@command -v npm >/dev/null 2>&1 || { echo "npm not found"; exit 1; }
	cd $(ORACLE_DIR) && npm ci && npm run build

## oracle-lint: Type-check oracle TypeScript without emitting output
oracle-lint:
	@command -v npm >/dev/null 2>&1 || { echo "npm not found"; exit 1; }
	cd $(ORACLE_DIR) && npm ci --silent && npx tsc --noEmit

# ---------------------------------------------------------------------------
# CI gate — mirrors .github/workflows/ci.yml exactly
# Run this before every push: make ci
# ---------------------------------------------------------------------------

## ci: Full CI pipeline (fmt-check, check, clippy, test, size-check, docs-check, oracle-lint)
ci: fmt-check check clippy test size-check docs-check oracle-lint
	@echo ""
	@echo "======================================"
	@echo "  All CI checks passed. Ready to push."
	@echo "======================================"

# ---------------------------------------------------------------------------
# Clean
# ---------------------------------------------------------------------------

## clean: Remove build artefacts
clean:
	$(CARGO) clean

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------

## help: Print available targets and their descriptions
help:
	@echo "Usage: make <target>"
	@echo ""
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/## /  /'
