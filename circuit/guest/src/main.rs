//! RISC0 guest: private airdrop claim proof for LP-0003.
//!
//! Proves: "My account_id and allocation are committed in the distribution Merkle tree,
//! and I have not claimed before (nullifier is fresh)."
//!
//! Private inputs:
//!   account_id_bytes: [u8; 32]       -- claimant's account ID (never revealed)
//!   allocation: u128                 -- token allocation for this claimant
//!   merkle_path: Vec<[u8; 32]>       -- sibling nodes from Merkle inclusion proof
//!   leaf_index: usize                -- index of this leaf in the tree
//!   recipient_note_preimage: Vec<u8> -- opaque bytes; only its hash is revealed
//!
//! Public outputs (journal):
//!   merkle_root: [u8; 32]            -- distribution Merkle root
//!   nullifier: [u8; 32]              -- SHA256(account_id || distributor_id)
//!   distributor_id: [u8; 32]         -- which distribution this claim is for
//!   allocation: u128                 -- token amount to transfer
//!   recipient_note_hash: [u8; 32]    -- SHA256(recipient_note_preimage); binds proof to one recipient

#![no_std]
#![no_main]

use risc0_zkvm::guest::env;
use risc0_zkvm::sha::{Impl as ShaImpl, Sha256 as _};

risc0_zkvm::guest::entry!(main);

#[derive(serde::Deserialize)]
struct GuestInput {
    account_id_bytes: [u8; 32],
    allocation: u128,
    merkle_path: alloc::vec::Vec<[u8; 32]>,
    leaf_index: usize,
    merkle_root: [u8; 32],
    distributor_id: [u8; 32],
    // The destination note; hash is committed in the journal so no third party
    // can redirect the claim by replaying this receipt with a different note.
    recipient_note_preimage: alloc::vec::Vec<u8>,
}

#[derive(serde::Serialize)]
struct ClaimAttestation {
    merkle_root: [u8; 32],
    nullifier: [u8; 32],
    distributor_id: [u8; 32],
    allocation: u128,
    recipient_note_hash: [u8; 32],
}

pub fn main() {
    let input: GuestInput = env::read();

    // 1. Compute leaf with domain tag 0x00 to prevent second-preimage attacks.
    //    leaf = SHA256(0x00 || account_id || allocation_le)
    let leaf_hash: [u8; 32] = {
        let mut preimage = alloc::vec::Vec::with_capacity(1 + 32 + 16);
        preimage.push(0x00u8);
        preimage.extend_from_slice(&input.account_id_bytes);
        preimage.extend_from_slice(&input.allocation.to_le_bytes());
        ShaImpl::hash_bytes(&preimage).as_bytes().try_into().unwrap()
    };

    // 2. Verify Merkle inclusion.
    //    Internal node hashes use domain tag 0x01:
    //      node = SHA256(0x01 || left || right)
    //    This prevents a Merkle second-preimage attack where an internal node
    //    value could be used as a valid leaf.
    let computed_root = {
        let mut result = leaf_hash;
        let mut level_index = input.leaf_index;
        for node in &input.merkle_path {
            let mut preimage = alloc::vec::Vec::with_capacity(1 + 64);
            preimage.push(0x01u8);
            if level_index & 1 == 0 {
                preimage.extend_from_slice(&result);
                preimage.extend_from_slice(node);
            } else {
                preimage.extend_from_slice(node);
                preimage.extend_from_slice(&result);
            }
            result = ShaImpl::hash_bytes(&preimage).as_bytes().try_into().unwrap();
            level_index >>= 1;
        }
        result
    };
    assert_eq!(computed_root, input.merkle_root, "Merkle root mismatch");

    // 3. Compute nullifier = SHA256(account_id || distributor_id).
    // Prevents double-claiming without revealing which account is claiming.
    let nullifier = {
        let mut preimage = alloc::vec::Vec::with_capacity(64);
        preimage.extend_from_slice(&input.account_id_bytes);
        preimage.extend_from_slice(&input.distributor_id);
        ShaImpl::hash_bytes(&preimage).as_bytes().try_into().unwrap()
    };

    // 4. Hash the recipient note. Publishing the hash in the journal binds this
    //    proof to exactly one destination; any attempt to relay the receipt with
    //    a different note will fail the on-chain hash check.
    let recipient_note_hash: [u8; 32] = ShaImpl::hash_bytes(&input.recipient_note_preimage)
        .as_bytes().try_into().unwrap();

    // 5. Write journal.
    env::commit(&ClaimAttestation {
        merkle_root: input.merkle_root,
        nullifier,
        distributor_id: input.distributor_id,
        allocation: input.allocation,
        recipient_note_hash,
    });
}

extern crate alloc;
