# LP-0003 Private Airdrop

Private airdrop protocol for the Logos Execution Zone. Claimants prove Merkle membership without revealing their account identity.

## Design

**Privacy**: `account_id` is a private RISC0 guest input. The on-chain verifier never sees it.

**Anti-double-claim (nullifier)**: `SHA256(account_id || distributor_id)`. Bound to both the claimant and the distribution -- re-submitting produces the same nullifier, rejected on-chain.

**Recipient binding**: `recipient_note_hash = SHA256(recipient_note)` is committed in the RISC0 journal. A relay that intercepts the receipt and substitutes a different destination fails the on-chain hash check.

**Merkle domain separation**: Leaf nodes use `SHA256(0x00 || ...)`, internal nodes use `SHA256(0x01 || ...)`. Prevents second-preimage attacks.

## Components

| Path | Role |
|------|------|
| `circuit/guest` | RISC0 zkVM guest circuit |
| `circuit/host` | Off-chain CLI: `airdrop-claim prove / verify` |
| `programs/airdrop` | LEZ on-chain verifier program |
| `sdk` | Client SDK (`submit_claim`, `leaf_hash`, `node_hash`) |

## Quick start

```bash
# Start local chain (from lez-build)
docker compose up -d

# Run demo (RISC0_DEV_MODE=1 for instant mock proofs)
./demo.sh --dev

# Full proofs (takes ~10 min)
./demo.sh
```

## CLI usage

```bash
# Prove membership and generate receipt
airdrop-claim prove \
  --account-id <hex> \
  --allocation 1000 \
  --distributor-id <hex> \
  --recipient-note <hex> \
  --out receipt.bin

# Verify offline
airdrop-claim verify \
  --receipt receipt.bin \
  --distributor-id <hex> \
  --merkle-root <hex> \
  --recipient-note <hex>
```

## Error codes

| Code | Meaning |
|------|---------|
| 7001 | ERR_PROOF_INVALID |
| 7002 | ERR_DISTRIBUTOR_MISMATCH |
| 7003 | ERR_ROOT_MISMATCH |
| 7004 | ERR_NULLIFIER_SPENT |
| 7005 | ERR_DISTRIBUTION_EXHAUSTED |
| 7006 | ERR_RECIPIENT_MISMATCH |

## License

MIT or Apache-2.0
