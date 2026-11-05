#!/usr/bin/env bash
# Rebuilds the browser bindings (web/src/wasm) after a change to the Rust crates.
# Needs wasm-pack (npm install -g wasm-pack). The generated package is committed so
# `cd web && npm install && npm run dev` works without it.
set -euo pipefail
cd "$(dirname "$0")/.."
(cd crates/antumbra-wasm && wasm-pack build --target web --release --out-dir ../../web/src/wasm)
rm -f web/src/wasm/.gitignore
ls -la web/src/wasm/antumbra_wasm_bg.wasm
