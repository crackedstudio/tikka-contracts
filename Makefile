.PHONY: all build test fmt check clippy size-check docs-check shellcheck oracle-lint ci clean

SOROBAN_TARGET ?= wasm32v1-none

all: test

build:
	@echo "==> Building contracts for $(SOROBAN_TARGET)"
	cargo build --target $(SOROBAN_TARGET) --release
	@echo "==> Build complete"
	@ls -lh target/$(SOROBAN_TARGET)/release/*.wasm 2>/dev/null || true

test:
	@echo "==> Running full workspace tests"
	cargo test --workspace

fmt:
	cargo fmt --all

check:
	@echo "==> Checking formatting"
	cargo fmt --all -- --check

clippy:
	@echo "==> Running Clippy"
	cargo clippy --all-targets --all-features -- -D warnings

size-check: build
	@echo "==> Checking WASM binary sizes"
	@for wasm in target/$(SOROBAN_TARGET)/release/*.wasm; do \
		if [ -f "$$wasm" ]; then \
			size_kb=$$(wc -c < "$$wasm" | awk '{printf "%.1f", $$1/1024}'); \
			echo "  $$(basename $$wasm): $$size_kb KB"; \
			size_bytes=$$(wc -c < "$$wasm"); \
			if [ "$$size_bytes" -gt 524288 ]; then \
				echo "  WARNING: $$(basename $$wasm) exceeds 512 KB recommended limit"; \
			fi; \
		fi; \
	done

docs-check:
	@echo "==> Checking documentation"
	@missing=0; \
	for doc in docs/ERRORS.md docs/EVENTS.md docs/ARCHITECTURE.md docs/STORAGE.md docs/DEPLOYMENT.md docs/README.md; do \
		if [ ! -f "$$doc" ]; then \
			echo "  MISSING: $$doc"; \
			missing=1; \
		else \
			echo "  OK: $$doc"; \
		fi; \
	done; \
	if [ "$$missing" -ne 0 ]; then \
		echo "ERROR: One or more required docs missing"; \
		exit 1; \
	fi

shellcheck:
	@echo "==> Running shellcheck on deploy scripts"
	@if command -v shellcheck >/dev/null 2>&1; then \
		shellcheck scripts/*.sh; \
		echo "  shellcheck passed"; \
	else \
		echo "  WARNING: shellcheck not installed, skipping"; \
	fi

oracle-lint:
	@echo "==> Linting oracle service"
	@if [ -f oracle/package.json ]; then \
		cd oracle && npm run build 2>&1; \
		echo "  oracle build passed"; \
		if grep -q '"lint"' oracle/package.json 2>/dev/null; then \
			cd oracle && npm run lint 2>&1; \
			echo "  oracle lint passed"; \
		else \
			echo "  oracle: lint script not defined in package.json, skipping lint"; \
		fi; \
		if grep -q '"format:check"' oracle/package.json 2>/dev/null; then \
			cd oracle && npm run format:check 2>&1; \
			echo "  oracle format:check passed"; \
		else \
			echo "  oracle: format:check script not defined in package.json, skipping format check"; \
		fi; \
	else \
		echo "  oracle/package.json not found, skipping oracle checks"; \
	fi

ci: check clippy build test size-check docs-check shellcheck oracle-lint
	@echo "==> All CI checks passed"

clean:
	cargo clean
