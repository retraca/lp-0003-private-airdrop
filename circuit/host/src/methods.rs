// Pre-built guest binary (R0BF packaged, risc0-zkvm 3.0.5).
pub const PRIVATE_AIRDROP_GUEST_ELF: &[u8] =
    include_bytes!("../../guest/private-airdrop-guest.bin");
pub const PRIVATE_AIRDROP_GUEST_ID: [u32; 8] = [
    0x6bb86bdd, 0xe8c9a2a7, 0xa4dc70bb, 0x6986f85e,
    0x5b5edf0d, 0x9f63e369, 0xdb97fa75, 0xf92e11b4,
];
