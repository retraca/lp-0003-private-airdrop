//! Host prover for LP-0003 private airdrop.

use anyhow::Result;
use risc0_zkvm::{ExecutorEnv, ProverOpts, Receipt};
use serde::{Deserialize, Serialize};

include!(concat!(env!("OUT_DIR"), "/methods.rs"));

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
    let receipt = prover.prove_with_opts(
        env,
        PRIVATE_AIRDROP_GUEST_ELF,
        &ProverOpts::groth16(),
    )?.receipt;
    Ok(receipt)
}

/// Fetch the Merkle proof for a leaf from the sequencer.
/// Returns `(merkle_root, leaf_index, sibling_nodes)`.
pub async fn fetch_claim_proof(
    sequencer_url: &str,
    distributor_id: &[u8; 32],
    leaf_hash: &[u8; 32],
) -> Result<([u8; 32], usize, Vec<[u8; 32]>)> {
    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .post(sequencer_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "getAirdropProof",
            "params": {
                "distributor_id": hex::encode(distributor_id),
                "leaf_hash": hex::encode(leaf_hash),
            },
            "id": 1,
        }))
        .send().await?
        .json().await?;

    let result = resp.get("result").ok_or_else(|| anyhow::anyhow!("no result in response"))?;

    let root_hex = result["merkle_root"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing merkle_root"))?;
    let root_bytes = hex::decode(root_hex)?;
    let merkle_root = <[u8; 32]>::try_from(root_bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("merkle_root must be 32 bytes"))?;

    let leaf_index = usize::try_from(
        result["leaf_index"].as_u64().ok_or_else(|| anyhow::anyhow!("missing leaf_index"))?
    )?;

    let siblings_raw = result["siblings"].as_array()
        .ok_or_else(|| anyhow::anyhow!("missing siblings"))?;
    let mut siblings = Vec::with_capacity(siblings_raw.len());
    for s in siblings_raw {
        let h = s.as_str().ok_or_else(|| anyhow::anyhow!("sibling must be string"))?;
        let bytes = hex::decode(h)?;
        siblings.push(<[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| anyhow::anyhow!("sibling must be 32 bytes"))?);
    }

    Ok((merkle_root, leaf_index, siblings))
}

/// Compute the leaf hash for an (account_id, allocation) pair.
/// Matches the guest's domain-tagged hash: SHA256(0x00 || account_id || allocation_le).
pub fn leaf_hash(account_id: &[u8; 32], allocation: u128) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&[0x00u8]);
    h.update(account_id);
    h.update(&allocation.to_le_bytes());
    h.finalize().into()
}
