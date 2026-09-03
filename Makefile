# genATP — local developer targets (self-hosted). No cloud scaffolding.
.PHONY: help test test-quick smoke fmt clippy check preflight bench version

help:
	@echo "Targets:"
	@echo "  make test         - cargo test (workspace, exclude python)"
	@echo "  make test-quick   - lib tests only"
	@echo "  make smoke        - CLI version + mock Python smoke if available"
	@echo "  make fmt          - rustfmt"
	@echo "  make clippy       - clippy -D warnings"
	@echo "  make check        - fmt check + clippy + lib tests"
	@echo "  make preflight    - scripts/preflight (full CI gates)"
	@echo "  make bench        - criterion microbenchmarks (cassette + vault)"
	@echo "  make version      - print workspace / SDK versions"

test:
	cargo test --workspace --exclude genatp-python

test-quick:
	cargo test --workspace --exclude genatp-python --lib

fmt:
	cargo fmt --all

clippy:
	RUSTFLAGS="-D warnings" cargo clippy --workspace --exclude genatp-python --all-targets -- -D warnings

check: fmt
	cargo fmt --all -- --check
	$(MAKE) clippy
	$(MAKE) test-quick

smoke:
	cargo run -q -p genatp-cli -- version
	@if command -v python >/dev/null 2>&1; then \
	  GENATP_LLM_PROVIDER=mock python bindings/python/examples/mock_smoke.py || true; \
	fi

preflight:
	@if [ -f scripts/preflight.sh ]; then bash scripts/preflight.sh; \
	elif [ -f scripts/preflight.ps1 ]; then pwsh -File scripts/preflight.ps1; \
	else echo "no preflight script"; exit 1; fi

bench:
	cargo bench -p genatp-cassette --bench cassette_replay -- --sample-size 20
	cargo bench -p genatp-vault --bench vault_unlock -- --sample-size 20

version:
	@echo "workspace: $$(grep -m1 '^version' Cargo.toml)"
	@echo "python:    $$(grep -m1 '^version' bindings/python/pyproject.toml)"
	@echo "node:      $$(grep -m1 '"version"' bindings/node/package.json)"
