//! Guest binary entry point for LP-0003 private airdrop on-chain program.
//! Deploy with: cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf
//!              then package with scripts/package_r0bf.py and `wallet deploy-program`.
//!
//! Proof model: claims arrive as chained calls from the claim-circuit program
//! (`programs/claim_circuit`) inside a privacy-preserving transaction. The PPE
//! outer circuit proves the chained-call linkage, so `claim` only needs to
//! check `ctx.caller_program_id` against the registered claim-circuit program.
//! No receipt or env::verify is needed here.

#![no_main]

use borsh::BorshDeserialize;
use nssa_core::account::{AccountWithMetadata, Data};
use private_airdrop_program::{
    apply_claim, ClaimJournal, DistributionState, ERR_PROOF_INVALID, ERR_UNAUTHORIZED_CALLER,
};
use spel_framework::prelude::*;

risc0_zkvm::guest::entry!(main);

#[lez_program]
mod airdrop {

    /// Initialize a new distribution with a Merkle root, total supply, and the
    /// program ID of the claim-circuit program authorized to deliver claims.
    #[instruction]
    pub fn initialize(
        #[account(init)] mut distribution_account: AccountWithMetadata,
        merkle_root: [u8; 32],
        claim_circuit_program_id: [u32; 8],
        total_supply: u128,
    ) -> SpelResult {
        let state = DistributionState {
            merkle_root,
            claim_circuit_program_id,
            total_supply,
            claimed: 0,
            spent_nullifiers: Vec::new(),
        };
        distribution_account.account.data =
            Data::try_from(borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?)
            .map_err(|e| SpelError::SerializationError {
                message: format!("state too large: {e:?}"),
            })?;
        Ok(SpelOutput::execute(vec![distribution_account], vec![]))
    }

    /// Claim tokens. Callable ONLY as a chained call from the registered
    /// claim-circuit program: the PPE outer circuit proves the caller identity,
    /// and the claim-circuit program only emits this call after verifying the
    /// claimant's Merkle inclusion proof.
    ///
    /// `journal_bytes` is the borsh-serialized journal: [u8;32] root,
    /// [u8;32] nullifier, [u8;32] distributor_id, u128 allocation,
    /// [u8;32] recipient_note_hash.
    /// `recipient_note` is the opaque note to credit to the claimant.
    #[instruction]
    pub fn claim(
        ctx: ProgramContext,
        #[account(mut)] mut distribution_account: AccountWithMetadata,
        journal_bytes: Vec<u8>,
        recipient_note: Vec<u8>,
    ) -> SpelResult {
        let mut state =
            DistributionState::try_from_slice(distribution_account.account.data.as_ref())
                .map_err(|_| SpelError::Custom {
                    code: ERR_PROOF_INVALID,
                    message: "state deserialise failed".to_string(),
                })?;

        // The inclusion proof is the caller itself: only the registered
        // claim-circuit program emits this chained call, and the PPE circuit
        // guarantees caller_program_id cannot be spoofed.
        if ctx.caller_program_id != state.claim_circuit_program_id {
            return Err(SpelError::Custom {
                code: ERR_UNAUTHORIZED_CALLER,
                message: "claim must arrive as a chained call from the claim-circuit program"
                    .to_string(),
            });
        }

        // Decode journal fields.
        #[derive(borsh::BorshDeserialize)]
        struct ClaimJournalRaw {
            merkle_root: [u8; 32],
            nullifier: [u8; 32],
            distributor_id: [u8; 32],
            allocation: u128,
            recipient_note_hash: [u8; 32],
        }

        let j = ClaimJournalRaw::try_from_slice(&journal_bytes).map_err(|_| SpelError::Custom {
            code: ERR_PROOF_INVALID,
            message: "journal decode failed".to_string(),
        })?;

        let journal = ClaimJournal {
            merkle_root: j.merkle_root,
            nullifier: j.nullifier,
            distributor_id: j.distributor_id,
            allocation: j.allocation,
            recipient_note_hash: j.recipient_note_hash,
        };

        let dist_id_bytes: [u8; 32] = *distribution_account.account_id.value();
        apply_claim(&mut state, &journal, dist_id_bytes, &recipient_note)?;

        distribution_account.account.data =
            Data::try_from(borsh::to_vec(&state).map_err(|e| SpelError::SerializationError {
                message: e.to_string(),
            })?)
            .map_err(|e| SpelError::SerializationError {
                message: format!("state too large: {e:?}"),
            })?;

        Ok(SpelOutput::execute(vec![distribution_account], vec![]))
    }
}
