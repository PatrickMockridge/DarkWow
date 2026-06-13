# Wallet Architecture

## Design Philosophy

The DarkWow wallet follows the same architecture as Bitcoin Core
(bitcoind + bitcoin-cli): **process separation between daemon and wallet**.

- `dwowd` is the full-node daemon. It syncs the chain via P2P, stores blocks
  locally in sled, validates consensus, and exposes a JSON-RPC interface.
- `dwow_wallet` is a command-line wallet. It connects to `dwowd` via localhost
  RPC for block data and transaction broadcast. Everything else — key
  management, scanning, balance queries, transaction building — is pure
  local computation over SQLite and sled.

The wallet never syncs the chain itself. It reads blocks from the already-synced
chain that `dwowd` maintains on the same machine. This is **not** SPV or light
client — the full chain is local. It is process separation: the daemon handles
chain sync and consensus, the wallet handles application logic.

## Two Things the Wallet Tracks

1. **Native Token** — the consensus asset. Used for fee payments and coinbase
   rewards. The only asset that requires Merkle proof tracking for fee spending.
   Stored in the `coins` table.

2. **Capabilities** — everything else. Every contract output that the wallet
   discovers via AEAD decryption. PN, BB, escrow, auction, all 25+ contracts.
   Stored in the `capabilities` table.

There is no third category. No contract gets its own table, its own tree,
or its own dedicated methods. Native Token is the sole special citizen —
and only because it is the consensus asset needed for fee payment.

## Merkle Proofs and Nullifiers Are Universal

Every DarkWow transaction — transfer, burn, mint, redeem, swap, deploy —
requires Merkle proofs and nullifiers. This is the protocol, not a native-token
special case. BurnV1 proves coin inclusion via Merkle root and publishes a
nullifier. TransferV1 does both: burns with Merkle proof + nullifier, creates
new blind outputs. The wallet caches Merkle trees for scan efficiency, but
Merkle proofs are not a "native token feature" — they are how every
transaction authorizes itself.

## Capability-First Scanning

The wallet discovers everything — native token and capabilities alike — by
attempting AEAD decryption on every output in every transaction. The AEAD
authentication tag IS the discriminator. Successful decryption proves the
output belongs to this wallet, regardless of which contract produced it.

All contracts use the same AEAD encryption primitive (ChaCha20Poly1305 +
Sapling DH). The wallet's byte-level scanner handles any contract that
produces `AeadEncryptedNote` outputs — **new contracts work without
wallet code changes**.

### Scan Architecture

```
for each transaction in block:
    # Path 1: Native Token coinbase (consensus reward)
    if coinbase present:
        try AEAD decrypt with wallet secrets
        if success → store in coins table with Merkle proof

    # Path 2: Generic AEAD (everything else — PN, BB, all 25+ contracts)
    for each contract call:
        scan call.data byte-by-byte for AeadEncryptedNote patterns
        for each note found:
            for each wallet secret:
                if AEAD decrypt succeeds:
                    store in capabilities table
```

Path 2 is a single generic function. No contract ID lookup. No per-contract
branch. No "optimized handler" dispatch. The byte-level AEAD scan handles
every contract uniformly.

## Async is for Chain Sync Only

The only async operations in the wallet are RPC calls to `dwowd`:
- `scan_blocks()` — fetches blocks in a loop
- `broadcast_tx()` — submits transactions to the mempool
- `DwowdRpcClient` methods — all RPC

Everything else is synchronous: keygen, balance, transfer, scanning logic,
SQLite/sled reads and writes.

## Transaction Building

The wallet builds transactions for specific contract functions. These are
legitimate contract-specific operations — they call named opcodes:

| Method | Contract | Opcode | What It Does |
|--------|----------|--------|--------------|
| `transfer()` | PN | TransferV1 (0x04) | Atomic burn + blind output |
| `redeem()` | PN | RedeemV1 (0x01) | Close lifecycle, create receipt |
| `burn()` | PN | BurnV1 (0x03) | Destroy value, publish nullifier |
| `create_token()` | PN | TokenMintV1 (0x00) | Create token type |
| `mint_tokens()` | PN | MintV1 (0x02) | Mint coins with backing proof |
| `init_swap()` / `join_swap()` | PN | OtcSwapV1 (0x05) | Atomic peer-to-peer exchange |

Transaction builders follow a common pattern:
1. Look up inputs from wallet DB
2. Fetch secrets, Merkle proof, leaf position, blinds
3. Load ZK binary, generate proofs
4. Encode params, wrap in ContractCall
5. Attach Native Token fee
6. Return signed Transaction

## Data Stores

| Store | Type | Contents |
|-------|------|----------|
| `cache` | sled | Block pointers, Merkle tree checkpoints, nullifier SMT, scanned block tracker |
| `wallet` | SQLite | Native token coins, capabilities, secrets, addresses, transaction history, contract registry |

## Database Schema

| Table | Purpose |
|-------|---------|
| `addresses` | User keypairs (public + secret) |
| `coins` | Native token outputs with Merkle proofs (fee spending) |
| `capabilities` | **Universal capability store** — every AEAD-decrypted output from every contract |
| `secrets` | Trial decryption secrets (all contracts) |
| `transactions_history` | Sent transactions with status |
| `contract_registry` | Known contract name → contract_id mappings |
| `contract_metadata` | On-chain metadata (name, symbol, deployer) |
| `contract_interactions` | Record of contract function calls |
| `scanned_blocks` | Last scanned block height |
| `deploy_authorities` | Deploy keys for user-deployed contracts |

## CLI

```
dwow_wallet keygen                        Generate new keypair
dwow_wallet balance                       Show balances
dwow_wallet address                       Show default address
dwow_wallet addresses                     List all addresses
dwow_wallet transfer <amt> <token> <rcpt> Send funds
dwow_wallet scan                          Scan blockchain
dwow_wallet broadcast                     Broadcast a transaction
dwow_wallet contract deploy <wasm>        Deploy a contract
dwow_wallet contract invoke <id> <fn>     Call a contract function
```
