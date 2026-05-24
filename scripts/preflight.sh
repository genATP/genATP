#!/usr/bin/env bash
# genATP preflight — run before every commit/push to match CI gates locally.
# Usage:  ./scripts/preflight.sh
#         ./scripts/preflight.sh --quick   # skip full test suite nuances

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export PATH="${HOME}/.cargo/bin:${PATH}"

QUICK=0
if [[ "${1:-}" == "--quick" ]]; then
  QUICK=1
fi

step() {
  echo ""
  echo "==> $1"
  shift
  "$@"
  echo "OK: $1"
}

step "cargo fmt (apply)" cargo fmt --all
step "cargo fmt --check" cargo fmt --all -- --check
step "cargo clippy" env RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings
if [[ "$QUICK" -eq 1 ]]; then
  step "cargo test (lib only, --quick)" cargo test --workspace --lib
else
  step "cargo test --workspace" cargo test --workspace
fi
step "cargo deny check" cargo deny check

echo ""
echo "Preflight passed. Safe to commit and push."
