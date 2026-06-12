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
        /// Claim-circuit program ID (64-char hex). Claims are accepted only
        /// as chained calls from this program.
        #[arg(long)]
        claim_circuit_program_id: String,
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
    /// Claim on-chain via a privacy-preserving transaction.
    ///
    /// Executes and proves the claim-circuit program locally (the account_id,
    /// allocation, Merkle path, and note preimage never leave this machine and
    /// never appear on-chain), composes the chained call into the airdrop
    /// program, and submits one privacy-preserving transaction.
    /// RISC0_DEV_MODE=0 means real proving: expect minutes.
    Claim {
        #[arg(long, default_value = "http://127.0.0.1:3040")]
        sequencer: String,
        /// Airdrop program ID (64-char hex).
        #[arg(long)]
        program_id: String,
        /// Path to the claim-circuit program binary (R0BF).
        #[arg(long, default_value = "programs/claim_circuit/claim_circuit.bin")]
        claim_circuit_bin: PathBuf,
        /// Path to the airdrop program binary (R0BF).
        #[arg(long, default_value = "programs/airdrop/private_airdrop.bin")]
        airdrop_bin: PathBuf,
        #[arg(long)]
        distributor_id: String,
        /// Claimant account ID (64-char hex). PRIVATE: used only for local proving.
        #[arg(long)]
        account_id: String,
        /// Allocation for this claimant (must match the Merkle leaf).
        #[arg(long)]
        allocation: u128,
        /// Leaf index in the eligibility tree.
        #[arg(long)]
        leaf_index: u32,
        /// Sibling nodes from leaf to root (comma-separated hex).
        #[arg(long)]
        merkle_path: String,
        /// Recipient note bytes (hex).
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
fn instr_initialize(
    merkle_root: &[u8; 32],
    claim_circuit_program_id: [u32; 8],
    total_supply: u128,
) -> Vec<u32> {
    let mut w = vec![0u32]; // variant 0
    encode_bytes32(merkle_root, &mut w);
    w.extend_from_slice(&claim_circuit_program_id); // [u32; 8] = 8 words
    encode_u128(total_supply, &mut w);
    w
}

/// Encode the claim-circuit program's `submit_claim` instruction (variant 0):
/// airdrop_program_id [u32;8], account_id [u8;32], allocation u128,
/// leaf_index u32, merkle_path Vec<[u8;32]>, recipient_note Vec<u8>.
#[cfg_attr(not(feature = "chain"), allow(dead_code))]
fn instr_cc_submit_claim(
    airdrop_program_id: [u32; 8],
    account_id: &[u8; 32],
    allocation: u128,
    leaf_index: u32,
    merkle_path: &[[u8; 32]],
    recipient_note: &[u8],
) -> Vec<u32> {
    let mut w = vec![0u32]; // variant 0 (single instruction)
    w.extend_from_slice(&airdrop_program_id);
    encode_bytes32(account_id, &mut w);
    encode_u128(allocation, &mut w);
    w.push(leaf_index);
    w.push(merkle_path.len() as u32);
    for node in merkle_path {
        encode_bytes32(node, &mut w);
    }
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

    /// Claim via a privacy-preserving transaction.
    ///
    /// Client-side: executes and proves the claim-circuit program (initial
    /// call, eligibility data as private input) plus the chained airdrop claim
    /// call, then wraps both in the PPE outer circuit proof. The sequencer
    /// verifies one composite proof; instruction data of the initial call (the
    /// claimant identity and Merkle path) never leaves this machine.
    pub async fn send_claim_ppe(
        sequencer: &str,
        distributor_id: [u8; 32],
        claim_circuit_bytecode: Vec<u8>,
        airdrop_bytecode: Vec<u8>,
        instruction_data: Vec<u32>,
    ) -> Result<String> {
        use key_protocol::key_management::{KeyChain, ephemeral_key_holder::EphemeralKeyHolder};
        use nssa::privacy_preserving_transaction::{
            Message as PpeMessage, WitnessSet as PpeWitnessSet,
            circuit::ProgramWithDependencies,
        };
        use nssa::program::Program;
        use nssa_core::account::{Account, AccountWithMetadata};
        use std::collections::HashMap;

        let client = SequencerClientBuilder::default()
            .build(sequencer)
            .context("build sequencer client")?;

        let account_id = AccountId::new(distributor_id);
        let account = client
            .get_account(account_id)
            .await
            .context("get_account failed")?;
        let pre_state = AccountWithMetadata::new(account, false, account_id);

        // Fresh zero-balance private "claimer note": gives the transaction its
        // required commitment/nullifier pair and makes the claim look like any
        // other private transaction. The keys are throwaway.
        let note_keys = KeyChain::new_os_random();
        let note_npk = note_keys.nullifier_public_key;
        let note_vpk = note_keys.viewing_public_key;
        let note_pre = AccountWithMetadata::new(Account::default(), false, &note_npk);
        let eph = EphemeralKeyHolder::new(&note_npk);
        let note_ssk = eph.calculate_shared_secret_sender(&note_vpk);
        let note_epk = eph.generate_ephemeral_public_key();

        let claim_circuit =
            Program::new(claim_circuit_bytecode).context("parse claim_circuit binary")?;
        let airdrop = Program::new(airdrop_bytecode).context("parse airdrop binary")?;
        let mut dependencies = HashMap::new();
        dependencies.insert(airdrop.id(), airdrop);
        let pwd = ProgramWithDependencies::new(claim_circuit, dependencies);

        eprintln!("Proving privacy-preserving execution (claim circuit + airdrop chained call)...");
        eprintln!("This runs the RISC0 prover locally; with RISC0_DEV_MODE=0 expect minutes.");
        let (output, proof) = nssa::execute_and_prove(
            vec![pre_state, note_pre],
            instruction_data,
            vec![0, 2],                 // public distribution account + fresh private note
            vec![(note_npk, note_ssk)], // note encryption keys
            vec![],                     // no nsks: the note is unauthenticated (new)
            vec![None],                 // membership proof slot for the new note
            &pwd,
        )
        .map_err(|e| anyhow::anyhow!("execute_and_prove failed: {e:?}"))?;

        let message = PpeMessage::try_from_circuit_output(
            vec![account_id],
            vec![], // no signer nonces: the distribution account is program-owned
            vec![(note_npk, note_vpk, note_epk)],
            output,
        )
        .map_err(|e| anyhow::anyhow!("message construction failed: {e:?}"))?;

        let witness_set = PpeWitnessSet::for_message(&message, proof, &[]);
        let tx = nssa::PrivacyPreservingTransaction::new(message, witness_set);

        let hash = client
            .send_transaction(NSSATransaction::PrivacyPreserving(tx))
            .await
            .context("send_transaction failed")?;

        Ok(hex::encode(hash))
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
                ChainCmd::Initialize { sequencer, program_id, claim_circuit_program_id, signing_key, merkle_root, total_supply } => {
                    let pid = parse_program_id(&program_id)?;
                    let cc_pid = parse_program_id(&claim_circuit_program_id)?;
                    let key: nssa::PrivateKey = signing_key.parse()
                        .map_err(|e| anyhow::anyhow!("invalid signing key: {e:?}"))?;
                    let root = parse_hex32(&merkle_root)?;
                    let instr = instr_initialize(&root, cc_pid, total_supply);
                    let (hash, account_id) =
                        chain::send_signed_call(&sequencer, pid, &key, instr).await?;
                    println!("tx: {hash}");
                    println!("distributor_id: {}", hex::encode(account_id.value()));
                }
                ChainCmd::Claim { sequencer, program_id, claim_circuit_bin, airdrop_bin, distributor_id, account_id, allocation, leaf_index, merkle_path, recipient_note } => {
                    let pid = parse_program_id(&program_id)?;
                    let did = parse_hex32(&distributor_id)?;
                    let acct = parse_hex32(&account_id)?;
                    let note_bytes = Vec::from_hex(&recipient_note).context("invalid recipient_note hex")?;

                    let path: Result<Vec<[u8; 32]>> = merkle_path
                        .split(',')
                        .map(|s| parse_hex32(s.trim()))
                        .collect();
                    let path = path?;

                    let cc_bytecode = std::fs::read(&claim_circuit_bin)
                        .with_context(|| format!("read {}", claim_circuit_bin.display()))?;
                    let ad_bytecode = std::fs::read(&airdrop_bin)
                        .with_context(|| format!("read {}", airdrop_bin.display()))?;

                    let instr = instr_cc_submit_claim(
                        pid, &acct, allocation, leaf_index, &path, &note_bytes,
                    );
                    let hash = chain::send_claim_ppe(
                        &sequencer, did, cc_bytecode, ad_bytecode, instr,
                    ).await?;
                    println!("tx: {hash}");
                }
            }
        }
    }

    Ok(())
}
