# DarkWow Architecture Documentation

Navigation index for smart contracts, consensus, and protocol documentation.

## Core Concepts

- [Overview](./overview.md) - Blockchain, WASM contracts, tokens, ZK proofs
- [Mining Tokenomics](./mining-tokenomics.md) - Supply, reward schedule, tail emission, uncle pins, difficulty
- [Spend Hooks](./zk/spend_hook.md) - Cross-contract call authorization
- [NativeToken](../contract/native_token.md) - Consensus token (fees/rewards)

## Smart Contracts (by category)

### Financial
- [money_v3](../contract/money_v3_migration.md) - Privacy-first DeFi tokens (STANDARD)
- [stablecoin](../contract/stablecoin.md) - Collateral-backed stablecoin
- [dex](../contract/dex.md) - Atomic swap exchange
- [auction](../contract/auction.md) - Privacy-preserving auctions
- [escrow](../contract/escrow.md) - HTLC-style trustless escrow
- [subscription](../contract/subscription.md) - Recurring payments via DAO

### Governance
- [dao](../contract/dao.md) - Decentralized autonomous organization
- [dao_escrow](../contract/dao_escrow.md) - DAO-controlled escrow

### Identity
- [identity](./identity.md) - ZK credential system
- [attestation](../contract/attestation.md) - Claims and delegation

### Gambling
- [baccarat](../contract/baccarat.md) - Privacy-preserving Baccarat
- [darktoshi_dice](../contract/darktoshi_dice.md) - On-chain dice game
- [lottery](../contract/lottery.md) - Configurable lottery
- [roulette](../contract/roulette.md) - Casino roulette
- [slot](../contract/slot.md) - Slot machine
- [betting_stake](../contract/betting_stake.md) - Trustless betting stake
- [pool_stake](../contract/pool_stake.md) - Pooled coverage for relayers

### Cross-Chain
- [bridge](../contract/bridge.md) - Multi-chain asset transfers
- [atomic_swap](../contract/atomic_swap.md) - Trustless cross-chain swaps
- [monero](./monero.md) - Monero integration

### Other
- [drain_protection](../contract/drain_protection.md) - Smart wallet security
- [oracle](../contract/oracle.md) - Push-model data feeds
- [game_room](../contract/game_room.md) - Multiplayer game coordination

## Protocol

- [consensus](./consensus/consensus.md) - Uncle Merkle consensus with RandomX PoW (replaces upstream overlay/diff)
- [sync](./sync.md) - Block synchronization
- [entropy](../contract/entropy.md) - Randomness generation
- [tau](../contract/tau.md) - Staking and token distribution
- [slashing](./slashing.md) - Validator punishment
- [ocap](./ocap.md) - Object-capability security
- [linear_zkvm](./consensus/linear_zkvm.md) - ZKVM on linear blockchain (stateless verification + WASM adapters)
- [linear_blockchain](./consensus/linear_blockchain.md) - Linear chain architecture
- [uncle_merkle](./consensus/uncle_merkle.md) - Uncle Merkle consensus — Pareto efficient fork handling without upstream's overlay complexity

## ZK & Circuits

- [opcode_universe](./zk/opcode_universe.md) - All ZK opcodes reference
- [opcodes](./zk/opcodes.md) - Opcode implementations
- [zkvm_primitives](./zk/zkvm_primitives.md) - Circuit primitive functions
- [field_arithmetic](./zk/field_arithmetic.md) - Finite field math in circuits
- [safemath](./zk/safemath.md) - Safe arithmetic gadgets
- [zk_verification](./zk/zk_verification.md) - Pure stateless ZK proof verification

## Reference

- [Contract Invoke API](./contract_invoke_api.md) - Inter-contract calling convention
- [Testing Overview](../dev/testing/overview.md) — Four-level testing taxonomy
  - [Level 1: Lightweight Tests](../dev/testing/level-1-lightweight.md)
  - [Level 2: Heavyweight Tests](../dev/testing/level-2-heavyweight.md)
  - [Level 3: Containerized Localnet](../dev/testing/level-3-localnet.md)
  - [Level 4: Containerized Devnet Node](../dev/testing/level-4-devnet.md)
- [anonymous_assets](./anonymous_assets.md) - Privacy token model
- [wallet](./legacy/wallet.md) - Wallet architecture and key management

## Legacy

Historical documents in [./legacy/](.legacy/):
- [money-vulnerability-analysis.md](./legacy/money-vulnerability-analysis.md) - Fork decision rationale (2024)
- [money-version-bridge.md](./legacy/money-version-bridge.md) - Fork vs bridge explanation (2024)

## Directory Structure

```
doc/src/arch/
├── README.md                    # This file - navigation index
├── overview.md                  # Current architecture overview
├── identity.md                  # ZK credential system
├── monero.md                    # Monero integration
├── ocap.md                      # Object-capability security
├── slashing.md                  # Validator slashing
├── anonymous_assets.md          # Privacy token model
├── contract_invoke_api.md       # Inter-contract calling convention
├── pipeline.md                  # → ../dev/testing/ (redirect stub)
├── test_harness_guide.md        # → ../dev/testing/ (redirect stub)
├── genesis_harness.md           # → ../dev/testing/ (redirect stub)
├── localnet_contract_testing.md # → ../dev/testing/ (redirect stub)
├── consensus/                   # Consensus documents
│   ├── consensus.md
│   ├── linear_blockchain.md
│   ├── linear_zkvm.md
│   └── uncle_merkle.md
├── zk/                          # ZK & circuit documents
│   ├── spend_hook.md
│   ├── field_arithmetic.md
│   ├── zkvm_primitives.md
│   ├── zk_verification.md
│   ├── opcodes.md
│   ├── opcodes-status.md
│   ├── opcode_universe.md
│   ├── safemath.md
│   └── merkle_depth.md
├── legacy/                      # Historical documents
│   ├── money-vulnerability-analysis.md
│   └── money-version-bridge.md
├── net/                         # P2P network docs
│   └── p2p-network.md
└── sc/                          # Smart contract docs
    ├── sc.md
    └── tx-lifetime.md

doc/src/contract/                # Smart contract design docs
├── native_token.md
├── money_v3_migration.md
├── dao.md
├── dao_escrow.md
├── dex.md
├── stablecoin.md
├── bridge.md
├── atomic_swap.md
└── ... (33 contract files total)
```