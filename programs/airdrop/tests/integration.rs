//! Integration tests for the LP-0003 private airdrop on-chain program.
//! Tests run against `apply_claim` directly -- no LEZ sequencer or RISC0
//! receipt needed. The ZK proof is tested separately via `demo.sh`.

use private_airdrop_program::*;
use spel_framework::error::SpelError;
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

fn base_state() -> DistributionState {
    DistributionState {
        merkle_root: [0xaau8; 32],
        claim_circuit_program_id: [0x11u32; 8],
        total_supply: 10_000,
        claimed: 0,
        spent_nullifiers: vec![],
    }
}

fn base_journal(note: &[u8]) -> ClaimJournal {
    ClaimJournal {
        merkle_root: [0xaau8; 32],
        nullifier: [0x01u8; 32],
        distributor_id: [0xbbu8; 32],
        allocation: 100,
        recipient_note_hash: sha256(note),
    }
}

#[test]
fn successful_claim_updates_state() {
    let mut state = base_state();
    let note = b"alice_note";
    let journal = base_journal(note);
    let distributor = [0xbbu8; 32];

    apply_claim(&mut state, &journal, distributor, note).unwrap();

    assert_eq!(state.claimed, 100);
    assert!(state.spent_nullifiers.contains(&[0x01u8; 32]));
}

#[test]
fn nullifier_spent_rejected() {
    let mut state = base_state();
    let note = b"alice_note";
    let journal = base_journal(note);
    let distributor = [0xbbu8; 32];

    apply_claim(&mut state, &journal, distributor, note).unwrap();

    let err = apply_claim(&mut state, &journal, distributor, note).unwrap_err();
    let SpelError::Custom { code, .. } = err else { panic!("wrong error type") }; assert_eq!(code, ERR_NULLIFIER_SPENT, "wrong error code");
}

#[test]
fn distributor_mismatch_rejected() {
    let mut state = base_state();
    let note = b"alice_note";
    let journal = base_journal(note);
    let wrong_distributor = [0xccu8; 32];

    let err = apply_claim(&mut state, &journal, wrong_distributor, note).unwrap_err();
    let SpelError::Custom { code, .. } = err else { panic!("wrong error type") }; assert_eq!(code, ERR_DISTRIBUTOR_MISMATCH, "wrong error code");
}

#[test]
fn merkle_root_mismatch_rejected() {
    let mut state = base_state();
    let note = b"alice_note";
    let mut journal = base_journal(note);
    journal.merkle_root = [0x00u8; 32]; // wrong root
    let distributor = [0xbbu8; 32];

    let err = apply_claim(&mut state, &journal, distributor, note).unwrap_err();
    let SpelError::Custom { code, .. } = err else { panic!("wrong error type") }; assert_eq!(code, ERR_ROOT_MISMATCH, "wrong error code");
}

#[test]
fn recipient_mismatch_rejected_before_nullifier_consumed() {
    let mut state = base_state();
    let real_note = b"alice_note";
    let relay_note = b"attacker_note";
    let mut journal = base_journal(real_note);
    // Journal has alice's note hash; attacker submits relay_note instead
    let distributor = [0xbbu8; 32];

    let err = apply_claim(&mut state, &journal, distributor, relay_note).unwrap_err();
    let SpelError::Custom { code, .. } = err else { panic!("wrong error type") }; assert_eq!(code, ERR_RECIPIENT_MISMATCH, "wrong error code");

    // Nullifier must not be marked spent -- alice can still claim with correct note
    assert!(state.spent_nullifiers.is_empty());
    apply_claim(&mut state, &journal, distributor, real_note).unwrap();
}

#[test]
fn distribution_exhausted_rejected() {
    let mut state = base_state();
    state.total_supply = 50;
    let note = b"alice_note";
    let mut journal = base_journal(note);
    journal.allocation = 100; // exceeds supply
    let distributor = [0xbbu8; 32];

    let err = apply_claim(&mut state, &journal, distributor, note).unwrap_err();
    let SpelError::Custom { code, .. } = err else { panic!("wrong error type") }; assert_eq!(code, ERR_DISTRIBUTION_EXHAUSTED, "wrong error code");

    // State unchanged
    assert_eq!(state.claimed, 0);
    assert!(state.spent_nullifiers.is_empty());
}

#[test]
fn two_distinct_claimants_both_succeed() {
    let mut state = base_state();
    let distributor = [0xbbu8; 32];

    let note_a = b"note_for_alice";
    let mut journal_a = base_journal(note_a);
    journal_a.nullifier = [0x01u8; 32];
    journal_a.allocation = 300;

    let note_b = b"note_for_bob";
    let mut journal_b = base_journal(note_b);
    journal_b.nullifier = [0x02u8; 32];
    journal_b.allocation = 400;

    apply_claim(&mut state, &journal_a, distributor, note_a).unwrap();
    apply_claim(&mut state, &journal_b, distributor, note_b).unwrap();

    assert_eq!(state.claimed, 700);
    assert_eq!(state.spent_nullifiers.len(), 2);
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
fn leaf_hash_domain_separated_from_node_hash() {
    let a = [1u8; 32];
    let b = [2u8; 32];
    let lh = leaf_hash(&a, 0u128);
    let nh = node_hash(&a, &b);
    assert_ne!(lh, nh);
}
