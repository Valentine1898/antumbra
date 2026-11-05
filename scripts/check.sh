#!/usr/bin/env bash
# Everything a judge needs to run, in order: unit tests, contract build with
# key regeneration, and the sandbox end-to-end scenarios.
set -euo pipefail
cd "$(dirname "$0")/.."
echo "== unit tests (core, circuits, pool)"
cargo test -p antumbra-core --all-features -p antumbra-circuits --release -- --nocapture 2>&1 | grep -E "constraints:|test result" | grep -v "0 passed; 0 failed"
CARGO_TARGET_DIR=target/tests cargo test -p antumbra-pool --release 2>&1 | grep -E "test result" | grep -v "0 passed; 0 failed" | head -1
echo "== contracts"
scripts/build-contracts.sh
echo "== sandbox end-to-end"
CARGO_TARGET_DIR=target/tests cargo test -p antumbra-e2e -- --nocapture 2>&1 | grep -E "Tgas|passed|test result|FAILED|panicked" | grep -v "0 passed; 0 failed"
