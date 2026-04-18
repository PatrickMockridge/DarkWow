# DarkFi Architecture Documentation

Navigation index for smart contracts, consensus, and protocol documentation.

## Core Concepts

- [Overview](./overview.md) - Blockchain, WASM contracts, tokens, ZK proofs
- [Spend Hooks](./spend_hook.md) - Cross-contract call authorization
- [NativeToken](./native_token.md) - Consensus token (fees/rewards)

## Smart Contracts (by category)

### Financial
- [money_v3](./money_v3.md) - Privacy-first DeFi tokens (STANDARD)
- [stablecoin](./stablecoin.md) - Collateral-backed stablecoin
- [dex](./dex.md) - Atomic swap exchange
- [auction](./auction.md) - Privacy-preserving auctions
- [escrow](./escrow.md) - HTLC-style trustless escrow
- [subscription](./subscription.md) - Recurring payments via DAO

### Governance
- [dao](./dao.md) - Decentralized autonomous organization
- [dao_escrow](./dao_escrow.md) - DAO-controlled escrow

### Identity
- [identity](./identity.md) - ZK credential system
- [attestation](./attestation.md) - Claims and delegation

### Gambling
- [baccarat](./baccarat.md) - Privacy-preserving Baccarat
- [darktoshi_dice](./darktoshi_dice.md) - On-chain dice game
- [lottery](./lottery.md) - Configurable lottery
- [roulette](./roulette.md) - Casino roulette
- [slot](./slot.md) - Slot machine
- [betting_stake](./betting_stake.md) - Trustless betting stake
- [pool_stake](./pool_stake.md) - Pooled coverage for relayers

### Cross-Chain
- [bridge](./bridge.md) - Multi-chain asset transfers
- [atomic_swap](./atomic_swap.md) - Trustless cross-chain swaps
- [monero](./monero.md) - Monero integration

### Other
- [drain_protection](./drain_protection.md) - Smart wallet security
- [oracle](./oracle.md) - Push-model data feeds
- [game_room](./game_room.md) - Multiplayer game coordination

## Protocol

- [consensus](./consensus.md) - RandomX Proof-of-Work
- [sync](./sync.md) - Block synchronization
- [entropy](./entropy.md) - Randomness generation
- [tau](./tau.md) - Staking and token distribution
- [slashing](./slashing.md) - Validator punishment
- [ocap](./ocap.md) - Object-capability security

## ZK & Circuits

- [opcode_universe](./opcode_universe.md) - All ZK opcodes reference
- [opcodes](./opcodes.md) - Opcode implementations
- [zkvm_primitives](./zkvm_primitives.md) - Circuit primitive functions
- [field_arithmetic](./field_arithmetic.md) - Finite field math in circuits
- [safemath](./safemath.md) - Safe arithmetic gadgets

## Reference

- [Contract Invoke API](./contract_invoke_api.md) - Inter-contract calling convention
- [pipeline](./pipeline.md) - Testing pipelines (lightweight + heavyweight)
- [test_harness_guide](./test_harness_guide.md) - Writing contract integration tests
- [genesis_harness](./genesis_harness.md) - Baseline chain setup
- [localnet_contract_testing](./localnet_contract_testing.md) - Local devnet testing
- [anonymous_assets](./anonymous_assets.md) - Privacy token model
- [wallet](./wallet.md) - Wallet architecture and key management

## Legacy

Historical documents in [./legacy/](.legacy/):
- [money-vulnerability-analysis.md](./legacy/money-vulnerability-analysis.md) - Fork decision rationale (2024)
- [money-version-bridge.md](./legacy/money-version-bridge.md) - Fork vs bridge explanation (2024)

## Directory Structure

```
doc/src/arch/
├── README.md              # This file - navigation index
├── overview.md            # Current architecture overview
├── spend_hook.md          # Cross-contract call pattern
├── native_token.md        # Consensus token contract
├── consensus.md           # PoW consensus
├── dao.md                 # Governance contract
├── dex.md                 # Atomic swap exchange
├── money_v3.md            # Privacy-first DeFi tokens
├── stablecoin.md          # Collateral stablecoin
├── opcode_universe.md     # ZK opcodes
├── opcodes.md             # Opcode implementations
├── pipeline.md            # Testing pipelines
├── test_harness_guide.md  # Contract testing guide
├── legacy/                # Historical documents
│   ├── money-vulnerability-analysis.md
│   └── money-version-bridge.md
├── net/                   # P2P network docs
│   └── p2p-network.md
└── sc/                    # Smart contract docs
    └── tx-lifetime.md
```