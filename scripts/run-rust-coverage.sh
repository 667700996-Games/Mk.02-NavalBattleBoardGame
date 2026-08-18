#!/usr/bin/env bash
set -euo pipefail

mkdir -p .artifacts
cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --no-report -- --test-threads=1
cargo llvm-cov report --json --summary-only --output-path .artifacts/rust-coverage.json
