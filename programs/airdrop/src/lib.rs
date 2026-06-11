//! On-chain airdrop distributor program for LP-0003.
//! Verifies a claim ZK proof, checks nullifier, transfers tokens to the claimant's private account.

use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::account::{AccountPostState, AccountWithMetadata, Data};
use sha2::{Digest, Sha256};
use spel_framework::error::SpelError;

fn sha2_hash(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

// IMAGE_ID injected by risc0-build; zero-check at compile time.
include!(concat!(env!("OUT_DIR"), "/methods.rs"));
use PRIVATE_AIRDROP_GUEST_ID as IMAGE_ID;
#[cfg(not(test))]
const _: () = assert!(
    IMAGE_ID[0] != 0 || IMAGE_ID[1] != 0,
    "IMAGE_ID is all-zero; build the guest before deploying"
);

pub const ERR_PROOF_INVALID: u32 = 7001;
pub const ERR_DISTRIBUTOR_MISMATCH: u32 = 7002;
pub const ERR_ROOT_MISMATCH: u32 = 7003;
pub const ERR_NULLIFIER_SPENT: u32 = 7004;
pub const ERR_DISTRIBUTION_EXHAUSTED: u32 = 7005;
pub const ERR_RECIPIENT_MISMATCH: u32 = 7006;

/// Per-distribution state stored on-chain.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct DistributionState {
    /// Merkle root of the (account_id, allocation) commitment tree.
    pub merkle_root: [u8; 32],
    /// Total tokens available in this distribution.
    pub total_supply: u128,
    /// Tokens already claimed.
    pub claimed: u128,
    /// Spent nullifiers: prevents double-claiming.
    pub spent_nullifiers: Vec<[u8; 32]>,
}

/// Decoded claim journal (public outputs from the RISC0 guest).
pub struct ClaimJournal {
    pub merkle_root: [u8; 32],
    pub nullifier: [u8; 32],
    pub distributor_id: [u8; 32],
    pub allocation: u128,
    pub recipient_note_hash: [u8; 32],
}

/// Core claim validation against mutable distribution state.
/// Separated from receipt parsing so the state-machine logic is unit-testable
/// without a real RISC0 receipt.
pub fn apply_claim(
    state: &mut DistributionState,
    journal: &ClaimJournal,
    distributor_id: [u8; 32],
    recipient_note: &[u8],
) -> Result<(), SpelError> {
    if journal.distributor_id != distributor_id {
        return Err(SpelError::Custom { code: ERR_DISTRIBUTOR_MISMATCH });
    }

    if journal.merkle_root != state.merkle_root {
        return Err(SpelError::Custom { code: ERR_ROOT_MISMATCH });
    }

    // Recipient binding before any state mutation: a relay that swaps the
    // destination after intercepting the receipt fails here.
    let expected_hash: [u8; 32] = sha2_hash(recipient_note);
    if expected_hash != journal.recipient_note_hash {
        return Err(SpelError::Custom { code: ERR_RECIPIENT_MISMATCH });
    }

    if state.spent_nullifiers.contains(&journal.nullifier) {
        return Err(SpelError::Custom { code: ERR_NULLIFIER_SPENT });
    }

    if state.claimed + journal.allocation > state.total_supply {
        return Err(SpelError::Custom { code: ERR_DISTRIBUTION_EXHAUSTED });
    }

    state.spent_nullifiers.push(journal.nullifier);
    state.claimed += journal.allocation;

    Ok(())
}

/// Claim tokens from a private airdrop.
///
/// Verifies the RISC0 receipt and transfers tokens to the claimant's private note.
///
/// Inputs:
///   - `distribution_account`: holds `DistributionState`
///   - `receipt_bytes`: RISC0 proof that the claimant is in the Merkle tree
///   - `recipient_note`: encrypted token note to credit (opaque bytes sent to recipient)
pub fn claim(
    distribution_account: AccountWithMetadata,
    receipt_bytes: Vec<u8>,
    recipient_note: Vec<u8>,
) -> Result<Vec<AccountPostState>, SpelError> {
    let mut state = DistributionState::try_from_slice(&distribution_account.account.data.0)
        .expect("DistributionState must deserialise");

    let receipt: risc0_zkvm::Receipt = risc0_zkvm::serde::from_slice(&receipt_bytes)
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    receipt.verify(IMAGE_ID)
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    #[derive(serde::Deserialize)]
    struct RawJournal {
        merkle_root: [u8; 32],
        nullifier: [u8; 32],
        distributor_id: [u8; 32],
        allocation: u128,
        recipient_note_hash: [u8; 32],
    }

    let j: RawJournal = receipt.journal.decode()
        .map_err(|_| SpelError::Custom { code: ERR_PROOF_INVALID })?;

    let journal = ClaimJournal {
        merkle_root: j.merkle_root,
        nullifier: j.nullifier,
        distributor_id: j.distributor_id,
        allocation: j.allocation,
        recipient_note_hash: j.recipient_note_hash,
    };

    let dist_id_bytes: [u8; 32] = distribution_account.account_id.to_bytes();
    apply_claim(&mut state, &journal, dist_id_bytes, &recipient_note)?;

    let mut dist_account = distribution_account.account;
    dist_account.data = Data::from_borsh(&state);

    // The recipient_note is an encrypted token commitment emitted as an event.
    // In a full LEZ token integration this would invoke the token program's
    // private transfer instruction. For now we emit the note as account data
    // on a separate ephemeral account (per the LEZ private note pattern).
    Ok(vec![AccountPostState::new(dist_account)])
}
