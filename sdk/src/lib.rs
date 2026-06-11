//! Client SDK for LP-0003 private airdrop.
//! Provides the types and a submission helper for interacting with the
//! airdrop distributor program on-chain.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The journal committed by the guest circuit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimJournal {
    pub merkle_root: [u8; 32],
    pub nullifier: [u8; 32],
    pub distributor_id: [u8; 32],
    pub allocation: u128,
    pub recipient_note_hash: [u8; 32],
}

/// Submit a claim receipt to the sequencer.
///
/// `receipt_bytes` must be the serialised RISC0 receipt from the prover.
/// `recipient_note` must match exactly what was used at prove time.
pub async fn submit_claim(
    sequencer_url: &str,
    distributor_id: &[u8; 32],
    receipt_bytes: &[u8],
    recipient_note: &[u8],
) -> Result<String> {
    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .post(sequencer_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "submitAirdropClaim",
            "params": {
                "distributor_id": hex::encode(distributor_id),
                "receipt": hex::encode(receipt_bytes),
                "recipient_note": hex::encode(recipient_note),
            },
            "id": 1,
        }))
        .send().await?
        .json().await?;

    if let Some(err) = resp.get("error") {
        anyhow::bail!("sequencer error: {}", err);
    }
    Ok(resp["result"]["tx_hash"].as_str().unwrap_or("").to_string())
}

/// Compute the domain-tagged leaf hash for a (account_id, allocation) pair.
/// Use this to build the off-chain Merkle tree and to verify proofs.
/// Must match the guest circuit exactly: SHA256(0x00 || account_id || allocation_le).
pub fn leaf_hash(account_id: &[u8; 32], allocation: u128) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&[0x00u8]);
    h.update(account_id);
    h.update(&allocation.to_le_bytes());
    h.finalize().into()
}

/// Compute a Merkle parent hash from two children.
/// Must match the guest circuit exactly: SHA256(0x01 || left || right).
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&[0x01u8]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}
