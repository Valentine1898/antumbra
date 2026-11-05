#!/usr/bin/env bash
# Runs the whole Antumbra flow through the CLI against a deployment written by
# `antumbra deploy`: two wallets, a deposit, a private transfer, a withdrawal,
# a shielded swap, epoch execution, a private claim and a withdrawal of the
# bought asset.
#
# Usage: scripts/demo.sh [deployment.json]
# Env:   ANTUMBRA_NETWORK (testnet | RPC url, default: the deployment's network)
#        ANTUMBRA_RPC     (optional RPC override for testnet)
set -euo pipefail
cd "$(dirname "$0")/.."

DEPLOYMENT="${1:-deployment.json}"
BIN="${ANTUMBRA_BIN:-target/release/antumbra}"
[ -x "$BIN" ] || cargo build -p antumbra-cli --release
NETWORK="${ANTUMBRA_NETWORK:-$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['network'])" "$DEPLOYMENT")}"
TOKEN1=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['tokens'][0])" "$DEPLOYMENT")
TOKEN2=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['tokens'][1])" "$DEPLOYMENT")
DEPLOYER=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['deployer'])" "$DEPLOYMENT")
PAIR=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['pair_id'])" "$DEPLOYMENT")

DEMO_DIR="${DEMO_DIR:-demo-wallets}"
mkdir -p "$DEMO_DIR"
A="$DEMO_DIR/alice.wallet.json"
B="$DEMO_DIR/bob.wallet.json"
ANT="$BIN --network $NETWORK --deployment $DEPLOYMENT"

step() { echo; echo "== $*"; }

step "wallets"
[ -f "$A" ] || $ANT --wallet "$A" keys-new
[ -f "$B" ] || $ANT --wallet "$B" keys-new
BOB_ADDR=$($ANT --wallet "$B" address)
echo "alice: $($ANT --wallet "$A" address)"
echo "bob:   $BOB_ADDR"

step "pool state"
$ANT info

step "alice deposits 600000 of token 1 into a shielded note"
$ANT --wallet "$A" deposit --token "$TOKEN1" --amount 600000
$ANT --wallet "$A" sync

step "alice privately sends 250000 to bob"
$ANT --wallet "$A" transfer --asset 1 --amount 250000 --to "$BOB_ADDR"
$ANT --wallet "$B" sync

step "bob withdraws 100000 to the deployer account (any NEAR account works)"
$ANT --wallet "$B" withdraw --asset 1 --amount 100000 --to "$DEPLOYER"
$ANT --wallet "$B" sync

step "alice swaps 100000 of asset 1 for asset 2"
$ANT --wallet "$A" sync
$ANT --wallet "$A" swap --pair "$PAIR" --sell 1 --amount 100000

step "waiting for the epoch to end, then executing the batch and claiming"
for i in $(seq 1 40); do
  OUT=$($ANT --wallet "$A" claim 2>&1 || true)
  echo "$OUT" | tail -3
  if echo "$OUT" | grep -q "claimed:"; then break; fi
  sleep 5
done
$ANT --wallet "$A" sync

step "alice withdraws the bought asset 2 to the deployer account"
BOUGHT=$($ANT --wallet "$A" balance | awk '/asset 2 =/ {print $NF}')
$ANT --wallet "$A" withdraw --asset 2 --amount "$BOUGHT" --to "$DEPLOYER"
$ANT --wallet "$A" sync

step "final pool state"
$ANT info
echo
echo "demo complete"
