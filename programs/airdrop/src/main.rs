//! Guest binary entry point for LP-0003 private airdrop on-chain program.
//! Deploy with: cargo +risc0 build --release --target riscv32im-risc0-zkvm-elf
//!              wallet deploy-program target/riscv32im-risc0-zkvm-elf/release/private_airdrop
//!
//! Proof verification model: the claim receipt is passed as a zkVM assumption
//! (not embedded receipt_bytes). The SPEL program calls env::verify(IMAGE_ID, journal_words)
//! to bind the proof to this specific circuit, which is the correct LEZ-native pattern.
//! See: lez-build/program_methods/guest/src/bin/privacy_preserving_circuit.rs

#![no_main]

use borsh::BorshDeserialize;
use nssa_core::account::{AccountWithMetadata, Data};
use private_airdrop_program::{apply_claim, ClaimJournal, DistributionState, ERR_PROOF_INVALID};
use risc0_zkvm::guest::env;
use spel_framework::prelude::*;

include!(concat!(env!("OUT_DIR"), "/methods.rs"));
use PRIVATE_AIRDROP_GUEST_ID as IMAGE_ID;

risc0_zkvm::guest::entry!(main);

#[lez_program]
mod airdrop {

    /// Initialize a new distribution with a Merkle root and total supply.
    #[instruction]
    pub fn initialize(
        #[account(init)] mut distribution_account: AccountWithMetadata,
        merkle_root: [u8; 32],
        total_supply: u128,
    ) -> SpelResult {
        let state = DistributionState {
            merkle_root,
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

    /// Claim tokens using a RISC0 ZK proof of Merkle membership.
    ///
    /// The receipt for IMAGE_ID must be provided as a zkVM assumption before calling.
    /// `journal_bytes` is the borsh-serialized journal: [u8;32] root, [u8;32] nullifier,
    /// [u8;32] distributor_id, u128 allocation, [u8;32] recipient_note_hash.
    /// `recipient_note` is the opaque encrypted note to credit to the claimant.
    #[instruction]
    pub fn claim(
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

        // Verify the zkVM assumption: the receipt for this circuit committed to journal_bytes.
        // The caller must add the receipt as an assumption before submitting the instruction.
        let journal_words: Vec<u32> =
            risc0_zkvm::serde::to_vec(&journal_bytes).map_err(|_| SpelError::Custom {
                code: ERR_PROOF_INVALID,
                message: "journal serialise failed".to_string(),
            })?;
        env::verify(IMAGE_ID, &journal_words).map_err(|_| SpelError::Custom {
            code: ERR_PROOF_INVALID,
            message: "assumption verification failed".to_string(),
        })?;

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
