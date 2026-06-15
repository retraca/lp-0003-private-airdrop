# LP-0003 Privacy Model

This document formalises the threat model for the private airdrop distributor.
It covers what each observer learns in each protocol phase, states the residual
leakage the design accepts, and lists the security assumptions the privacy
guarantees rest on.

---

## Threat-model table

Rows are protocol phases.  
Columns are adversary roles (each is assumed to be computationally bounded and
to control the described vantage point but nothing more).

| Phase | On-chain observer (passive) | Distributor (knows the allowlist) | Other claimants (eligible) |
|---|---|---|---|
| **Deploy** | Sees program bytecode (deterministically built; no secrets). Sees program IDs `airdrop` and `claim_circuit`. | Same as on-chain observer. | Same as on-chain observer. |
| **Initialize (public tx)** | Sees: Merkle root of the eligibility tree, `claim_circuit` program ID pinned in state, `total_supply`. Does NOT see: individual allocations, any account IDs in the tree. | Knows the full eligibility tree (they built it), so can correlate root → individual allocations. | Sees only the Merkle root. Cannot recover allocations or account IDs without the preimage. |
| **Commit (PPE execution, client-side)** | No on-chain event at this sub-step. The claimant proves the claim-circuit program locally. Private inputs (`account_id`, `allocation`, Merkle path, note preimage) never leave the claimant's machine. | No information revealed; nothing is broadcast yet. | No information revealed. |
| **Claim (PPE tx on-chain)** | Sees: nullifier `SHA256(account_id ‖ distributor_id)`, allocation amount, recipient-note hash, one zero-balance "claimer note" account (makes the claim look like any private transfer). Does NOT see: `account_id` or which leaf was claimed. | Sees the allocation amount; already knew it from the tree. Can attempt to correlate nullifier ← SHA256(account_id ‖ distributor_id) for every account in the list (brute-force preimage search over the allowlist). See residual leakage below. | Sees the same public fields as any on-chain observer. Cannot identify the claimant. |
| **Post-claim (chain state)** | Sees: spent nullifiers, aggregate `claimed` total. Cannot link nullifiers to identities without the allowlist. | Can confirm which of their listed accounts have claimed by testing SHA256(account_id ‖ distributor_id) against spent nullifiers. | Same as on-chain observer. |

---

## Residual leakage the design accepts

1. **Allocation amount is public.** The claimed amount appears in the PPE
   journal and the distribution state. An on-chain observer learns *someone*
   claimed *N* tokens from distributor *D* at time *T*. If allocations are
   unique across the list, amount leakage lets the distributor identify the
   claimant with certainty. If allocations are not unique, the distributor
   learns only the subset of candidates with that allocation.

2. **Nullifier is linkable by the distributor.** Because nullifiers are
   `SHA256(account_id ‖ distributor_id)` and the distributor holds the full
   allowlist, it can test every account and learn which accounts have claimed.
   This is a deliberate design choice: it lets the distributor enforce a
   single-claim-per-account invariant without relying on the distributor
   storing per-account state. An on-chain passive observer who does not have
   the allowlist cannot perform this test.

3. **Timing and sequencing.** Submission timestamp and tx ordering are public.
   A well-resourced adversary may correlate claim timing with off-chain signals
   (e.g., social announcements).

4. **Allocation-to-account rebinding is prevented.** The recipient-note hash
   is bound in the proof: a valid receipt cannot be replayed to a different
   recipient. The claim-circuit program is pinned in the distribution state at
   `initialize`, so a modified circuit cannot forge proofs.

---

## Security assumptions

| Assumption | Scope |
|---|---|
| **RISC0 soundness** | The proving system is computationally sound: a PPT adversary cannot produce a valid receipt for a false statement. The guarantee degrades in `RISC0_DEV_MODE=1` (mock proofs, used in CI only). |
| **SHA-256 collision and preimage resistance** | Leaf hashing `SHA256(0x00 ‖ account_id ‖ allocation_le)` and node hashing `SHA256(0x01 ‖ left ‖ right)` rely on SHA-256 second-preimage resistance. Domain tags `0x00`/`0x01` prevent cross-level collisions (length-extension is not an issue for fixed-length inputs). |
| **Nullifier uniqueness** | `SHA256(account_id ‖ distributor_id)` is collision-resistant given distinct `account_id` values within a tree. The distributor must ensure account IDs are unique across the eligibility list. |
| **Private inputs stay private** | The claimant's machine is trusted for the duration of the proving step. The claim-circuit program's initial-call instruction data (account_id, allocation, path, note preimage) is marked as private RISC0 host input. The PPE outer circuit does not include it in the public journal. |
| **LEZ sequencer correctness** | The sequencer verifies the succinct receipt before applying state diffs. A compromised sequencer could apply false state, but cannot forge a valid receipt without breaking RISC0 soundness. |
| **Distributor key bootstrap** | The distribution account is initialized with a one-time signing key (chain keygen). After the `initialize` transaction, the account is program-owned and the bootstrap key has no further authority. If the key is compromised before `initialize` lands, an adversary could race to initialize with a different Merkle root. Mitigations: submit the `initialize` tx promptly; verify the root on-chain before advertising the distributor ID. |

---

## What this design does NOT provide

- **Sender anonymity for the distributor.** The `initialize` transaction is
  public; the deployer's key signs it.
- **Forward secrecy for the allowlist.** If the allowlist leaks later, spent
  nullifiers become identifiable retroactively.
- **Hiding claim timing.** Block timestamps are public.
- **Hiding which distribution a claim belongs to.** The distributor ID appears
  in every claim transaction and the nullifier derivation.
