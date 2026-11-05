#!/usr/bin/env bash
# Builds the NEAR contracts (release, wasm32) and regenerates the embedded verifying key.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run -p antumbra-circuits --bin antumbra-setup --release
cargo build -p antumbra-pool -p test-ft --target wasm32-unknown-unknown --release
ls -la target/wasm32-unknown-unknown/release/antumbra_pool.wasm target/wasm32-unknown-unknown/release/test_ft.wasm
