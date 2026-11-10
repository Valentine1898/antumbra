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

# Amounts below are in base units. With a 24-decimal token 1 (wNEAR) they are
# scaled up so the demo moves real fractions of a token; override with AMOUNT_UNIT.
case "$TOKEN1" in wrap.*) UNIT="${AMOUNT_UNIT:-1000000000000000000}" ;; *) UNIT="${AMOUNT_UNIT:-1}" ;; esac
amt() { python3 -c "print($1 * $UNIT)"; }

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

step "the gas payer gets some of token 1"
case "$TOKEN1" in
  wrap.*) $ANT wrap --token "$TOKEN1" --amount "$(amt 700000)" ;;
  *) $ANT mint --token "$TOKEN1" --to "$DEPLOYER" --amount "$(amt 1000000)" ;;
esac

step "alice deposits $(amt 600000) base units of token 1 into a shielded note"
$ANT --wallet "$A" deposit --token "$TOKEN1" --amount "$(amt 600000)"
$ANT --wallet "$A" sync

step "alice privately sends 250000 to bob"
$ANT --wallet "$A" transfer --asset 1 --amount "$(amt 250000)" --to "$BOB_ADDR"
$ANT --wallet "$B" sync

step "bob withdraws 100000 to the deployer account (any NEAR account works)"
$ANT --wallet "$B" withdraw --asset 1 --amount "$(amt 100000)" --to "$DEPLOYER"
$ANT --wallet "$B" sync

step "alice swaps 100000 of asset 1 for asset 2"
$ANT --wallet "$A" sync
$ANT --wallet "$A" swap --pair "$PAIR" --sell 1 --amount "$(amt 100000)"

step "waiting for the epoch to end and the sealer to clear it, then claiming"
# With the sealer's secrets in the secret file (any deployment made by `deploy`)
# this clears the epoch itself. Otherwise the pool's own sealer does: within
# seconds while its operator runs scripts/sealer.sh, within minutes when only
# the scheduled workflow is clearing the hosted pool. SEALER_WAIT (seconds,
# default 1800) is how long to wait for it.
SEALER_WAIT="${SEALER_WAIT:-1800}"
DEADLINE=$(( $(date +%s) + SEALER_WAIT ))
SEEN=""
while :; do
  $ANT sealer clear --pair "$PAIR" >/dev/null 2>&1 || true
  OUT=$($ANT --wallet "$A" claim 2>&1 || true)
  if echo "$OUT" | grep -q "claimed:"; then echo "$OUT" | tail -3; break; fi
  STATE=$(echo "$OUT" | grep -m1 "swap #" || echo "$OUT" | tail -1)
  case "$STATE" in
    *"not cleared"*)
      if [ "$SEEN" != cleared ]; then
        echo "$STATE"
        echo "waiting for the pool's sealer (up to $((SEALER_WAIT / 60)) min; the hosted pool is cleared on a schedule, every few minutes)"
        SEEN=cleared
      fi ;;
    *) echo "$STATE" ;;
  esac
  if [ "$(date +%s)" -ge "$DEADLINE" ]; then
    echo "the sealer has not cleared the epoch in $SEALER_WAIT s; once it has, run: $ANT --wallet $A claim" >&2
    exit 1
  fi
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
