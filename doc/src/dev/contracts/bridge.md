# Bridge Contract

Anonymous bridge contract for cross-chain asset transfers using **Object Capability Security** instead of VSS.

## Overview

The bridge contract enables privacy-preserving transfers between DarkWow and external blockchains (initially Ethereum).

**Key Innovation**: DarkWow replaces traditional VSS (Verifiable Secret Sharing) with **deterministic address derivation** - users hold their own secrets, bridge nodes cannot steal funds.

## The VSS Problem

Traditional bridges use VSS for custody:
```
User deposits → VSS nodes hold secret shards → Withdrawal requires n-of-m threshold
```

**Vulnerabilities:**
- VSS Node Compromise: Any t of n nodes can reconstruct secret and steal funds
- Centralization: Threshold nodes can censor withdrawals
- Slow: Threshold signing required for each withdrawal

## The Object Capability Solution

DarkWow bridge uses deterministic address derivation:
```
User knows secret → Derive bridge_address = H(recipient_identity, nonce) → Deposit
User knows secret → Compute nullifier = H(secret) → Withdraw (self-signed)
```

**Advantages:**
- No shared secrets: Bridge nodes cannot know user's bridge secret
- Fast withdrawals: No threshold coordination, just ZK proof verification
- Censorship resistant: User alone authorizes, no gatekeepers

## Security Comparison

| Aspect | VSS-Based Bridge | DarkWow OCap Bridge |
|--------|------------------|--------------------|
| Key custody | Distributed shards | User-held secrets |
| Withdrawal speed | Slow (round) | Fast (self-signed) |
| Node compromise | Catastrophic | Impossible |
| Censorship | Threshold can block | Cannot block |
| Complexity | High (DKG) | Low (hashing) |

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize bridge state |
| DepositV1 | 0x01 | Register external chain deposit |
| WithdrawV1 | 0x02 | Claim withdrawal to external chain |
| UpdateConfigV1 | 0x03 | Update bridge operators/threshold |
| CancelWithdrawV1 | 0x04 | Cancel timed-out withdrawal |
| ExecuteGuaranteedWithdrawV1 | 0x05 | Execute guaranteed withdrawal with pool stake |
| CreateHtlcV1 | 0x06 | Create HTLC for cross-chain swap |
| ClaimHtlcV1 | 0x07 | Claim HTLC with preimage |
| RefundHtlcV1 | 0x08 | Refund expired HTLC |
| ReassignWithdrawalV1 | 0x09 | Reassign stuck withdrawal to new relayer |
| RegisterRelayerV1 | 0x0A | Register relayer pubkey |
| AcceptWithdrawalV1 | 0x0B | Accept pending withdrawal as relayer |
| VerifyRelayerReputationV1 | 0x0C | Query relayer reputation on-chain |
| RegisterFeeScheduleV1 | 0x0D | Register fee schedule commitment |
| GovernanceReportV1 | 0x0E | Per-chain accounting report |

## ZK Circuits

- `deposit_v1.zk`: Prove Ethereum deposit is valid without revealing recipient
- `withdraw_v1.zk`: Prove withdrawal authorization without revealing secret
- `xmr_deposit_v1.zk`: Prove Monero deposit via DLEq proof (stubbed DLEq verification)
- `zec_deposit_v1.zk`: Prove Zcash Sapling deposit via nullifier + merkle proof
- `azt_deposit_v1.zk`: Prove Aztec rollup deposit via note + merkle proof
- `ltc_deposit_v1.zk`: Prove Litecoin deposit via merkle proof (MWEB optional)

## External Chain Support

### Ethereum

Standard Merkle proof verification for ETH deposits:
- Merkle proof authenticates deposit inclusion in ETH block
- ZK circuit verifies: commitment = H(secret, amount, bridge_address)
- 12 block confirmations required

### Monero

Monero uses Cryptonote protocol, fundamentally different from Ethereum's UTXO model:

| Aspect | Ethereum | Monero |
|--------|----------|--------|
| Address type | Regular public keys | One-time addresses |
| Ownership proof | Signatures | DLEq proofs |
| Hash function | Keccak256 | cn_fast_hash (Keccak256) |
| Privacy | Transparent | Ring signatures |

**XMR Deposit Flow:**
```
1. User computes one-time address: derive_from(bridge_pub, view_key)
2. User sends XMR to this address on Monero chain
3. Relayer observes deposit via Monero RPC (view key only)
4. Relayer constructs DLEq proof showing ownership
5. User submits DepositV1 with XmrDepositProof
6. Contract verifies DLEq + merkle proof + confirmations
7. Contract mints wXMR to user
```

**Trust Model:**
- Relayer: Honest-but-curious (view key only, cannot spend)
- Economic incentives + overcollateralization prevent fraud
- DLEq verification stubbed in MVP (would require Montgomery curve support)

**XMR Constants:**
- Minimum deposit: 0.001 XMR (1,000,000,000 piconero)
- Confirmations required: 10 blocks

**XMR Withdrawal Flow:**
```
1. User burns wXMR on DarkWow
2. User specifies recipient hash (Monero address)
3. Relayer picks up pending withdrawal
4. Relayer broadcasts TX to Monero network
5. If relayer fails > 100 blocks, user can cancel
```

**Timeout & Slashing:**
- Withdrawal timeout: 100 blocks
- If relayer doesn't execute within timeout:
  - User can call CancelWithdrawV1 to reclaim funds
  - Relayer gets slashed (BRIDGE_CONTRACT_SLASH_AMOUNT)

### Zcash

Zcash Sapling provides fully shielded transactions with zero-knowledge proofs:

| Aspect | Ethereum | Zcash |
|--------|----------|-------|
| Address type | Regular public keys | Shielded (zaddr) |
| Ownership proof | Signatures | Groth16 zk-SNARKs |
| Privacy | Transparent | Full shielding |
| TX visibility | Public | Private |

**The Pitch: "Shield your Zcash once and forever more"**

Once your ZEC is deposited to the DarkWow bridge:
- Your ZEC remains in a Sapling shielding on the Zcash chain
- You receive wZEC on DarkWow for private DeFi
- All subsequent DarkWow transactions are completely private
- When you unwrap, wZEC burns and ZEC returns to your Zcash address
- Your Zcash transaction history stays private **forever**

Unlike other bridges that require you to re-shield on every transaction, DarkWow's bridge means you only shield **once**.

**ZEC Deposit Flow:**
```
1. User creates Sapling shielded address (zaddr)
2. User sends ZEC to DarkWow bridge shielded address
3. Relayer observes deposit via light walletd RPC (view key only)
4. Relayer constructs proof showing note exists in Sapling tree
5. User submits DepositV1 with ZcashDepositProof
6. Contract verifies anchor + merkle proof + confirmations
7. Contract mints wZEC to user
```

**Trust Model:**
- Relayer: Honest-but-curious (view key only, cannot spend)
- Economic incentives + overcollateralization prevent fraud
- Spend proof verification stubbed in MVP (would require Groth16 verifier)

**ZEC Constants:**
- Minimum deposit: 0.0001 ZEC (10,000 zatoshi)
- Confirmations required: 10 blocks

**ZEC Withdrawal Flow:**
```
1. User burns wZEC on DarkWow
2. User specifies recipient hash (zaddr or taddr)
3. Relayer picks up pending withdrawal
4. Relayer broadcasts TX to Zcash network
5. If relayer fails > 100 blocks, user can cancel
```

### Aztec

Aztec is a private rollup on Ethereum enabling fully private smart contracts. Perfect for private DAI and ETH transfers:

| Aspect | Ethereum | Aztec |
|--------|----------|-------|
| Privacy | Transparent | Full privacy |
| Tokens | All ERC-20 | ETH + ERC-20 (DAI) |
| Technology | Direct | ZK Rollup |
| Gas | High | Low (batched) |

**The Pitch: "Private DAI and ETH - Aztec's private DeFi made portable"**

Unlike transparent Ethereum transactions, Aztec keeps your:
- Transaction amounts private
- Counterparties private
- Full DeFi history private

Once your DAI/ETH is deposited to the DarkWow bridge via Aztec:
- Your funds remain private in Aztec's rollup on Ethereum
- You receive wDAI/wETH on DarkWow for private DeFi
- All subsequent DarkWow transactions are completely private
- When you unwrap, tokens return to your Aztec private account
- Your DeFi history stays private **forever**

**AZT Deposit Flow:**
```
1. User deposits ETH/DAI into Aztec bridge on Ethereum
2. Aztec rollup processes deposit and creates private note
3. Relayer observes deposit via Ethereum events (encrypted data)
4. Relayer constructs proof showing note exists in rollup tree
5. User submits DepositV1 with AztecDepositProof
6. Contract verifies rollup inclusion + note proof
7. Contract mints wETH/wDAI to user
```

**Trust Model:**
- Relayer: Honest-but-curious (observes encrypted notes, cannot spend)
- Aztec rollup: Trustless (ZK proofs ensure validity)
- Economic incentives + overcollateralization prevent fraud

**AZT Constants:**
- Minimum deposit: 0.001 ETH or equivalent
- Confirmations required: 5 Ethereum blocks after rollup
- Supported assets: ETH (0), DAI (1)

**AZT Withdrawal Flow:**
```
1. User burns wETH/wDAI on DarkWow
2. User specifies recipient Aztec address hash
3. Relayer picks up pending withdrawal
4. Relayer broadcasts private TX to Aztec rollup
5. If relayer fails > 100 blocks, user can cancel
```

### Litecoin

Litecoin is Bitcoin's silver - fundamentally similar to Bitcoin but with faster block times and active development. Key features:

| Aspect | Bitcoin | Litecoin |
|--------|---------|----------|
| Block time | 10 min | 2.5 min |
| Fees | Higher | Lower |
| PoW | SHA256 | Scrypt |
| Privacy | Transparent | MWEB (MimbleWimble) |

**The Pitch: "The Monero trade pair - move in and out of privacy with LTC"**

Litecoin is the natural segueway to the Bitcoin ecosystem:
- **XMR/LTC trading pair**: Most Monero trades happen via LTC on exchanges
- **Lower fees than Bitcoin**: Move in/out of privacy cheaper
- **MWEB privacy**: MimbleWimble extension blocks for confidential transactions
- **Segue to Bitcoin**: Natural stepping stone to/from the Bitcoin ecosystem
- **Already used this way**: Traders use LTC as a privacy stepping stone to Monero

**LTC Deposit Flow:**
```
1. User deposits LTC to DarkWow bridge address on Litecoin
2. Relayer observes deposit via Litecoin RPC (transparent or MWEB)
3. Relayer constructs proof showing:
   - Deposit exists in Litecoin blockchain
   - Amount verified (via UTXO or MWEB commitment)
4. User submits DepositV1 with LitecoinDepositProof
5. Contract verifies merkle proof + amount
6. Contract mints wLTC to user
```

**Trust Model:**
- Relayer: Honest-but-curious (observes deposits, cannot spend)
- Litecoin network: Trustless (confirmed by merkle proof)
- Economic incentives prevent fraud

**LTC Constants:**
- Minimum deposit: 0.001 LTC (100,000 satoshis)
- Confirmations required: 6 blocks (~15 min with 2.5 min blocks)
- Supported: Transparent UTXO + MWEB confidential

**LTC Withdrawal Flow:**
```
1. User burns wLTC on DarkWow
2. User specifies recipient Litecoin address (LTC or MWEB)
3. Relayer picks up pending withdrawal
4. Relayer broadcasts TX to Litecoin network
5. If relayer fails > 100 blocks, user can cancel
```

## Stablecoin Integration

wXMR can be used as collateral in the [stablecoin contract](../stablecoin.md):

**Full Flow: XMR → wXMR → Stablecoin Collateral → Mint Stablecoin**
```
1. XMR → DarkWow (Deposit):
   - User deposits XMR to bridge one-time address
   - Relayer observes + verifies via DLEq proof
   - DarkWow mints wXMR to user

2. wXMR → Collateral (DepositCollateral):
   - User deposits wXMR to stablecoin pool
   - CollateralPool tracks wXMR deposits
   - User receives debt shares

3. Collateral → Stablecoin (MintStable):
   - User locks collateral + pays stability fee
   - Receives stablecoin (e.g., USD-stable)
   - Must maintain collateralization ratio

4. Stablecoin → Collateral (RepayStable + WithdrawCollateral):
   - User repays stablecoin debt
   - Withdraws wXMR collateral

5. wXMR → XMR (Withdraw):
   - User burns wXMR on bridge
   - Relayer executes withdrawal on Monero
```

**Price Feed:**
- XMR/USD price used for collateral valuation
- Fallback price: 150 USD per XMR (until DEX pool exists)
- In production, TWAP from XMR/DRKW or XMR/USD AMM pool

## Structure

```
src/contract/bridge/
├── proof/
│   ├── deposit_v1.zk       # Ethereum deposit circuit
│   ├── withdraw_v1.zk        # Withdrawal circuit
│   └── xmr_deposit_v1.zk    # Monero deposit circuit (DLEq stubbed)
├── src/
│   ├── client/mod.rs        # DepositBuilder, WithdrawBuilder
│   ├── entrypoint.rs        # Contract implementation
│   ├── error.rs
│   ├── lib.rs               # BridgeFunction enum, constants
│   └── model/mod.rs         # DepositParams, WithdrawParams, XmrDepositProof
├── tests/
├── Cargo.toml
└── Makefile
```

## Relayer Binary

```
bin/xmr_relayer/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── monero_rpc.rs        # Monero RPC client
│   ├── proof.rs             # ZK proof construction
│   └── withdrawal.rs        # Withdrawal handling + timeout
├── xmr_relayer_config.toml
└── Cargo.toml
```

## Building

```bash
cd src/contract/bridge
make        # Build WASM contract
make proof  # Compile ZK circuits
cargo test  # Run tests
```

## References

- [Bridge Architecture](../../contract/bridge.md)
- [Object Capability Security](https://en.wikipedia.org/wiki/Object-capability_model)
