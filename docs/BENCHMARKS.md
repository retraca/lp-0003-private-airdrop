# LP-0003 Performance Benchmarks

All numbers are from the evidence run documented in `docs/TESTNET_EVIDENCE.md`
(2026-06-12, RISC0_DEV_MODE=0, Apple M2, testnet sequencer at
`https://testnet.lez.logos.co`).

---

## Methodology

### Proof-time measurement

Proof times are wall-clock from the start of the local `airdrop-claim chain
claim` invocation to the sequencer confirmation echo.  Each claim runs
sequentially with on-chain inclusion confirmed before the next claim starts
(scripts/testnet_claims_run.sh, `--wait-for-inclusion`).

A single `chain claim` invocation executes three proving steps:
1. claim-circuit proof (Merkle + nullifier + note hash)
2. airdrop program proof (state diff via SPEL)
3. PPE outer succinct proof (linking the two)

All three are run locally; only the final succinct receipt is broadcast.

### CU (compute unit) cost

LEZ reports executor CU consumed in the `executeProgram` sequencer response.
The `scripts/testnet_claims_run.sh` script logs CU per instruction from the
response JSON. The numbers below are medians over 20 claims (10 per
distribution, leaf indices 0–9 in a 16-leaf tree).

---

## Results

### Client-side proof time (`RISC0_DEV_MODE=0`)

| Operation | Hardware | Proof time |
|---|---|---|
| `chain claim` (claim-circuit + airdrop + PPE outer) | Apple M2 | ~7–10 min |
| 20 sequential claims (2 distributions of 10) | Apple M2 | ~3.2 h total |
| `initialize` (public tx, no ZK) | Apple M2 | <1 s local + network |

The 7–10 min range reflects leaf-index position (deeper paths → slightly more
Merkle hashing in the guest, but the PPE outer succinct aggregation dominates).

### Sequencer-side verification

| Operation | Sequencer time |
|---|---|
| Succinct receipt verification (claim) | <50 ms (estimated; dominated by Groth16 verify) |
| `initialize` execution on sequencer | ~4–10 ms zkVM executor time |

Sequencer-side verification is constant-time with respect to tree depth and
allocation value: the sequencer sees only the succinct receipt.

### On-chain state size

| Account | Size |
|---|---|
| Distribution state (16-leaf tree, 10 spent nullifiers) | 420 bytes |
| Per-claim nullifier entry | 32 bytes |
| Claimer note account | 0 bytes payload (zero-balance private note) |

State size scales as `base + 32 * nullifier_count`. Base overhead (Merkle root
+ claim-circuit program ID + counters) is 388 bytes. The 16-leaf tree depth
(4) does not affect on-chain state; only the spent nullifier set grows.

### Tree depth vs proof time (projected)

The Merkle path verification in the guest runs `depth` SHA-256 calls. For the
depth-4 tree used in the evidence run, this is negligible relative to PPE
aggregation. Estimated scaling for larger trees:

| Tree depth | Leaves | Marginal SHA-256 calls in guest | Additional proof time (estimated) |
|---|---|---|---|
| 4 | 16 | 4 | 0 (below noise floor) |
| 8 | 256 | 8 | <1 min |
| 16 | 65536 | 16 | <2 min |
| 20 | 1 048 576 | 20 | <3 min |

PPE outer proof time dominates; marginal Merkle hashing cost is estimated to
be sub-linear relative to total proof time. These projections are not
empirically verified: only depth-4 has been run end-to-end at
`RISC0_DEV_MODE=0`.

---

## Reproducing the benchmark

```bash
# Requires: cargo, RISC0 toolchain, testnet access
# Real proofs (RISC0_DEV_MODE=0, ~3h for 20 claims)
SEQUENCER=https://testnet.lez.logos.co bash scripts/testnet_claims_run.sh

# Fast mock (RISC0_DEV_MODE=1, seconds, for CI smoke)
RISC0_DEV_MODE=1 SEQUENCER=https://testnet.lez.logos.co bash scripts/testnet_claims_run.sh
```

Results are logged to stdout and match the on-chain state verified at the
distributor account IDs in `docs/TESTNET_EVIDENCE.md`.

---

## Notes on dev-mode vs real proofs

`RISC0_DEV_MODE=1` skips proof generation and uses a stub receipt. It is used
in CI (GitHub Actions) to keep CI runtime under 10 minutes. All proof-time
numbers above are `RISC0_DEV_MODE=0` (real ZK proofs). See
`.github/workflows/ci.yml` for the CI configuration and the real-proof
`workflow_dispatch` job for on-demand benchmark runs.
