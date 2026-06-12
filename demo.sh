#!/usr/bin/env bash
# LP-0003 private airdrop end-to-end demo.
#
# Offline mode (default): generates a claim proof locally, then verifies it.
# Chain mode (--chain):   also initializes the distributor and submits the claim on-chain.
#
# Usage:
#   ./demo.sh [--dev] [--chain] [--sequencer <url>]
#
#   --dev        RISC0_DEV_MODE=1 (fast mock proofs, no ZK work)
#   --chain      Run on-chain steps. Requires:
#                  1. wallet deploy-program output in PROGRAM_BIN (see below)
#                  2. cargo build --release --features chain
#                  3. A running sequencer (local docker or testnet)
#
# Testnet:
#   SEQUENCER=https://testnet.lez.logos.co ./demo.sh --dev --chain

set -euo pipefail

DEV_MODE=0
CHAIN_MODE=0
SEQUENCER="${SEQUENCER:-http://127.0.0.1:3040}"
for arg in "$@"; do
  [ "$arg" = "--dev" ]   && DEV_MODE=1
  [ "$arg" = "--chain" ] && CHAIN_MODE=1
done

if [ "$DEV_MODE" = "1" ]; then
  export RISC0_DEV_MODE=1
  echo "[demo] RISC0_DEV_MODE=1 (fast mock proofs, no ZK)"
else
  echo "[demo] Real RISC0 proofs -- proof generation takes several minutes"
fi

if [ "$CHAIN_MODE" = "1" ]; then
  BIN_FEATURES="--features chain"
else
  BIN_FEATURES=""
fi

CLAIM_BIN="./target/release/airdrop-claim"
PROGRAM_BIN="${PROGRAM_BIN:-}"
PROGRAM_ID="${PROGRAM_ID:-}"

echo ""
echo "=== LP-0003 Private Airdrop Demo ==="
echo "Sequencer: $SEQUENCER"
echo ""

echo "[1/5] Building..."
# shellcheck disable=SC2086
cargo build --release --bin airdrop-claim $BIN_FEATURES 2>&1 | tail -3

# Deterministic test inputs -- never use these outside demo/testing
ACCOUNT_ID="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
DISTRIBUTOR_ID="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
ALLOCATION=1000
RECIPIENT_NOTE="cafebabe01020304"

# Minimal depth-1 Merkle tree: root = SHA256(0x01 || leaf || leaf)
LEAF=$(python3 -c "
import hashlib, struct
acct = bytes.fromhex('$ACCOUNT_ID')
alloc = ($ALLOCATION).to_bytes(16, 'little')
leaf = hashlib.sha256(b'\x00' + acct + alloc).digest()
print(leaf.hex())
")
MERKLE_ROOT=$(python3 -c "
import hashlib
leaf = bytes.fromhex('$LEAF')
root = hashlib.sha256(b'\x01' + leaf + leaf).digest()
print(root.hex())
")

echo ""
echo "[2/5] Tree: leaf=$LEAF"
echo "      root=$MERKLE_ROOT"

echo ""
if [ "$CHAIN_MODE" = "1" ]; then
  echo "[3/5] Deploying program + initializing distributor on-chain..."
  if [ -z "$PROGRAM_ID" ] && [ -n "$PROGRAM_BIN" ]; then
    echo "  Deploying program binary: $PROGRAM_BIN"
    PROGRAM_ID=$(wallet deploy-program "$PROGRAM_BIN" | grep -oE '[0-9a-f]{64}' | head -1)
    echo "  Program ID: $PROGRAM_ID"
  elif [ -z "$PROGRAM_ID" ]; then
    echo "  Skipping deploy (set PROGRAM_BIN=<path> or PROGRAM_ID=<hex> to run this step)"
    echo "  Using distributor_id as program account (testnet demo)"
    PROGRAM_ID="${PROGRAM_ID:-0000000000000000000000000000000000000000000000000000000000000001}"
  fi
  TX=$("$CLAIM_BIN" chain initialize \
    --sequencer "$SEQUENCER" \
    --program-id "$PROGRAM_ID" \
    --distributor-id "$DISTRIBUTOR_ID" \
    --merkle-root "$MERKLE_ROOT" \
    --total-supply 1000000)
  echo "  Initialize tx: $TX"
else
  echo "[3/5] On-chain init skipped (pass --chain to run on-chain steps)"
  echo "      merkle_root=$MERKLE_ROOT, total_supply=1000000"
fi

echo ""
echo "[4/5] Generating claim proof (account_id is a private input -- never in output)..."
"$CLAIM_BIN" prove \
  --account-id "$ACCOUNT_ID" \
  --allocation "$ALLOCATION" \
  --distributor-id "$DISTRIBUTOR_ID" \
  --merkle-root "$MERKLE_ROOT" \
  --leaf-index 0 \
  --merkle-path "$LEAF" \
  --recipient-note "$RECIPIENT_NOTE" \
  --out /tmp/claim-receipt.bin

echo ""
echo "[5/5] Verifying receipt offline..."
"$CLAIM_BIN" verify \
  --receipt /tmp/claim-receipt.bin \
  --distributor-id "$DISTRIBUTOR_ID" \
  --merkle-root "$MERKLE_ROOT" \
  --recipient-note "$RECIPIENT_NOTE"

echo ""
if [ "$CHAIN_MODE" = "1" ]; then
  echo "  Submitting claim on-chain..."
  TX=$("$CLAIM_BIN" chain claim \
    --sequencer "$SEQUENCER" \
    --program-id "$PROGRAM_ID" \
    --distributor-id "$DISTRIBUTOR_ID" \
    --receipt /tmp/claim-receipt.bin \
    --recipient-note "$RECIPIENT_NOTE")
  echo "  Claim tx: $TX"
fi

echo ""
echo "=== Demo complete ==="
echo "Receipt: /tmp/claim-receipt.bin"
echo ""
if [ "$CHAIN_MODE" = "0" ]; then
  echo "To run the full on-chain demo:"
  echo "  cargo build --release --features chain"
  echo "  SEQUENCER=https://testnet.lez.logos.co ./demo.sh --dev --chain"
fi
echo ""
echo "Privacy properties:"
echo "  - account_id is a private RISC0 input and never leaves this machine"
echo "  - nullifier = SHA256(account_id || distributor_id) prevents double-claim"
echo "  - Merkle leaf/node domain tags prevent second-preimage attacks"
echo "  - recipient_note_hash bound in circuit -- proof cannot be replayed to a different recipient"
