#!/usr/bin/env bash
set -euo pipefail

fuzz_toolchain="${FUZZ_TOOLCHAIN:-nightly}"
fuzz_seconds="${FUZZ_SECONDS:-20}"
cargo "+$fuzz_toolchain" fuzz run protocol_json -- \
  "-max_total_time=$fuzz_seconds" \
  -timeout=2 \
  -rss_limit_mb=2048
