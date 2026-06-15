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
| `programs/airdrop` | LEZ on-chain program (`initialize`, `claim`) |
| `programs/claim_circuit` | Claim-circuit program: Merkle inclusion proof + chained claim delivery |
| `circuit/guest` | Standalone RISC0 circuit for offline claim receipts (off-chain coordination) |
| `circuit/host` | CLI: offline (`prove / verify`) + on-chain (`chain keygen / initialize / claim / state`) |
| `sdk` | Client SDK (`submit_claim`, `leaf_hash`, `node_hash`) |

## Quick start

```bash
./demo.sh --dev                 # offline: tree, claim proof, verification

# full on-chain lifecycle against the hosted testnet (real proofs):
cargo build --release --features chain
SEQUENCER=https://testnet.lez.logos.co ./demo.sh --chain
```

## On-chain usage

```bash
# 0. Generate the distributor account key (one-time bootstrap credential)
airdrop-claim chain keygen
# -> signing_key + distributor_id

# 1. Initialize the distribution (signed; commits Merkle root, registers the
#    claim-circuit program)
airdrop-claim chain initialize --sequencer <url> \
  --program-id <airdrop-program-id> \
  --claim-circuit-program-id <claim-circuit-program-id> \
  --signing-key <hex> --merkle-root <hex> --total-supply 1000000

# 2. Claim: one privacy-preserving transaction per recipient.
#    Proving runs locally; the account_id and Merkle path never leave the machine.
airdrop-claim chain claim --sequencer <url> --program-id <hex> \
  --distributor-id <hex> --account-id <hex> --allocation <n> \
  --leaf-index <i> --merkle-path <hex,hex,...> --recipient-note <hex>

# Inspect state at any point
airdrop-claim chain state --sequencer <url> --distributor-id <hex>
```

Deployed on the hosted LEZ testnet (`https://testnet.lez.logos.co`) — program IDs, distributions, and all claim transaction hashes in [docs/TESTNET_EVIDENCE.md](docs/TESTNET_EVIDENCE.md).

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
| 7007 | ERR_UNAUTHORIZED_CALLER |

## License

MIT or Apache-2.0

## Privacy model & benchmarks

- [docs/PRIVACY_MODEL.md](docs/PRIVACY_MODEL.md) — full threat model: what on-chain observers, the distributor, and other claimants learn at each phase; residual leakage; security assumptions.
- [docs/BENCHMARKS.md](docs/BENCHMARKS.md) — reproducible CU-cost and proof-time methodology with measured numbers from the testnet evidence run.

## FURPS self-assessment

See [docs/FURPS.md](docs/FURPS.md) for the Functionality / Usability / Reliability / Performance / Supportability self-assessment.
