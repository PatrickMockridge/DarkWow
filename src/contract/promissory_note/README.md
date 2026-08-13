# PromissoryNote - Privacy-First DeFi Token Contract

**Status**: ✅ Production Ready | Standard for DeFi tokens

PromissoryNote is DarkWow's privacy-first token contract designed for DeFi applications. It uses **Poseidon-only ZK circuits** with EC operations pushed to smart contract verification.

## Design Principles

1. **Poseidon-only ZK**: All ZK circuit operations use Poseidon hash. No EC operations in ZK.
2. **EC in contracts**: Pedersen commitments and other EC ops happen in contract verification layer.
3. **Minimal circuits**: ZK circuits remain simple and auditable.
4. **Privacy-first**: Coin commitments, nullifiers, and value commitments all use Poseidon.

## Token Model

### Coin Commitment
```
coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)
```
Where `pub = poseidon_hash(secret)` (a field element, not EC point)

### Nullifier
```
nullifier = poseidon_hash(secret, coin)           # spending
```

### Value Commitment
```
value_commit = pedersen_commit(value, value_blind)   # Pedersen (additively homomorphic), not Poseidon
token_commit = poseidon_hash(token_id, token_blind)
```

## Functions

| ID | Function | Description |
|----|----------|-------------|
| `0x00` | RegisterTypeV1 | Create a new token type (stablecoin, wrapped, etc.) |
| `0x01` | RedeemV1 | Redeem a coin — burns value, creates a zero-value receipt |
| `0x02` | IssueV1 | Mint tokens of existing type (proves backing capability) |
| `0x03` | RevokeV1 | Burn/destroy tokens |
| `0x04` | TransferV1 | Private token transfer |
| `0x05` | OtcSwapV1 | Atomic OTC token swap |

## Circuits

| Circuit | Purpose | Witnesses |
|---------|---------|-----------|
| `token_mint_v1.zk` | Create new token type | token_auth_parent, token_user_data, token_blind, recipient, value, spend_hook, user_data, coin_blind |
| `mint_v1.zk` | Mint tokens (proves backing secret) | mint_public, token_leaf_pos, token_path, coin_public, coin_value, coin_token_id, coin_spend_hook, coin_user_data, coin_blind, value_blind |
| `burn_v1.zk` | Burn tokens | (burn proof) |

## Mint Flow

```
1. RegisterTypeV1: Create token type → token_id
   └─→ Stores token_auth_parent in registry (backing capability commitment)
   └─→ TokenMintParamsV1 { coin, value_commit, token_id, token_commit, token_auth_parent }

2. IssueV1: Mint tokens — single-step backing proof
   └─→ Proves knowledge of mint_secret against stored token_auth_parent
   └─→ Verifies token_registry_root matches on-chain state
   └─→ Creates new coin with token_id
   └─→ MintParamsV1 { coin, value_commit, token_id, token_registry_root, mint_public }
```

## Heavyweight Test Status

```bash
cargo test --release --package darkfid test_promissory_note_heavyweight
```

Test calls all endpoints:
- `RegisterTypeV1 (0x00)` - Create token type
- `IssueV1 (0x01)` - Mint tokens

## Dependencies

- `darkfi_sdk::crypto::poseidon_hash` - All hash operations
- `darkfi_sdk::crypto::MerkleNode` - Merkle tree verification
- `darkfi_sdk::bridgetree::Hashable` - Merkle tree combine

## Files

```
src/
├── lib.rs              # Contract definition and function enum
├── entrypoint/mod.rs   # Function implementation and metadata
├── model/mod.rs        # Data structures (Coin, Nullifier, Params)
├── client/
│   ├── token_mint_v1.rs     # Token creation client
│   └── mint_v1.rs           # Mint client
proof/
├── token_mint_v1.zk.bin    # Compiled circuit
└── mint_v1.zk.bin
```

## Why Poseidon-Only?

Traditional ZK circuits use EC operations (Pedersen commitments) which require heap allocation for point arithmetic. This creates potential for heap bugs and memory safety issues.

PromissoryNote's approach:
- ZK circuits: Poseidon hash only → deterministic, no heap allocation
- Smart contracts: EC operations → verifiable but outside ZK

This separation means:
- ZK circuits are auditable and formally verifiable
- Complex EC logic is isolated in contract layer
- Token contract is the most frequently called → minimal attack surface
