# LP-0003 testnet deployment evidence

Date: 2026-06-12. Sequencer: `https://testnet.lez.logos.co` (hosted LEZ testnet).

All transactions are verifiable with:

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"getTransaction","params":["<tx_hash>"],"id":1}'
```

and the distributor state with:

```bash
airdrop-claim chain state --sequencer https://testnet.lez.logos.co --distributor-id <hex>
```

## 1. Program deployment

| Field | Value |
|---|---|
| Program binary | `programs/airdrop/private_airdrop.bin` (R0BF) |
| Program ID | `d7f401fde733a4ac2b54f4fa909de9e2c86d2f2fd9e256498efea527ade52e85` |
| Deployment tx | `5e1b19525562dd9f02dde1096bbe5a17d6755f78e30e002f30dced5338c7c5df` |

## 2. Two distinct distributions

### Distribution 1

| Field | Value |
|---|---|
| Distributor account | `213a015cc6efb32a75fcee3f972781b8b35b795e5b9b00ddcf4ea5605be89878` |
| Initialize tx | `ac74f7e172c27d9b31c9361aa9aebec02e62b3173baf5327a825d6f0c39d68df` |
| Merkle root | `ec31d4e39bb156515854ea226229b56eb878758a1527d349430b6145db2f2175` |
| Total supply | 1,000,000 |

### Distribution 2

| Field | Value |
|---|---|
| Distributor account | `ad4009d154e6d836e62f6cf7061521093ea6238a9bbf29b17a1b6f7ac4351215` |
| Initialize tx | `83c4f97dc7a6aacae7e1185379acf2e436f890f1e7c27c4435693678497acd76` |
| Merkle root | `6d3aabd1f20386bafb0f32ca197201f47b36edc1ac72ee7ab8aad7788ec24223` |
| Total supply | 500,000 |

Read-back state for both accounts shows `program_owner = d7f401fd…2e85` (claimed by the airdrop program) and `DistributionState { merkle_root, total_supply, claimed: 0, spent_nullifiers: [] }`.

## Authorization model

`#[account(init)]` account claiming requires the transaction to be authorized by the account's key: an unsigned initialize fails with `InvalidProgramBehavior(ClaimedUnauthorizedAccount)`. The CLI flow:

1. `airdrop-claim chain keygen` generates a fresh schnorr (BIP340) key; the distributor ID is `SHA256("/LEE/v0.3/AccountId/Public/" || pubkey)`.
2. `airdrop-claim chain initialize --signing-key <hex> …` fetches the nonce, signs, and submits. The key is a one-time bootstrap credential; after claiming, the account belongs to the program.

## Known limitation: on-chain claim submission

The `claim` instruction verifies the claimant's RISC0 receipt inside the program. LEZ public transactions carry no receipts and the public execution path adds no assumptions to the executor (`nssa/src/program.rs::execute`), so submitting a claim today fails with `sys_verify_integrity: no receipt found to resolve assumption`.

The LEZ-native resolution is the privacy-preserving transaction path: the client proves the program call locally with the claim receipt as an assumption (`nssa/src/privacy_preserving_transaction/circuit.rs`) and submits one composite proof. Wiring claims through that path is the remaining work item for the 20-claims requirement; the commitment scheme, proof generation, offline verification, and distributor lifecycle are all demonstrated.

Cycle-budget note: in-guest Groth16 verification (no assumptions) does not fit the 32M-cycle public execution budget, ruling out the simpler alternative.
