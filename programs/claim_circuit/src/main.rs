//! LP-0003 claim-circuit program: the eligibility proof for private airdrop claims.
//!
//! Runs as the INITIAL call of a privacy-preserving transaction, executed and
//! proven CLIENT-SIDE by the claimant. The instruction data of the initial call
//! (containing the claimant's account_id, allocation, Merkle path, and
//! recipient-note preimage) never appears on-chain: the sequencer sees only the
//! PPE circuit output (public account post-states, commitments, nullifiers).
//!
//! The program:
//!   1. Reads the distribution account state (pre-state supplied by the client
//!      and checked against live chain state by the sequencer at submission).
//!   2. Recomputes the Merkle leaf `SHA256(0x00 || account_id || allocation_le)`
//!      and walks the supplied path with the `0x01` node domain tag, asserting
//!      the computed root equals the root committed in the distribution state.
//!   3. Derives the nullifier `SHA256(account_id || distributor_id)` and the
//!      recipient-note hash binding.
//!   4. Declares a ChainedCall into the airdrop program's `claim` instruction.
//!      The PPE outer circuit proves this linkage; the airdrop program accepts
//!      claims only from this program (caller_program_id check).
//!
//! The journal reveals the distribution, the allocation amount, the nullifier,
//! and the recipient-note hash -- never which eligible account claimed.

#![no_main]

use borsh::BorshDeserialize;
use nssa_core::{account::AccountWithMetadata, program::ChainedCall};
use private_airdrop_program::{DistributionState, ERR_PROOF_INVALID, ERR_ROOT_MISMATCH};
use sha2::{Digest, Sha256};
use spel_framework::prelude::*;

risc0_zkvm::guest::entry!(main);

/// Wire-format mirror of the airdrop program's SPEL-generated instruction
/// enum. Variant order must match the declaration order in
/// `programs/airdrop/src/main.rs`: 0=initialize, 1=claim. Only `Claim` is
/// constructed here.
#[derive(serde::Serialize)]
enum AirdropInstruction {
    #[allow(dead_code)]
    Initialize,
    Claim(Vec<u8>, Vec<u8>),
}

fn leaf_hash(account_id: &[u8; 32], allocation: u128) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x00u8]);
    h.update(account_id);
    h.update(allocation.to_le_bytes());
    h.finalize().into()
}

fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x01u8]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

#[lez_program]
mod claim_circuit {

    /// Prove eligibility and deliver a claim to the airdrop program.
    ///
    /// All argument data is PRIVATE: initial-call instruction data is never
    /// revealed on-chain in a privacy-preserving transaction.
    /// `airdrop_program_id` selects the target program; the target itself pins
    /// this program's ID in its state, so pointing the call elsewhere cannot
    /// forge claims.
    ///
    /// `claimer_note` is a fresh zero-balance private account created by this
    /// transaction (required commitment/nullifier pair; makes a claim look
    /// like any other private transaction). Passed through unchanged.
    #[instruction]
    pub fn submit_claim(
        #[account(mut)] distribution_account: AccountWithMetadata,
        #[account(mut)] claimer_note: AccountWithMetadata,
        airdrop_program_id: [u32; 8],
        account_id: [u8; 32],
        allocation: u128,
        leaf_index: u32,
        merkle_path: Vec<[u8; 32]>,
        recipient_note: Vec<u8>,
    ) -> SpelResult {
        let state = DistributionState::try_from_slice(
            distribution_account.account.data.as_ref(),
        )
        .map_err(|_| SpelError::Custom {
            code: ERR_PROOF_INVALID,
            message: "distribution state deserialise failed".to_string(),
        })?;

        let distributor_id: [u8; 32] = *distribution_account.account_id.value();

        // Merkle inclusion check against the root committed on-chain. This is
        // the heart of the proof -- it can only succeed for a (account_id,
        // allocation) pair in the eligibility tree.
        let mut node = leaf_hash(&account_id, allocation);
        let mut index = leaf_index;
        for sibling in &merkle_path {
            node = if index & 1 == 0 {
                node_hash(&node, sibling)
            } else {
                node_hash(sibling, &node)
            };
            index >>= 1;
        }
        if node != state.merkle_root {
            return Err(SpelError::Custom {
                code: ERR_ROOT_MISMATCH,
                message: "merkle inclusion proof does not match committed root".to_string(),
            });
        }

        // Nullifier: one claim per account per distribution.
        let nullifier: [u8; 32] = {
            let mut h = Sha256::new();
            h.update(account_id);
            h.update(distributor_id);
            h.finalize().into()
        };

        // Recipient binding: the claim is only valid for this note.
        let recipient_note_hash: [u8; 32] = Sha256::digest(&recipient_note).into();

        // borsh ClaimJournal: merkle_root, nullifier, distributor_id,
        // allocation, recipient_note_hash
        let mut journal_bytes = Vec::with_capacity(32 + 32 + 32 + 16 + 32);
        journal_bytes.extend_from_slice(&state.merkle_root);
        journal_bytes.extend_from_slice(&nullifier);
        journal_bytes.extend_from_slice(&distributor_id);
        journal_bytes.extend_from_slice(&allocation.to_le_bytes());
        journal_bytes.extend_from_slice(&recipient_note_hash);

        let chained_call = ChainedCall::new(
            airdrop_program_id,
            vec![distribution_account.clone()],
            &AirdropInstruction::Claim(journal_bytes, recipient_note),
        );

        // Post-states must mirror pre-states 1:1; this program reads both
        // accounts without modifying them -- the chained claim call performs
        // the distribution state update, and the PPE circuit handles the
        // claimer note commitment.
        Ok(SpelOutput::execute(
            vec![distribution_account, claimer_note],
            vec![chained_call],
        ))
    }
}
