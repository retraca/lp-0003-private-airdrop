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
    /// On-chain operations (requires --features chain and lez-build at ../../../lez-build/).
    #[command(subcommand)]
    Chain(ChainCmd),
}

#[derive(Subcommand)]
enum ChainCmd {
    /// Generate a fresh account signing key and print the derived account ID.
    /// Use the account ID as the distributor ID and pass the key to `initialize`.
    Keygen,
    /// Read the distributor account state from the chain.
    State {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        #[arg(long)]
        distributor_id: String,
    },
    /// Initialize the airdrop distributor on-chain.
    ///
    /// Account claiming requires authorization: the transaction must be signed
    /// with the distributor account's key (generate one with `chain keygen`).
    /// The distributor account ID is derived from the signing key.
    Initialize {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        /// Program ID (64-char hex, from wallet deploy-program output).
        #[arg(long)]
        program_id: String,
        /// Account signing key (64-char hex, from `chain keygen`).
        #[arg(long)]
        signing_key: String,
        /// Merkle root of the eligibility tree (64-char hex).
        #[arg(long)]
        merkle_root: String,
        /// Total token supply for this distribution.
        #[arg(long)]
        total_supply: u128,
    },
    /// Submit a claim proof on-chain.
    Claim {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        #[arg(long)]
        program_id: String,
        #[arg(long)]
        distributor_id: String,
        /// Path to claim receipt (output of `airdrop-claim prove --out`).
        #[arg(long)]
        receipt: PathBuf,
        /// Recipient note bytes (hex, same value used when generating the proof).
        #[arg(long)]
        recipient_note: String,
    },
}

fn parse_hex32(s: &str) -> Result<[u8; 32]> {
    let bytes = Vec::from_hex(s).context("invalid hex")?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("expected 32 bytes, got {}", bytes.len()))
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn parse_program_id(hex: &str) -> Result<[u32; 8]> {
    let bytes = Vec::from_hex(hex).context("invalid program_id hex")?;
    anyhow::ensure!(bytes.len() == 32, "program_id must be 64 hex chars (32 bytes)");
    let mut pid = [0u32; 8];
    for (i, chunk) in bytes.chunks(4).enumerate() {
        pid[i] = u32::from_le_bytes(chunk.try_into().unwrap());
    }
    Ok(pid)
}

// ---------------------------------------------------------------------------
// risc0 serde instruction encoding
//
// The SPEL #[lez_program] macro generates an Instruction enum serialized with
// serialize_tuple_variant(variant_index, ...). In risc0 serde format:
//   - u8 / u128: 1 / 4 u32 words.
//   - [u8; N]: N words (one per byte).
//   - Vec<u8>: 1 length word + N byte words.
//   - Enum variant: 1 word (variant index) + fields in order.
//
// Variant order follows declaration order in #[lez_program] mod:
//   0 = Initialize, 1 = Claim
// ---------------------------------------------------------------------------

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn encode_bytes32(b: &[u8; 32], out: &mut Vec<u32>) {
    for &byte in b {
        out.push(byte as u32);
    }
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn encode_u128(v: u128, out: &mut Vec<u32>) {
    // u128 = 4 u32 words (LE)
    for i in 0..4u128 {
        out.push(((v >> (i * 32)) & 0xffff_ffff) as u32);
    }
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn encode_vec_u8(bytes: &[u8], out: &mut Vec<u32>) {
    out.push(bytes.len() as u32);
    for &b in bytes {
        out.push(b as u32);
    }
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn instr_initialize(merkle_root: &[u8; 32], total_supply: u128) -> Vec<u32> {
    let mut w = vec![0u32]; // variant 0
    encode_bytes32(merkle_root, &mut w);
    encode_u128(total_supply, &mut w);
    w
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn instr_claim(journal_bytes: &[u8], recipient_note: &[u8]) -> Vec<u32> {
    let mut w = vec![1u32]; // variant 1
    encode_vec_u8(journal_bytes, &mut w);
    encode_vec_u8(recipient_note, &mut w);
    w
}

#[cfg(feature = "chain")]
mod chain {
    use super::*;
    use anyhow::Result;
    use common::transaction::NSSATransaction;
    use nssa::{
        AccountId, PrivateKey, PublicKey,
        public_transaction::{Message, WitnessSet},
        PublicTransaction,
    };
    use sequencer_service_rpc::{RpcClient as _, SequencerClientBuilder};

    pub fn account_id_for_key(key: &PrivateKey) -> AccountId {
        AccountId::from(&PublicKey::new_from_private_key(key))
    }

    pub fn keygen() -> (PrivateKey, AccountId) {
        let key = PrivateKey::new_os_random();
        let account_id = account_id_for_key(&key);
        (key, account_id)
    }

    /// Unsigned call: for instructions on an account already owned by the
    /// program (`#[account(mut)]`), no signature is needed.
    pub async fn send_call(
        sequencer: &str,
        program_id: [u32; 8],
        distributor_id: [u8; 32],
        instruction_data: Vec<u32>,
    ) -> Result<String> {
        let client = SequencerClientBuilder::default()
            .build(sequencer)
            .context("build sequencer client")?;

        let account_id: AccountId = AccountId::new(distributor_id);

        let message = Message::new_preserialized(
            program_id,
            vec![account_id],
            vec![], // no signers → no nonces
            instruction_data,
        );
        let witness_set = WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let hash = client
            .send_transaction(NSSATransaction::Public(tx))
            .await
            .context("send_transaction failed")?;

        Ok(hex::encode(hash))
    }

    /// Signed call: account claiming (`#[account(init)]`) requires the
    /// transaction to be authorized by the account's key.
    pub async fn send_signed_call(
        sequencer: &str,
        program_id: [u32; 8],
        key: &PrivateKey,
        instruction_data: Vec<u32>,
    ) -> Result<(String, AccountId)> {
        let client = SequencerClientBuilder::default()
            .build(sequencer)
            .context("build sequencer client")?;

        let account_id = account_id_for_key(key);
        let nonces = client
            .get_accounts_nonces(vec![account_id])
            .await
            .context("get_accounts_nonces failed")?;

        let message = Message::new_preserialized(
            program_id,
            vec![account_id],
            nonces,
            instruction_data,
        );
        let witness_set = WitnessSet::for_message(&message, &[key]);
        let tx = PublicTransaction::new(message, witness_set);

        let hash = client
            .send_transaction(NSSATransaction::Public(tx))
            .await
            .context("send_transaction failed")?;

        Ok((hex::encode(hash), account_id))
    }

    /// Read raw account state.
    pub async fn get_account_state(
        sequencer: &str,
        account_id_bytes: [u8; 32],
    ) -> Result<(Vec<u8>, [u32; 8])> {
        let client = SequencerClientBuilder::default()
            .build(sequencer)
            .context("build sequencer client")?;
        let account = client
            .get_account(AccountId::new(account_id_bytes))
            .await
            .context("get_account failed")?;
        Ok((account.data.as_ref().to_vec(), account.program_owner))
    }
}

#[cfg(not(feature = "chain"))]
fn chain_not_available() -> ! {
    eprintln!("Chain commands require --features chain.");
    eprintln!("Rebuild: cargo build --release --features chain");
    eprintln!("(requires lez-build workspace at ../../../lez-build/)");
    std::process::exit(1);
}

#[tokio::main]
async fn main() -> Result<()> {
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

        Cmd::Chain(chain_cmd) => {
            #[cfg(not(feature = "chain"))]
            chain_not_available();

            #[cfg(feature = "chain")]
            match chain_cmd {
                ChainCmd::Keygen => {
                    let (key, account_id) = chain::keygen();
                    println!("signing_key: {key}");
                    println!("distributor_id: {}", hex::encode(account_id.value()));
                    println!("account_id (base58): {account_id}");
                }
                ChainCmd::State { sequencer, distributor_id } => {
                    let did = parse_hex32(&distributor_id)?;
                    let (data, program_owner) = chain::get_account_state(&sequencer, did).await?;
                    let owner_hex = program_owner
                        .iter()
                        .flat_map(|w| w.to_le_bytes())
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    println!("program_owner: {owner_hex}");
                    println!("data ({} bytes): {}", data.len(), hex::encode(&data));
                }
                ChainCmd::Initialize { sequencer, program_id, signing_key, merkle_root, total_supply } => {
                    let pid = parse_program_id(&program_id)?;
                    let key: nssa::PrivateKey = signing_key.parse()
                        .map_err(|e| anyhow::anyhow!("invalid signing key: {e:?}"))?;
                    let root = parse_hex32(&merkle_root)?;
                    let instr = instr_initialize(&root, total_supply);
                    let (hash, account_id) =
                        chain::send_signed_call(&sequencer, pid, &key, instr).await?;
                    println!("tx: {hash}");
                    println!("distributor_id: {}", hex::encode(account_id.value()));
                }
                ChainCmd::Claim { sequencer, program_id, distributor_id, receipt, recipient_note } => {
                    let pid = parse_program_id(&program_id)?;
                    let did = parse_hex32(&distributor_id)?;
                    let note_bytes = Vec::from_hex(&recipient_note).context("invalid recipient_note hex")?;

                    let raw = std::fs::read(&receipt).context("read receipt file")?;
                    let receipt_words: Vec<u32> = raw.chunks_exact(4)
                        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                        .collect();
                    let r: risc0_zkvm::Receipt = risc0_zkvm::serde::from_slice(&receipt_words)
                        .map_err(|e| anyhow::anyhow!("deserialise receipt: {e}"))?;

                    #[derive(serde::Deserialize, borsh::BorshSerialize)]
                    struct Journal {
                        merkle_root: [u8; 32],
                        nullifier: [u8; 32],
                        distributor_id: [u8; 32],
                        allocation: u128,
                        recipient_note_hash: [u8; 32],
                    }
                    let j: Journal = r.journal.decode()?;
                    let journal_bytes = borsh::to_vec(&j)?;
                    let instr = instr_claim(&journal_bytes, &note_bytes);
                    let hash = chain::send_call(&sequencer, pid, did, instr).await?;
                    println!("tx: {hash}");
                }
            }
        }
    }

    Ok(())
}
