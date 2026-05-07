# Overview

DarkWow is a layer-one Proof-of-Work blockchain supporting anonymous WASM smart contracts.

## Consensus

DarkWow uses **Uncle Merkle consensus** with RandomX Proof-of-Work. This replaces upstream's overlay/diff architecture with a deterministic design:

- **Pure PoW**: The canonical chain is the one with the most accumulated work. No governance DAO decides between forks.
- **Uncle Merkle pin mechanism**: The canonical chain is **obligated** to offer competing uncle chains a one-time option (within minutes) to form a side chain and share the PoW reward. Uncle chains can accept or reject.
- **Pareto efficient**: Miners who produce non-canonical blocks still earn partial reward (50% at depth 1, halving each depth). No wasted work.
- **Deterministic**: No overlay, no speculative state, no rollback. Same block = same result every time.
- **Hard forks are natural**: Without a DAO motivated to keep everything under one tent, chain splits follow the Bitcoin model — both sides coexist (like BTC/BCH). No complex mechanism needed.

See [Consensus](consensus/consensus.md) and [Uncle Merkle](consensus/uncle_merkle.md) for the full specification.

## WASM Contracts

DarkWow uses WASM smart contracts deployed via the **Deployooor** contract. This model provides:

- **Upgradeable contracts**: Contracts can be upgraded without hard forking the network
- **Minimal genesis**: Only Deployooor and NativeToken exist at genesis
- **Composable applications**: Additional contracts are deployed as needed

## Token Architecture

DarkWow separates token concerns into two specialized contracts:

| Contract | Purpose | Use Case |
|----------|---------|----------|
| **NativeToken (WASM)** | Consensus-layer operations | Block rewards, fee payment |
| **money_v3** | Privacy-first DeFi tokens | User tokens, DeFi operations |

### NativeToken

Minimal WASM contract handling only consensus requirements:
- Block reward distribution
- Fee payment

Philosophy: **Tokens are pipework, not reactors.** One job, done well.

### money_v3

Privacy-first DeFi token contract:
- **Poseidon-only ZK circuits**: All cryptographic operations use Poseidon hash. No EC operations in ZK.
- **Coin model**: `poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)`
- **Function IDs**: TokenMintV1, AuthTokenMintV1, MintV1, BurnV1, TransferV1, OtcSwapV1

## Cross-Contract Calls

Contracts communicate via **spend hooks**. A contract can call another by specifying:

- `spend_hook`: Which function to invoke (function ID)
- `user_data`: Arbitrary data passed to the hook

Example usage:
- **DEX ExecuteSwapV1**: Uses `otc_swap_v1` child call for bilateral token swap
- **Stablecoin MintStableV1**: Uses `transfer_v1` child call to move minted stablecoins to user
- **DarkbetExchange**: Uses `transfer_v1` child calls for position minting/burning

## ZK Proofs

All private state transitions use ZK proofs verified on-chain:

- **zkVM**: DarkWow's virtual machine executes Halo2 proofs
- **No trusted setup**: zkSNARK system uses universal reference strings
- **Privacy**: Zero-knowledge proofs hide amounts, identities, and state changes

## Testing

DarkWow provides two testing pipelines:

1. **Lightweight pipeline**: Deployment verification without ZK proof generation
2. **Heavyweight pipeline**: Full ZK proof generation and contract execution testing

See [Pipeline](./pipeline.md) and [Test Harness Guide](./test_harness_guide.md) for details.

## Genesis Contracts

Only two contracts exist at genesis (Satoshi-style minimalism):

1. **Deployooor**: Deploys additional WASM contracts
2. **NativeToken**: Handles block rewards and fees

All other contracts (money_v3, DEX, stablecoin, gambling games, etc.) are composed as needed.