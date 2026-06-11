# Private Airdrop Claim — Basecamp App

A Logos Basecamp mini-app for the LP-0003 private airdrop distributor.

## Load in Logos app (Basecamp)

1. In the Logos desktop app, open the Basecamp module.
2. Click **Load local app** and point it at this directory.
3. The app loads `index.html` directly — no build step.

## What it does

**Compute leaf hash** — enter your account ID and allocation to get `SHA256(0x00 || account_id || allocation_le)`. Give this to the distributor to include in the Merkle tree. Your account ID is type `password` and never leaves the page.

**Generate proof** — the page shows the CLI command to generate a claim proof offline once the distributor publishes the Merkle root.

**Submit claim** — submit the receipt to the on-chain `claim` instruction. The program verifies the RISC0 proof, checks the nullifier is unspent, and checks the recipient note hash.

## Security note

Leaf hash computation runs client-side using the Web Crypto API. The account ID input is type `password` and is never stored or transmitted.
