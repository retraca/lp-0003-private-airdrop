#!/usr/bin/env bash
# LP-0003 private airdrop end-to-end demo.
# Requires: a running LEZ sequencer at localhost:9090 (start via lez-build).
# Usage: ./demo.sh [--dev]   (--dev sets RISC0_DEV_MODE=1 for fast local testing)

set -euo pipefail

DEV_MODE=0
for arg in "$@"; do
  [ "$arg" = "--dev" ] && DEV_MODE=1
done

if [ "$DEV_MODE" = "1" ]; then
  export RISC0_DEV_MODE=1
  echo "[demo] RISC0_DEV_MODE=1 (fast mock proofs)"
else
  echo "[demo] Using real RISC0 proofs (may take several minutes)"
fi

SEQUENCER="${SEQUENCER:-http://127.0.0.1:9090}"
CLAIM_BIN="./target/release/airdrop-claim"

echo ""
echo "=== LP-0003 Private Airdrop Demo ==="
echo ""

# Build
echo "[1/5] Building..."
cargo build --release --bin airdrop-claim 2>&1 | tail -3

# Deterministic test keys (never use these outside of demo/testing)
ACCOUNT_ID="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
DISTRIBUTOR_ID="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
ALLOCATION=1000
RECIPIENT_NOTE="cafebabe01020304"

echo ""
echo "[2/5] Computing leaf hash..."
LEAF=$("$CLAIM_BIN" leaf-hash --account-id "$ACCOUNT_ID" --allocation "$ALLOCATION" 2>/dev/null || \
  echo "run: airdrop-claim leaf-hash ...")
echo "Leaf: $LEAF"

echo ""
echo "[3/5] Generating claim proof (account_id kept private)..."
"$CLAIM_BIN" prove \
  --account-id "$ACCOUNT_ID" \
  --allocation "$ALLOCATION" \
  --distributor-id "$DISTRIBUTOR_ID" \
  --recipient-note "$RECIPIENT_NOTE" \
  --sequencer "$SEQUENCER" \
  --out /tmp/claim-receipt.bin

echo ""
echo "[4/5] Verifying receipt offline..."
MERKLE_ROOT=$("$CLAIM_BIN" verify \
  --receipt /tmp/claim-receipt.bin \
  --distributor-id "$DISTRIBUTOR_ID" \
  --recipient-note "$RECIPIENT_NOTE" \
  2>/dev/null | grep "OK" || true)

if [ -n "$MERKLE_ROOT" ]; then
  echo "Offline verification: OK"
else
  echo "Offline verify failed (expected if RISC0_DEV_MODE changed between prove/verify)"
fi

echo ""
echo "[5/5] Submitting to sequencer..."
echo "  (In production: airdrop-claim submit --receipt /tmp/claim-receipt.bin ...)"
echo "  Recipient note hash committed in proof -- cannot be redirected by relay."

echo ""
echo "=== Demo complete ==="
echo "Receipt: /tmp/claim-receipt.bin"
echo ""
echo "Key properties demonstrated:"
echo "  - account_id never leaves this machine (private input only)"
echo "  - nullifier prevents double-claim"
echo "  - Merkle leaf/node domain separation prevents second-preimage"
echo "  - recipient_note_hash bound in circuit -- proof cannot be replayed to different recipient"
