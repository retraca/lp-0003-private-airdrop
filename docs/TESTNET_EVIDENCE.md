# LP-0003 testnet deployment evidence

Date: 2026-06-12. Sequencer: `https://testnet.lez.logos.co` (hosted LEZ testnet). Explorer: `https://explorer.testnet.lez.logos.co`.

All transactions are verifiable with:

```bash
curl -s -X POST https://testnet.lez.logos.co -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"getTransaction","params":["<tx_hash>"],"id":1}'
```

and distributor state with `airdrop-claim chain state --sequencer https://testnet.lez.logos.co --distributor-id <hex>`.

## 1. Program deployments

| Program | Program ID | Deployment tx |
|---|---|---|
| `programs/airdrop/private_airdrop.bin` | `641e17aa9ac2c393a01d4cdf3d12621c1816466b685e0b6993a760c16f5d2e8f` | `b0f49e8c6e85bdbc72a34a76684e3e03a78c7cdccef61de1be432f9d10ba60a8` |
| `programs/claim_circuit/claim_circuit.bin` | `2919d161b729ec935b5ef5cc40b319fda02ad6d81df81f0245f5308b86b7fcd8` | `88553fd0e82e8e5387f398b99b85d5c90269b47f1fcfb580d71e718be56bcd79` |

## 2. Two distributions, 20 unique claims, REAL proofs

Run end to end at `RISC0_DEV_MODE=0` with `scripts/testnet_claims_run.sh`:
two 16-leaf eligibility trees with 10 claimants each (deterministic demo
account IDs, allocations 100..1000), every claim a privacy-preserving
transaction proven locally and confirmed on-chain before the next claim
started. Each distribution's final state shows `claimed = 5500` (the sum of
all 10 allocations) and 10 spent nullifiers.

### Distribution alpha

| Field | Value |
|---|---|
| Distributor account | `4bfe13dfbc63851ed8d0a9f1f055153fab2f1e1f64dc8c7902e39e5baacece4e` |
| Merkle root (16-leaf tree, 10 claimants) | `5eabd616bb2881fda0fc2fc7630bce0c0eb273218dc0fde4437056d4bbe65226` |
| Initialize tx | `6f11a8461f19f4d1d5cdd2f90cd2a4a5c2fbd977c2541f06dbea83bae45cb20d` |

| # | Allocation | Nullifier-bearing claim tx |
|---|---|---|
| 0 | 100 | `dd559c7576b398de33d0f46642b872d54b6b9191a240a87935f4bdb08cd59e8c` |
| 1 | 200 | `91ef16b46524dbae8b8f91013edee8daba4287e61f47931454f1f524621c08dc` |
| 2 | 300 | `e40c026746f4161d6473bc25683f48862a5164f154b2f5da8adba8d6db79d4e3` |
| 3 | 400 | `781467a6a973244141121f2612792fce99b91c0315216b6cd63f8e01a7d59567` |
| 4 | 500 | `cd117143cd23a183965371f65ca1db97df2af23ae70914b81814b243818f1147` |
| 5 | 600 | `fc689655d5539b8122178a936802a07c37daef1dd3c486fa972d91a072198647` |
| 6 | 700 | `9a9e9dadaf6decd3d07bf76824af83f1728531e67d90ee952269ecb78185e2af` |
| 7 | 800 | `0df3c7e58a4635be6ec4c6617ad796a3ba1b494be504264b4ce5cad92e3107f3` |
| 8 | 900 | `174ef4632750fba79ad4a18bbcd6850e5d669f080da8bf5482e153ef27823fb6` |
| 9 | 1000 | `ae60ff3642a7731f8ab9886f0a84ae16005ea59fc28b3e53dd136d374f1e79c0` |

Final state:
```
program_owner: 641e17aa9ac2c393a01d4cdf3d12621c1816466b685e0b6993a760c16f5d2e8f
data (420 bytes): 5eabd616bb2881fda0fc2fc7630bce0c0eb273218dc0fde4437056d4bbe652262919d161b729ec935b5ef5cc40b319fda02ad6d81df81f0245f5308b86b7fcd8a08601000000000000000000000000007c1500000000000000000000000000000a000000de16a1f8c17f29ff23c82ed00a09413393c61a6a4073eeefec13372e06e4387379ba406230ff77abe51d7056af528c061c78b06d74d6001eb892b2536ea0f53b36f6fc9eb17b74431d5df67393349d0fb42cc75f62a630f1f39636ccbfd47fdb2b0760a303c9fe6e76973f2751ca2cdf6b75c629dd02fb2a9f8c5eb1fb584eff6641d0a29ed8d18a4be818e7a849787b3b2717aa6cdb135c137e41f79dfe1c364452b73db6154fb3ff8353e403bf1671606bb0c40acabfa53915a972168071b986781202a4ca41fb9cfd649a20ffd67b2a53adbeef3d2d0d00ad5e08d60979a15be745801e581dedd9912622ad4f5f229b958507444c7421295c5527d22193d521e3fe9fea7dfec36917c367e592f0f2249f965ce1e0f21e5cd31ce6d927f981c8f202c3f8e0fa401881e8ea6f8d066a6e648503b2fa331e11cd2472ab3c1a01
```

### Distribution beta

| Field | Value |
|---|---|
| Distributor account | `65fdea015b68981f48b352a0c1ae2746162417fc7f10bd96aeed56131be4d857` |
| Merkle root (16-leaf tree, 10 claimants) | `1b1f3e7cc8e6b3caad645a1daadf8225cd10bb6370068a530a4dfac4b1a5445e` |
| Initialize tx | `f4c63ab733a049b42b132056d274ddcd3487d01e1dd6b8dc0884e640bd8f76ab` |

| # | Allocation | Nullifier-bearing claim tx |
|---|---|---|
| 0 | 100 | `dddef2c1db796ec52e5fd72c0d0fb2c1acb80be555fb1bd7f72ada36f05c8bb5` |
| 1 | 200 | `605037be17788b5d14c2f1570786d4392553d357557385b3fa3f8f47dc9dd4cc` |
| 2 | 300 | `bdd7b9b3c2fcfe349cdd2aa87ffd53839eb4bde122355c825e6fd2587a693c50` |
| 3 | 400 | `06caf27d99390a175d5e35ede51315361832ef9f865038f5ea416cbdc022696f` |
| 4 | 500 | `a1a36aa3e61eab0e7e6069d205da1f997864151fd50f853f680d8ef675f668f4` |
| 5 | 600 | `49e91378de7f83c5dbb59a84c84b72b562fc7457d46b0dd41973eeb75b9a6bac` |
| 6 | 700 | `1afa701ff85dabb0745484a9d1b7cf35877cc456620663fea3a00bacfdc5b5c4` |
| 7 | 800 | `8bd6465887d1765d8f280f51c28e74379cd18e45efc1a94ffbff4d99f460d8be` |
| 8 | 900 | `bd90ea9fcac08e8768139d0c59c98b1ab15078faa031aa79845d1f8fd2ccf884` |
| 9 | 1000 | `49daf93a3368dba8f4da8f0bd3d839b1efc8d5a1f08afcd225f650122c4b19ea` |

Final state:
```
program_owner: 641e17aa9ac2c393a01d4cdf3d12621c1816466b685e0b6993a760c16f5d2e8f
data (420 bytes): 1b1f3e7cc8e6b3caad645a1daadf8225cd10bb6370068a530a4dfac4b1a5445e2919d161b729ec935b5ef5cc40b319fda02ad6d81df81f0245f5308b86b7fcd8a08601000000000000000000000000007c1500000000000000000000000000000a00000007f273b58d26ed5bd969cc8dbeb0e1bdd783da2ac7da8f309b57d8fd8ab96b35ea4e109e4715f6ec4552f745234a826784b97c88dc892c642a01bf5ef2e3625469096334170d48fb90dafa65336fc6b750a57a90ab08d3d959b45568de09520ecfd1940ed7a5d7057a13b960487e69de2f90e475fb298cdd5a5403f1900598d4a5889c950e44b297883d5873215fd3b0fb14db15a9fbbe1af3046b966ee1ba420571ae0f07a306197beff0f91a2c923ba4d5dbc9f48b75c3669075f31a2829c186e13246f7421ef054d65e816d66171e7d5ce445bbbbd56f4ad797b4c4db2999cb0e97032b60afcacdcecc3c512e2b7256b5cbb24552a804a5885d59ddcfd1d9772f7de4b1317a04726db6c50f965d058bdf8ec7d46d3f5673cbcb16d5bd86ae07536d8e85b7509519afafa475c3c26cd2a1e43c855f72c7d61300e2323433d3
```


## 3. How claims work on-chain (chained-call composition)

LEZ public transactions carry no RISC0 receipts, so a program cannot resolve
an `env::verify` assumption in public execution. Claims travel through the
privacy-preserving execution (PPE) pipeline:

1. The claimant runs `airdrop-claim chain claim`. The CLI executes and proves
   the **claim-circuit program** locally: the account_id, allocation, Merkle
   path, and recipient-note preimage are initial-call instruction data, which
   never appears on-chain.
2. The claim-circuit program verifies Merkle inclusion (domain-tagged
   leaf/node hashing) against the root committed in the distribution account,
   derives the nullifier `SHA256(account_id || distributor_id)` and the
   recipient-note hash, and declares a `ChainedCall` into the airdrop
   program's `claim` instruction.
3. The PPE outer circuit proves both program executions and their linkage.
   The airdrop program accepts claims only from the claim-circuit program
   registered at `initialize` (`7007 ERR_UNAUTHORIZED_CALLER` otherwise).
4. The sequencer verifies ONE composite succinct proof and applies the public
   state diff. Each claim also creates a fresh zero-balance private "claimer
   note", giving it the same on-chain shape as any private transfer.

Negative path verified against a local standalone sequencer: a double claim
fails client-side during proving with `Program error 7004: nullifier already
spent`, before any transaction is sent.

## 4. Authorization model

`#[account(init)]` account claiming requires the transaction to be authorized
by the account's key (`chain keygen` + signed `chain initialize`). The key is
a one-time bootstrap credential; after claiming, the account is program-owned
and claims are submitted without any wallet signature.

## 5. Performance

| Operation | Cost |
|---|---|
| `initialize` (public tx) | ~4-10 ms zkVM executor time on the sequencer |
| `chain claim` client-side proving (`RISC0_DEV_MODE=0`, Apple M2) | ~7-10 minutes (claim-circuit proof + airdrop proof + PPE outer succinct proof) |
| Claim verification on the sequencer | one succinct receipt verification (same as any privacy-preserving transaction) |

Observed sequential throughput during the evidence run: 20 claims in ~3.2 hours
on a single laptop, including per-claim on-chain confirmation waits.

## 6. Superseded v1 evidence

An earlier program version (`d7f401fd…2e85`, deploy tx `5e1b1952…c5df`) used
`env::verify` for claims and could not accept them via public transactions.
Its two initialized distributions (`213a015c…9878`, `ad4009d1…1215`) remain
on the testnet as historical deployment evidence. The v2 architecture above
supersedes it. An aborted first v2 evidence run (distributor
`6871b0e8…23a9`, 2 landed claims before the runner was fixed to wait for
per-claim inclusion) also remains on-chain.
