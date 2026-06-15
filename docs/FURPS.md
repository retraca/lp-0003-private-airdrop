# FURPS Self-Assessment — LP-0003 Private Allowlist / Airdrop Distributor

## Functionality
- Distributor commits to an eligibility set as a Merkle root on-chain; individual recipient addresses are never revealed (`programs/airdrop` `initialize`).
- Eligible recipients claim without revealing their address: `account_id` is a private RISC0 guest input delivered as a chained call through the privacy-preserving execution (PPE) pipeline (`programs/claim_circuit`).
- Double-claim prevention: `nullifier = SHA256(account_id || distributor_id)` stored in `spent_nullifiers`; re-submission fails with `ERR_NULLIFIER_SPENT (7004)`.
- On-chain observers cannot link a claim to an address: the only state diff is a fresh nullifier + `claimed += allocation`.
- Demonstrated end-to-end on the hosted LEZ testnet: 2 distributions, 20 unique real-proof claims (see `docs/TESTNET_EVIDENCE.md`).

## Usability
- Reusable SDK (`sdk/`) exposes `submit_claim`, `leaf_hash`, `node_hash`, `ClaimJournal`.
- Logos Basecamp mini-app (`basecamp-app/`) with local build/load instructions; no build step required.
- IDL generated via the SPEL framework (`lp-0003-private-airdrop.idl.json`).
- CLI documented in README (keygen / initialize / claim / state).

## Reliability
- Proof-generation failures propagate as typed errors (`anyhow`); `demo.sh` uses `set -euo pipefail`.
- A failed or rejected claim never marks the claimant as claimed: the nullifier and recipient-note checks run before any state write.
- Deterministic, documented error codes `7001`–`7007` (README table).

## Performance
- Proof time and compute-unit methodology documented with measured numbers in `docs/BENCHMARKS.md` (≈7–10 min/claim client-side on M2; sequencer verification is one succinct receipt).
- Privacy/leakage trade-offs documented in `docs/PRIVACY_MODEL.md`.

## Supportability
- Deployed and exercised on the hosted LEZ testnet with program IDs and tx hashes (`docs/TESTNET_EVIDENCE.md`).
- End-to-end integration tests run against a standalone LEZ sequencer in CI (`.github/workflows/ci.yml`), `RISC0_DEV_MODE=0` real-proof path available.
- README covers deployment steps, program addresses, CLI and Basecamp usage; `demo.sh` is the reproducible end-to-end script.

## Known limitations
- Allocation amounts are public by design (only identity is private).
- The distributor, who holds the allowlist, can correlate nullifiers against known leaves; claim timing is observable. See `docs/PRIVACY_MODEL.md`.
