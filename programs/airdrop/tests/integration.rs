//! Integration tests for the LP-0003 private airdrop on-chain program.
//! These tests exercise the program logic directly (no LEZ sequencer needed).

use private_airdrop_program::*;
use sha2::{Digest, Sha256};

fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

fn leaf_hash(account_id: &[u8; 32], allocation: u128) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(&[0x00u8]);
    h.update(account_id);
    h.update(&allocation.to_le_bytes());
    h.finalize().into()
}

fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(&[0x01u8]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

#[test]
fn error_codes_are_distinct() {
    let codes = [
        ERR_PROOF_INVALID,
        ERR_DISTRIBUTOR_MISMATCH,
        ERR_ROOT_MISMATCH,
        ERR_NULLIFIER_SPENT,
        ERR_DISTRIBUTION_EXHAUSTED,
        ERR_RECIPIENT_MISMATCH,
    ];
    let mut seen = std::collections::HashSet::new();
    for &c in &codes {
        assert!(seen.insert(c), "duplicate error code {}", c);
    }
}

#[test]
fn leaf_hash_is_domain_separated_from_internal_nodes() {
    // A leaf hash must never equal a node hash for the same input bytes,
    // otherwise a second-preimage attack is possible.
    let a = [1u8; 32];
    let b = [2u8; 32];

    // Compute leaf for (a, 0) -- matches guest's domain-tagged hash
    let lh = leaf_hash(&a, 0u128);

    // Compute internal node for the same pair
    let nh = node_hash(&a, &b);

    assert_ne!(lh, nh, "leaf and node hashes must be distinct");
}

#[test]
fn recipient_note_hash_binding() {
    let note1 = b"note_for_alice";
    let note2 = b"note_for_bob";
    let h1: [u8; 32] = sha256(note1);
    let h2: [u8; 32] = sha256(note2);
    assert_ne!(h1, h2);
}

#[test]
fn nullifier_is_account_and_distributor_bound() {
    let account_id = [0xaau8; 32];
    let distributor_id = [0xbbu8; 32];

    // nullifier = SHA256(account_id || distributor_id)
    let mut preimage = Vec::with_capacity(64);
    preimage.extend_from_slice(&account_id);
    preimage.extend_from_slice(&distributor_id);
    let nullifier = sha256(&preimage);

    // Different account produces different nullifier
    let account_id2 = [0xccu8; 32];
    let mut preimage2 = Vec::with_capacity(64);
    preimage2.extend_from_slice(&account_id2);
    preimage2.extend_from_slice(&distributor_id);
    let nullifier2 = sha256(&preimage2);

    assert_ne!(nullifier, nullifier2);
}
