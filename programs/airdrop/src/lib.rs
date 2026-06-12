//! On-chain airdrop distributor program for LP-0003.
//!
//! Architecture (chained-call composition over the LEZ privacy-preserving
//! execution pipeline):
//!   - The distributor commits to an eligibility Merkle tree at initialization.
//!   - To claim, a recipient submits a privacy-preserving transaction whose
//!     initial call is the claim-circuit program (`programs/claim_circuit`).
//!     That program receives the claimant's account_id, allocation, Merkle
//!     path, and recipient-note preimage as PRIVATE inputs (initial-call
//!     instruction data never appears on-chain), verifies Merkle inclusion
//!     against the root stored in the distribution account, derives the
//!     nullifier, and declares a ChainedCall into this program's `claim`
//!     instruction.
//!   - `claim` trusts its caller identity: the PPE outer circuit proves the
//!     chained-call linkage (caller_program_id cannot be spoofed), so checking
//!     `ctx.caller_program_id == state.claim_circuit_program_id` is equivalent
//!     to verifying the inclusion proof itself.
//!   - Per-distribution nullifiers prevent double-claiming.

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256};
use spel_framework::error::SpelError;

fn sha2_hash(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub const ERR_PROOF_INVALID: u32 = 7001;
pub const ERR_DISTRIBUTOR_MISMATCH: u32 = 7002;
pub const ERR_ROOT_MISMATCH: u32 = 7003;
pub const ERR_NULLIFIER_SPENT: u32 = 7004;
pub const ERR_DISTRIBUTION_EXHAUSTED: u32 = 7005;
pub const ERR_RECIPIENT_MISMATCH: u32 = 7006;
/// `claim` was invoked by something other than the registered claim-circuit
/// program. Only chained calls from that program carry a valid inclusion proof.
pub const ERR_UNAUTHORIZED_CALLER: u32 = 7007;

/// Per-distribution state stored on-chain.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct DistributionState {
    /// Merkle root of the (account_id, allocation) commitment tree.
    pub merkle_root: [u8; 32],
    /// Program ID of the claim-circuit program authorized to deliver claims
    /// (chained-call caller). Set once at initialization.
    pub claim_circuit_program_id: [u32; 8],
    /// Total tokens available in this distribution.
    pub total_supply: u128,
    /// Tokens already claimed.
    pub claimed: u128,
    /// Spent nullifiers: prevents double-claiming.
    pub spent_nullifiers: Vec<[u8; 32]>,
}

/// Decoded claim journal (public outputs of the claim circuit, delivered as
/// chained-call instruction data).
pub struct ClaimJournal {
    pub merkle_root: [u8; 32],
    pub nullifier: [u8; 32],
    pub distributor_id: [u8; 32],
    pub allocation: u128,
    pub recipient_note_hash: [u8; 32],
}

/// Core claim validation against mutable distribution state.
/// Separated from instruction plumbing so the state-machine logic is
/// unit-testable without a chained-call context.
pub fn apply_claim(
    state: &mut DistributionState,
    journal: &ClaimJournal,
    distributor_id: [u8; 32],
    recipient_note: &[u8],
) -> Result<(), SpelError> {
    if journal.distributor_id != distributor_id {
        return Err(SpelError::Custom { code: ERR_DISTRIBUTOR_MISMATCH, message: "distributor mismatch".to_string() });
    }

    if journal.merkle_root != state.merkle_root {
        return Err(SpelError::Custom { code: ERR_ROOT_MISMATCH, message: "merkle root mismatch".to_string() });
    }

    // Recipient binding before any state mutation: a relay that swaps the
    // destination after intercepting the claim fails here.
    let expected_hash: [u8; 32] = sha2_hash(recipient_note);
    if expected_hash != journal.recipient_note_hash {
        return Err(SpelError::Custom { code: ERR_RECIPIENT_MISMATCH, message: "recipient mismatch".to_string() });
    }

    if state.spent_nullifiers.contains(&journal.nullifier) {
        return Err(SpelError::Custom { code: ERR_NULLIFIER_SPENT, message: "nullifier already spent".to_string() });
    }

    if state.claimed + journal.allocation > state.total_supply {
        return Err(SpelError::Custom { code: ERR_DISTRIBUTION_EXHAUSTED, message: "distribution exhausted".to_string() });
    }

    state.spent_nullifiers.push(journal.nullifier);
    state.claimed += journal.allocation;

    Ok(())
}
