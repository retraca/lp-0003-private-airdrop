use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hex::FromHex;
use std::path::PathBuf;

mod prover;
use prover::{fetch_claim_proof, leaf_hash, prove, ProverInput};

#[derive(Parser)]
#[command(name = "airdrop-claim", about = "LP-0003 private airdrop claim CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a claim proof.
    Prove {
        /// Account ID (64-char hex) -- kept private; never leaves this machine.
        #[arg(long)]
        account_id: String,
        /// Token allocation for this account (u128).
        #[arg(long)]
        allocation: u128,
        /// Distributor account ID (64-char hex).
        #[arg(long)]
        distributor_id: String,
        /// Recipient note bytes (hex) -- the private token commitment to receive.
        #[arg(long)]
        recipient_note: String,
        /// Sequencer JSON-RPC URL.
        #[arg(long, default_value = "http://127.0.0.1:9090")]
        sequencer: String,
        /// Write receipt bytes to this file.
        #[arg(long, default_value = "claim-receipt.bin")]
        out: PathBuf,
    },
    /// Verify a claim receipt offline.
    Verify {
        /// Path to receipt file.
        #[arg(long)]
        receipt: PathBuf,
        /// Distributor account ID (64-char hex).
        #[arg(long)]
        distributor_id: String,
        /// Merkle root to check against (64-char hex).
        #[arg(long)]
        merkle_root: String,
        /// Recipient note bytes (hex) -- must match what was used at prove time.
        #[arg(long)]
        recipient_note: String,
    },
}

fn parse_hex32(s: &str) -> Result<[u8; 32]> {
    let bytes = Vec::from_hex(s).context("invalid hex")?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("expected 32 bytes, got {}", bytes.len()))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Prove { account_id, allocation, distributor_id, recipient_note, sequencer, out } => {
            let acct = parse_hex32(&account_id)?;
            let dist = parse_hex32(&distributor_id)?;
            let note_bytes = Vec::from_hex(&recipient_note).context("invalid recipient_note hex")?;

            let lh = leaf_hash(&acct, allocation);
            eprintln!("Leaf hash: {}", hex::encode(lh));
            eprintln!("Fetching claim proof from {}...", sequencer);

            let (merkle_root, leaf_index, merkle_path) =
                fetch_claim_proof(&sequencer, &dist, &lh).await?;
            eprintln!("Merkle root: {}", hex::encode(merkle_root));

            let input = ProverInput {
                account_id_bytes: acct,
                allocation,
                merkle_path,
                leaf_index,
                merkle_root,
                distributor_id: dist,
                recipient_note_preimage: note_bytes,
            };

            eprintln!("Running RISC0 prover...");
            let receipt = prove(input)?;

            let receipt_words = risc0_zkvm::serde::to_vec(&receipt)
                .map_err(|e| anyhow::anyhow!("serialise: {e}"))?;
            let receipt_bytes: Vec<u8> = bytemuck::cast_slice(&receipt_words).to_vec();
            std::fs::write(&out, &receipt_bytes)?;
            eprintln!("Receipt written to {}", out.display());

            // Print the journal for inspection.
            #[derive(serde::Deserialize)]
            struct Journal { nullifier: [u8; 32], allocation: u128, recipient_note_hash: [u8; 32] }
            let j: Journal = receipt.journal.decode()?;
            println!("nullifier:           {}", hex::encode(j.nullifier));
            println!("allocation:          {}", j.allocation);
            println!("recipient_note_hash: {}", hex::encode(j.recipient_note_hash));
        }

        Cmd::Verify { receipt, distributor_id, merkle_root, recipient_note } => {
            include!(concat!(env!("OUT_DIR"), "/methods.rs"));

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
