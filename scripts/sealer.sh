#!/usr/bin/env bash
# Runs the sealer for the pool in deployment.json: it opens every finished
# epoch's sealed amounts and submits the signed totals, so swaps can be
# claimed. Needs deployment.secret.json next to deployment.json (written by
# `antumbra deploy`, ignored by git). Without a running sealer swaps stay
# sealed, so keep this alive while the pool is in use.
#   scripts/sealer.sh [pair]            ANTUMBRA_RPC overrides the RPC endpoint
set -euo pipefail
cd "$(dirname "$0")/.."
BIN="${ANTUMBRA_BIN:-target/release/antumbra}"
[ -x "$BIN" ] || cargo build -p antumbra-cli --release
[ -f deployment.secret.json ] || { echo "deployment.secret.json is missing: only the pool's operator can run its sealer" >&2; exit 1; }
exec "$BIN" --network testnet --rpc "${ANTUMBRA_RPC:-https://test.rpc.fastnear.com}" sealer watch --pair "${1:-1}" --interval-secs 20 --lookback 4
