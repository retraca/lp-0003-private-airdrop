mod methods;
mod prover;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hex::FromHex;
use std::path::PathBuf;

use methods::PRIVATE_AIRDROP_GUEST_ID;
use prover::{leaf_hash, prove, ProverInput};

#[derive(Parser)]
#[command(name = "airdrop-claim", about = "LP-0003 private airdrop claim CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a claim proof offline (provide Merkle proof manually).
    Prove {
        #[arg(long)]
        account_id: String,
        #[arg(long)]
        allocation: u128,
        #[arg(long)]
        distributor_id: String,
        /// Merkle root (64-char hex).
        #[arg(long)]
        merkle_root: String,
        /// Leaf index in the tree.
        #[arg(long)]
        leaf_index: usize,
        /// Sibling nodes from leaf to root (comma-separated hex).
        #[arg(long)]
        merkle_path: String,
        /// Recipient note bytes (hex).
        #[arg(long)]
        recipient_note: String,
        #[arg(long, default_value = "claim-receipt.bin")]
        out: PathBuf,
    },
    /// Verify a claim receipt offline.
    Verify {
        #[arg(long)]
        receipt: PathBuf,
        #[arg(long)]
        distributor_id: String,
        #[arg(long)]
        merkle_root: String,
        #[arg(long)]
        recipient_note: String,
    },
}

fn parse_hex32(s: &str) -> Result<[u8; 32]> {
    let bytes = Vec::from_hex(s).context("invalid hex")?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("expected 32 bytes, got {}", bytes.len()))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Prove {
            account_id, allocation, distributor_id, merkle_root,
            leaf_index, merkle_path, recipient_note, out,
        } => {
            let acct = parse_hex32(&account_id)?;
            let dist = parse_hex32(&distributor_id)?;
            let root = parse_hex32(&merkle_root)?;
            let note_bytes = Vec::from_hex(&recipient_note).context("invalid recipient_note hex")?;

            let path: Result<Vec<[u8; 32]>> = merkle_path
                .split(',')
                .map(|s| parse_hex32(s.trim()))
                .collect();
            let path = path?;

            eprintln!("Leaf hash: {}", hex::encode(leaf_hash(&acct, allocation)));
            eprintln!("Running RISC0 prover...");

            let receipt = prove(ProverInput {
                account_id_bytes: acct,
                allocation,
                merkle_path: path,
                leaf_index,
                merkle_root: root,
                distributor_id: dist,
                recipient_note_preimage: note_bytes,
            })?;

            let words = risc0_zkvm::serde::to_vec(&receipt)
                .map_err(|e| anyhow::anyhow!("serialise: {e}"))?;
            let bytes: Vec<u8> = bytemuck::cast_slice(&words).to_vec();
            std::fs::write(&out, &bytes)?;
            eprintln!("Receipt written to {}", out.display());

            #[derive(serde::Deserialize)]
            struct Journal {
                merkle_root: [u8; 32],
                nullifier: [u8; 32],
                distributor_id: [u8; 32],
                allocation: u128,
                recipient_note_hash: [u8; 32],
            }
            let j: Journal = receipt.journal.decode()?;
            println!("merkle_root:         {}", hex::encode(j.merkle_root));
            println!("distributor_id:      {}", hex::encode(j.distributor_id));
            println!("nullifier:           {}", hex::encode(j.nullifier));
            println!("allocation:          {}", j.allocation);
            println!("recipient_note_hash: {}", hex::encode(j.recipient_note_hash));
        }

        Cmd::Verify { receipt, distributor_id, merkle_root, recipient_note } => {
            let dist = parse_hex32(&distributor_id)?;
            let root = parse_hex32(&merkle_root)?;
            let note_bytes = Vec::from_hex(&recipient_note).context("invalid recipient_note hex")?;

            let raw = std::fs::read(&receipt)?;
            let words: Vec<u32> = raw.chunks_exact(4)
                .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                .collect();
            let r: risc0_zkvm::Receipt = risc0_zkvm::serde::from_slice(&words)
                .map_err(|e| anyhow::anyhow!("deserialise: {e}"))?;

            r.verify(PRIVATE_AIRDROP_GUEST_ID).context("receipt verification failed")?;

            #[derive(serde::Deserialize)]
            struct Journal {
                merkle_root: [u8; 32],
                nullifier: [u8; 32],
                distributor_id: [u8; 32],
                allocation: u128,
                recipient_note_hash: [u8; 32],
            }
            let j: Journal = r.journal.decode()?;
            anyhow::ensure!(j.distributor_id == dist, "distributor_id mismatch");
            anyhow::ensure!(j.merkle_root == root, "merkle_root mismatch");

            use sha2::{Digest, Sha256};
            let expected_hash: [u8; 32] = Sha256::digest(&note_bytes).into();
            anyhow::ensure!(j.recipient_note_hash == expected_hash, "recipient_note hash mismatch");

            println!("OK");
            println!("nullifier:  {}", hex::encode(j.nullifier));
            println!("allocation: {}", j.allocation);
        }
    }

    Ok(())
}
