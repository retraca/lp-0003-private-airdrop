#!/usr/bin/env bash
# LP-0003 private airdrop end-to-end demo.
#
# Runs fully offline: generates a claim proof locally, then verifies it.
# Submitting to a live LEZ chain requires a running sequencer (see README).
#
# Usage: ./demo.sh [--dev]   (--dev sets RISC0_DEV_MODE=1 for fast testing)

set -euo pipefail

DEV_MODE=0
for arg in "$@"; do
  [ "$arg" = "--dev" ] && DEV_MODE=1
done

if [ "$DEV_MODE" = "1" ]; then
  export RISC0_DEV_MODE=1
  echo "[demo] RISC0_DEV_MODE=1 (fast mock proofs, no ZK)"
else
  echo "[demo] Real RISC0 proofs -- proof generation takes several minutes"
fi

CLAIM_BIN="./target/release/airdrop-claim"

echo ""
echo "=== LP-0003 Private Airdrop Demo ==="
echo ""

echo "[1/4] Building..."
cargo build --release --bin airdrop-claim 2>&1 | tail -3

# Deterministic test inputs -- never use these outside demo/testing
ACCOUNT_ID="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
DISTRIBUTOR_ID="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
ALLOCATION=1000
RECIPIENT_NOTE="cafebabe01020304"

# Minimal depth-1 Merkle tree: root = SHA256(0x01 || leaf || leaf)
# (both leaves identical for demo simplicity)
LEAF=$(python3 -c "
import hashlib, struct
acct = bytes.fromhex('$ACCOUNT_ID')
alloc = struct.pack('<16s', struct.pack('<Q', $ALLOCATION)[:8].ljust(16, b'\x00'))
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
echo "[2/4] Tree: leaf=$LEAF"
echo "      root=$MERKLE_ROOT"

echo ""
echo "[3/4] Generating claim proof (account_id is a private input -- never in output)..."
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
echo "[4/4] Verifying receipt offline..."
"$CLAIM_BIN" verify \
  --receipt /tmp/claim-receipt.bin \
  --distributor-id "$DISTRIBUTOR_ID" \
  --merkle-root "$MERKLE_ROOT" \
  --recipient-note "$RECIPIENT_NOTE"

echo ""
echo "=== Demo complete ==="
echo "Receipt: /tmp/claim-receipt.bin"
echo ""
echo "To submit to a running LEZ chain, deploy programs/airdrop and call claim()"
echo "with the receipt bytes and recipient_note."
echo ""
echo "Privacy properties:"
echo "  - account_id is a private RISC0 input and never leaves this machine"
echo "  - nullifier = SHA256(account_id || distributor_id) prevents double-claim"
echo "  - Merkle leaf/node domain tags prevent second-preimage attacks"
echo "  - recipient_note_hash bound in circuit -- proof cannot be replayed to a different recipient"
