//! Host prover for LP-0003 private airdrop.

use anyhow::Result;
use risc0_zkvm::{ExecutorEnv, Receipt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::methods::PRIVATE_AIRDROP_GUEST_ELF;

#[derive(Serialize, Deserialize)]
pub struct ProverInput {
    pub account_id_bytes: [u8; 32],
    pub allocation: u128,
    pub merkle_path: Vec<[u8; 32]>,
    pub leaf_index: usize,
    pub merkle_root: [u8; 32],
    pub distributor_id: [u8; 32],
    pub recipient_note_preimage: Vec<u8>,
}

pub fn prove(input: ProverInput) -> Result<Receipt> {
    let env = ExecutorEnv::builder()
        .write(&input)?
        .build()?;
    let prover = risc0_zkvm::default_prover();
    let receipt = prover.prove(env, PRIVATE_AIRDROP_GUEST_ELF)?.receipt;
    Ok(receipt)
}

/// Compute the leaf hash for an (account_id, allocation) pair.
/// Matches the guest's domain-tagged hash: SHA256(0x00 || account_id || allocation_le).
pub fn leaf_hash(account_id: &[u8; 32], allocation: u128) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(&[0x00u8]);
    h.update(account_id);
    h.update(&allocation.to_le_bytes());
    h.finalize().into()
}
