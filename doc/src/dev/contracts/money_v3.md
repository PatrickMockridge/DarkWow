# MoneyV3: Privacy-First DeFi Token Contract

## Overview

MoneyV3 is DarkWow's DeFi token contract designed for ERC-20 style functionality (wrapped tokens, stablecoins, multi-token support). It implements a **complete Poseidon-only design** with zero EC operations, eliminating heap bugs entirely.

**Key Principle**: Privacy-first with 100% fungibility. Token IDs are hidden commitments, not traceable identifiers.

> [!NOTE]
> **Design Philosophy: Tokens are Infrastructure, Not Business Logic**
>
> MoneyV3 is intentionally minimal. Tokens move value - that's their only job. Complexity lives in smart contracts (DEX, stablecoin, etc.), not in the token layer. This is a deliberate security decision:
>
> - **Tokens are called frequently** - Every transfer, swap, and mint touches them
> - **Simpler = fewer bugs** - Minimal code surface means fewer vulnerabilities
> - **Bugs in tokens cascade** - A bug in a token affects every operation
> - **Bugs in smart contracts are isolated** - A bug in DEX only affects DEX operations
>
> See [MoneyV3 Architecture](../contract/money_v3_migration.md) for migration details.

## Why MoneyV3?

MoneyV3 was created to address fundamental limitations in both MoneyV2 and NativeToken:

| Aspect | MoneyV2 | NativeToken | MoneyV3 |
|--------|---------|-------------|---------|
| Purpose | General DeFi | Consensus (PoW, fees) | DeFi (tokens, stablecoins) |
| EC Operations | 4 circuits buggy | 3 circuits | **0 (Poseidon-only)** |
| Token ID | Revealed | Revealed | **Hidden commitment** |
| Fungibility | Partial | Partial | **100%** |
| Heap Bug Risk | YES | YES | **NO** |

## Design Philosophy: PRIVACY FIRST

1. **Zero EC Operations**: All computations use Poseidon hashes
2. **Hidden Token IDs**: Token identity is a commitment, not plaintext
3. **Full Composability**: spend_hook and user_data for cross-contract calls
4. **Atomic Transfers**: Burn + Mint preserves privacy across transactions

## Token Model

### Coin Structure

```rust
struct Coin {
    inner: pallas::Base,  // poseidon_hash of coin attributes
}

struct CoinAttributes {
    public_key: pallas::Base,  // poseidon_hash(secret), not EC point
    value: u64,
    token_id: pallas::Base,    // Hidden commitment
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    blind: pallas::Base,
}
```

**Coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)**

Where `pub = poseidon_hash(secret)` is a field element public key (Schnorr-style).

### Token ID Commitment

Unlike MoneyV2 where token_id is revealed, MoneyV3 uses:

```
token_id = poseidon_hash(token_auth_parent, token_user_data, token_blind)
```

This makes tokens **100% fungible** - the token type cannot be traced.

### Nullifier

```
nullifier = poseidon_hash(secret, coin)
```

Prevents double-spending by breaking the link between mint and burn.

## Contract Functions

| Function | Opcode | Purpose |
|----------|--------|---------|
| TokenMintV1 | 0x00 | Create new token types |
| AuthTokenMintV1 | 0x01 | Authorize minting for a token |
| MintV1 | 0x02 | Mint tokens of existing type |
| BurnV1 | 0x03 | Burn tokens with nullifier |
| TransferV1 | 0x04 | Atomic burn+mint transfer |
| OtcSwapV1 | 0x05 | Atomic OTC token swap |

### TokenMintV1 (0x00)

Creates a new token type. This is how new tokens (stablecoins, wrapped assets) are born.

**Parameters:**
```rust
struct TokenMintParamsV1 {
    coin: Coin,              // Initial coin commitment
    value_commit: Base,      // poseidon_hash(value, value_blind)
    token_id: Base,          // poseidon_hash(auth_parent, user_data, blind)
    token_auth_parent: Base, // Authorization parent (bound in ZK proof)
    token_commit: Base,      // poseidon_hash(token_id, token_blind)
}
```

**ZK Circuit:** `token_mint_v1.zk`

**Process:**
1. Derive token_id from auth_parent, user_data, and blind
2. Create initial coin for the token supply
3. Prove token_id doesn't already exist (via Merkle proof)

### AuthTokenMintV1 (0x01)

Authorizes minting for an existing token type. Required before MintV1 can be called.

**Parameters:**
```rust
struct AuthTokenMintParamsV1 {
    nullifier: Nullifier,           // Prevents replay
    mint_public: pallas::Base,        // Public key of authority
    token_id: pallas::Base,          // Token being authorized
    token_registry_root: MerkleNode, // Merkle proof of token existence
}
```

**ZK Circuit:** `auth_token_mint_v1.zk`

**Authorization Model:**
- token_id must exist in Token Registry Merkle tree
- mint_authority is derived from mint_secret via Poseidon
- nullifier prevents reuse of authorization

### MintV1 (0x02)

Mints new tokens of an existing authorized type.

**Parameters:**
```rust
struct MintParamsV1 {
    auth_proof: AuthProof,   // Authorization proof from AuthTokenMintV1
    coin: Coin,              // The newly minted coin
    value_commit: Base,      // Value commitment (Poseidon hash)
    token_id: Base,          // Token ID being minted
}

/// Authorization proof from AuthTokenMintV1
struct AuthProof {
    nullifier: Nullifier,           // From auth call (prevents replay)
    mint_public: pallas::Base,       // Public key of the authority
    token_registry_root: MerkleNode, // Proves token_id is authorized
}
```

**ZK Circuit:** `mint_v1.zk`

**Validation:**
- AuthProof nullifier not already spent
- AuthProof mint_public matches authorization
- Token exists in registry (via token_registry_root)
- Range check on value

### BurnV1 (0x03)

Destroys coins with nullifier generation.

**Parameters:**
```rust
struct BurnParamsV1 {
    input: Input,
}
```

**ZK Circuit:** `burn_v1.zk`

**Validation:**
- Coin exists in Merkle tree
- Nullifier not spent
- Signature from public key
- Range check on value

### TransferV1 (0x04)

Atomic burn + mint for private transfers.

**Parameters:**
```rust
struct TransferParamsV1 {
    inputs: Vec<Input>,   // Coins being spent
    outputs: Vec<Output>, // Coins being created
}
```

**ZK Circuit:** `transfer_v1.zk` (combines burn + mint proofs)

**Privacy Model:**
- Atomic burn+mint breaks transaction graph
- Nullifiers prevent double-spend
- Value balance preserved mathematically (burn == mint)

## ZK Circuits

MoneyV3 uses 4 Poseidon-only circuits:

| Circuit | Namespace | Purpose | EC Operations |
|---------|-----------|---------|---------------|
| token_mint_v1.zk | `TokenMint_V1` | Create token types | **0** |
| auth_token_mint_v1.zk | `AuthTokenMint_V1` | Authorize minting | **0** |
| mint_v1.zk | `Mint_V1` | Mint tokens | **0** |
| burn_v1.zk | `Burn_V1` | Burn tokens | **0** |

### Circuit Design Principles

1. **Zero EC Operations**: All hashes are Poseidon
2. **Schnorr Public Keys**: `pub = poseidon_hash(secret)` instead of `ec_mul_base`
3. **Poseidon Value Commitment**: `poseidon_hash(value, blind)` instead of Pedersen
4. **Range Proofs**: 64-bit value validation
5. **Merkle Proofs**: Token and coin existence verification

### Schnorr Signature in Circuits

DarkWow uses a Schnorr variant where the public key is a Poseidon hash:

```zk
# Derive public key
public_key = poseidon_hash(secret)

# Signature verification (outside ZK)
s = hash(message || public_key) * secret

# Inside circuit - constrain public key matches
constrain_instance(poseidon_hash(secret))  # Must equal public input
```

This eliminates ALL EC operations from signature verification.

## Privacy Architecture

```
MINT (TokenMintV1):
  token_id = poseidon_hash(auth_parent, user_data, blind)
  coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)
  value_commit = poseidon_hash(value, value_blind)

BURN (BurnV1):
  coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)
  nullifier = poseidon_hash(secret, coin)  # Breaks mint<->burn link
  Merkle proof: coin exists in tree
  Range check: value < 2^64

TRANSFER (TransferV1):
  Atomic burn + mint, value preserved
  Nullifiers break transaction graph
```

## Token Authorization Model

MoneyV3 uses a two-phase token creation:

### Phase 1: Create Token Type (TokenMintV1)
```
token_id = poseidon_hash(authority_parent, user_data, blind)
```
Creates the token in the registry. The token_id is a commitment.

### Phase 2: Authorize Minting (AuthTokenMintV1)
```
nullifier = poseidon_hash(mint_secret, token_id)
root = merkle_root(leaf_pos, path, token_id)
```
Authorizes a specific entity to mint this token type.

### Phase 3: Mint Tokens (MintV1)
```
Mint_V1.prove(auth_nullifier, token_merkle_proof, ...)
```
Mints actual tokens. Requires valid authorization.

This model enables:
- **Multi-token support**: Many token types in one contract
- **Authorization control**: Token issuers control supply
- **Privacy**: Token ID is hidden, only authorization is revealed

## Composability

MoneyV3 supports cross-contract calls via spend_hook:

```
BurnV1(coin, spend_hook=contract_id, user_data=params)
  -> Executes contract_id::exec(user_data)
```

This enables:
- DEX integrations
- Lending protocols
- Staking mechanisms
- Any custom DeFi logic

## Comparison with Other Contracts

| Feature | MoneyV2 | NativeToken | MoneyV3 |
|---------|---------|-------------|---------|
| **Use Case** | General DeFi | Consensus/PoW | DeFi/ERC-20 |
| **Token ID** | Revealed | Revealed | Hidden |
| **Fungibility** | Partial | Partial | **Full** |
| **EC Operations** | 4 buggy | 3 | **0** |
| **Circuits** | 5 | 3 | 4 |
| **Functions** | 9 | 6 | 6 |
| **Heap Bug** | YES | YES | **NO** |
| **Token Minting** | Yes | No | **Yes** |
| **Authorization** | ACL | None | **Merkle** |

## Database Trees

```
MONEY_V3_CONTRACT_COINS_TREE              - coin commitment -> ()
MONEY_V3_CONTRACT_NULLIFIERS_TREE         - nullifier -> spent
MONEY_V3_CONTRACT_MERKLE_TREE             - Merkle tree of all coins
MONEY_V3_CONTRACT_INFO_TREE               - contract metadata
MONEY_V3_CONTRACT_COIN_ROOTS_TREE         - historical Merkle roots
MONEY_V3_CONTRACT_NULLIFIER_ROOTS_TREE    - historical nullifier roots
```

Note: a token registry tree and auth nullifiers tree are planned but not yet implemented (see code comments in `src/contract/money_v3/src/entrypoint/mod.rs`).

## Files

- `src/contract/money_v3/` - Contract implementation
  - `src/lib.rs` - Function enum, constants
  - `src/error.rs` - Error types
  - `src/model/mod.rs` - Data models
  - `src/entrypoint/mod.rs` - WASM entrypoint
  - `src/client/` - Client APIs
    - `token_mint_v1.rs` - TokenMintCallBuilder
    - `auth_token_mint_v1.rs` - AuthTokenMintCallBuilder
    - `mint_v1.rs` - MintCallBuilder
    - `burn_v1.rs` - BurnCallBuilder
    - `transfer_v1.rs` - TransferCallBuilder
  - `proof/*.zk` - ZK circuit source (Poseidon-only)

## Testing

Money V3 is tested via `cargo run -p dwow-contract-test-harness --bin test_money_v3`.
See [Testing Overview](../testing/overview.md) for the four-level testing taxonomy
and command reference.

## Migration from MoneyV2

MoneyV2 is deprecated due to EC heap bugs. Migration path:

| MoneyV2 Function | MoneyV3 Equivalent |
|-----------------|-------------------|
| TokenMintV2 | TokenMintV1 + AuthTokenMintV1 |
| MintV2 | MintV1 (requires AuthTokenMintV1) |
| BurnV2 | BurnV1 |
| TransferV2 | TransferV1 |

**Benefits of Migration:**
- Zero heap bugs (Poseidon-only)
- Full token privacy (hidden token_id)
- 100% fungibility
- Simpler authorization model

## Integration Examples

### Stablecoin Integration

The [stablecoin](../stablecoin/stablecoin.md) contract uses MoneyV3 as its token layer:

```
Stablecoin → MoneyV3 Integration:
├── InitializeV1: Creates USDx token type in MoneyV3
├── OpenPositionV1: Mints collateral receipt tokens (MoneyV3 MintV1)
├── MintStableV1: Burns collateral tokens via spend_hook → mints USDx
├── RepayStableV1: Burns USDx → mints collateral tokens back
└── LiquidateV1: Seizure via spend_hook → rewards in USDx
```

This enables stablecoin to focus on CDP mechanics while delegating token management to MoneyV3.

### DEX Integration

See [dex.md](dex.md) for DEX integration using spend_hook for atomic swaps.

## Wallet Integration (dww)

The `dww` command-line wallet supports MoneyV3 with the following functionality:

| Feature | Status | Notes |
|---------|--------|-------|
| Coin Scanning | ✅ Implemented | `apply_tx_money_data()` in rpc.rs |
| Coin Storage | ✅ Implemented | `coins` and `coin_merkle_proofs` tables in wallet.sql |
| Secret Management | ✅ Implemented | `coin_secrets` table |
| Transfer Creation | ✅ Implemented | `transfer()` in transfer.rs with FeeV1 attachment |
| Token Creation | ✅ Implemented | `create_token()` in token.rs using TokenMintV1 |
| Mint Tokens | ✅ Implemented | `mint_tokens()` in token.rs using AuthTokenMintV1 + MintV1 |

### Transfer Implementation

The `dww transfer` command flow:

```
1. Select unspent coin with sufficient value
2. Retrieve Merkle proof from wallet database
3. Decode secret key from wallet
4. Build TransferCallBuilder with:
   - Input: coin data + Merkle proof
   - Output: recipient + change
5. Generate ZK proofs (Burn_V1 + Mint_V1)
6. Select DRKW coin for fee payment
7. Build NativeToken::FeeV1 for fee attachment
8. Combine into final transaction using TransactionBuilder
```

### Token Creation Implementation

The `dww create_token` command flow:

```
1. Generate mint authority (SecretKey) and token blind (BaseBlind)
2. Derive token_id = poseidon_hash(mint_authority_public, token_user_data, token_blind)
3. Load TokenMint ZK binary and create proving key
4. Build TokenMintV1 transaction with initial supply
5. Select DRKW coin for fee payment
6. Attach FeeV1 for network fee
7. Build and return transaction
```

### Mint Tokens Implementation

The `dww mint` command flow:

```
1. Load mint authority and token registry Merkle proof
2. Build AuthTokenMintV1 to prove mint authority
3. Build MintV1 to create new coins
4. Attach FeeV1 for network fee
5. Build and return transaction
```

**Note**: `mint_tokens()` requires explicit mint authority and token registry Merkle proof parameters.

### Scanning Implementation

During blockchain scanning:

```
1. For each MoneyV3 transaction:
2. If TransferV1 (0x04):
   - Decode TransferParamsV1
   - For each output note:
     - Try decryption with each known secret
     - If successful, extract MoneyV3Note
     - Calculate coin_id = Coin::from_attributes(...)
     - Store coin + Merkle proof in wallet
```

This allows the wallet to track all owned coins with their Merkle proofs.

## Standards Reference

For security standards (Poseidon-only, NativeToken vs MoneyV3 separation), see [standards.md](standards.md).

## References

- [Stablecoin Integration](../stablecoin/stablecoin.md)
- [DEX Integration](dex.md)
- [Standards](standards.md)
- [NativeToken](../native_token/native_token.md)