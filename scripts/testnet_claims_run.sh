#!/usr/bin/env bash
# LP-0003 testnet evidence run: 2 distributions, 20 unique real-proof claims.
#
# Each claim is a privacy-preserving transaction proven locally with
# RISC0_DEV_MODE=0 (expect ~10 minutes per claim). Writes all transaction
# hashes and per-claim data to $EVIDENCE_OUT as it goes, so a partial run
# still leaves usable evidence.
#
# Usage: SEQUENCER=https://testnet.lez.logos.co ./scripts/testnet_claims_run.sh

set -euo pipefail

SEQUENCER="${SEQUENCER:-https://testnet.lez.logos.co}"
BIN="${BIN:-./target/release/airdrop-claim}"
ADPID="${ADPID:-641e17aa9ac2c393a01d4cdf3d12621c1816466b685e0b6993a760c16f5d2e8f}"
CCPID="${CCPID:-2919d161b729ec935b5ef5cc40b319fda02ad6d81df81f0245f5308b86b7fcd8}"
EVIDENCE_OUT="${EVIDENCE_OUT:-/tmp/lp0003-claims-evidence.md}"
CLAIMS_PER_DIST="${CLAIMS_PER_DIST:-10}"

export RISC0_DEV_MODE=0

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$EVIDENCE_OUT.log"; }

# Parse the on-chain spent-nullifier count from distribution state.
nullifier_count() {
  "$BIN" chain state --sequencer "$SEQUENCER" --distributor-id "$1" 2>/dev/null \
    | python3 -c '
import sys
lines = sys.stdin.read().split("\n")
try:
    data = lines[1].split(": ")[1]
    b = bytes.fromhex(data)
    # root 32 + ccpid 32 + supply 16 + claimed 16 + count 4
    print(int.from_bytes(b[96:100], "little"))
except Exception:
    print(-1)
'
}

# Generate tree data for a distribution: 16-leaf tree, first N leaves are
# claimants (deterministic demo account ids), rest are padding.
# Output lines: ROOT, then per-claimant: idx|account_id|allocation|path(comma hex)
tree_data() {
  python3 - "$1" "$CLAIMS_PER_DIST" <<'EOF'
import hashlib, sys
dist_tag, n = sys.argv[1], int(sys.argv[2])
def H(b): return hashlib.sha256(b).digest()
def leaf(acct, alloc): return H(b'\x00' + acct + alloc.to_bytes(16, 'little'))
def node(l, r): return H(b'\x01' + l + r)

claimants = []
for i in range(n):
    acct = H(f"lp0003-demo-claimant/{dist_tag}/{i}".encode())
    alloc = 100 * (i + 1)
    claimants.append((acct, alloc))

leaves = [leaf(a, v) for a, v in claimants]
while len(leaves) < 16:
    leaves.append(leaves[-1])

levels = [leaves]
while len(levels[-1]) > 1:
    cur = levels[-1]
    levels.append([node(cur[i], cur[i+1]) for i in range(0, len(cur), 2)])
root = levels[-1][0]

print(root.hex())
for i, (acct, alloc) in enumerate(claimants):
    path = []
    idx = i
    for lvl in levels[:-1]:
        sib = idx ^ 1
        path.append(lvl[sib].hex())
        idx >>= 1
    print(f"{i}|{acct.hex()}|{alloc}|{','.join(path)}")
EOF
}

run_distribution() {
  local tag="$1"
  log "=== Distribution $tag ==="
  local data; data=$(tree_data "$tag")
  local root; root=$(echo "$data" | head -1)

  local keyout sk did
  keyout=$("$BIN" chain keygen)
  sk=$(echo "$keyout" | grep '^signing_key:' | awk '{print $2}')
  did=$(echo "$keyout" | grep '^distributor_id:' | awk '{print $2}')

  local itx
  itx=$("$BIN" chain initialize --sequencer "$SEQUENCER" --program-id "$ADPID" \
    --claim-circuit-program-id "$CCPID" --signing-key "$sk" \
    --merkle-root "$root" --total-supply 100000 | grep '^tx:' | awk '{print $2}')
  log "dist $tag: distributor=$did root=$root initialize_tx=$itx"
  {
    echo ""
    echo "### Distribution $tag"
    echo ""
    echo "| Field | Value |"
    echo "|---|---|"
    echo "| Distributor account | \`$did\` |"
    echo "| Merkle root (16-leaf tree, $CLAIMS_PER_DIST claimants) | \`$root\` |"
    echo "| Initialize tx | \`$itx\` |"
    echo ""
    echo "| # | Allocation | Nullifier-bearing claim tx |"
    echo "|---|---|---|"
  } >> "$EVIDENCE_OUT"

  for i in $(seq 1 60); do
    "$BIN" chain state --sequencer "$SEQUENCER" --distributor-id "$did" 2>/dev/null \
      | grep -q "program_owner: $ADPID" && break
    sleep 5
  done
  log "dist $tag initialized on-chain"

  echo "$data" | tail -n +2 | while IFS='|' read -r idx acct alloc path; do
    local note="ca11ab1e$(printf '%02x' "$idx")$(echo "$tag" | head -c 2 | xxd -p)"
    log "dist $tag claim $idx (allocation $alloc): proving..."
    local ctx
    ctx=$("$BIN" chain claim --sequencer "$SEQUENCER" --program-id "$ADPID" \
      --distributor-id "$did" --account-id "$acct" --allocation "$alloc" \
      --leaf-index "$idx" --merkle-path "$path" --recipient-note "$note" \
      2>>"$EVIDENCE_OUT.log" | grep '^tx:' | awk '{print $2}')
    log "dist $tag claim $idx tx: $ctx"
    # Each claim's proof commits to the distribution pre-state, so claims must
    # be sequential: wait until this claim's nullifier is on-chain before the
    # next proof starts.
    local want=$((idx + 1)) landed=0
    for w in $(seq 1 36); do
      if [ "$(nullifier_count "$did")" = "$want" ]; then landed=1; break; fi
      sleep 5
    done
    if [ "$landed" != "1" ]; then
      log "ERROR: dist $tag claim $idx did not land (expected nullifier count $want)"
      exit 1
    fi
    log "dist $tag claim $idx landed (nullifiers=$want)"
    echo "| $idx | $alloc | \`$ctx\` |" >> "$EVIDENCE_OUT"
  done

  sleep 30
  local final
  final=$("$BIN" chain state --sequencer "$SEQUENCER" --distributor-id "$did")
  log "dist $tag final state: $final"
  {
    echo ""
    echo "Final state:"
    echo '```'
    echo "$final"
    echo '```'
  } >> "$EVIDENCE_OUT"
}

echo "# LP-0003 testnet claims evidence run ($(date -u +%Y-%m-%dT%H:%MZ))" > "$EVIDENCE_OUT"
echo "" >> "$EVIDENCE_OUT"
echo "Sequencer: $SEQUENCER. Real proofs (RISC0_DEV_MODE=0)." >> "$EVIDENCE_OUT"

run_distribution "alpha"
run_distribution "beta"

log "ALL DONE. Evidence: $EVIDENCE_OUT"
