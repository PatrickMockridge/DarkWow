# DarkFi Architecture Documentation

Navigation index for smart contracts, consensus, and protocol documentation.

## Smart Contracts

### Financial
- [Auction](./auction.md) - English, sealed-bid, and Dutch auction types
- [DAO](./dao.md) - Governance with voting and proposal execution
- [DAO Escrow](./dao_escrow.md) - DAO-controlled escrow with premium payments
- [DEX](./dex.md) - Decentralized exchange with atomic swaps
- [Escrow](./escrow.md) - Trustless escrow with HTLC-style refunds
- [Stablecoin](./stablecoin.md) - Collateral-backed stablecoin (CDP)
- [Subscription](./subscription.md) - Recurring payment subscriptions via DAO escrow

### Identity & Attestation
- [Attestation](./attestation.md) - Claims system with delegation chains
- [Identity](./identity.md) - O-Cap authorization and credential issuance
- [Oracle](./oracle.md) - Push-model data feeds with aggregation

### Gambling
- [Baccarat](./baccarat.md) - Card game with commit-reveal randomness
- [DarkToshi Dice](./darktoshi_dice.md) - On-chain dice with weighted outcomes
- [Lottery](./lottery.md) - Parimutuel lottery with probabilistic distribution
- [Roulette](./roulette.md) - Casino roulette with house edge
- [Slot](./slot.md) - Slot machine with RNG-based reveals
- [Betting Stake](./betting_stake.md) - Trustless betting stake management

### Labor & Markets
- [Labor Market](./labor_market.md) - Job board with deliverable verification
- [Tender](./tender.md) - Sealed-bid tender system
- [Insurance Market](./insurance_market.md) - Prediction market for insurance
- [Block Height Prediction](./block_height_prediction.md) - On-chain random oracle

### Cross-Chain
- [Atomic Swap](./atomic_swap.md) - Trustless cross-chain swaps
- [Bridge](./bridge.md) - Multi-chain bridge with deposit/withdraw
- [Monero](./monero.md) - Monero integration and privacy comparison

### Other
- [Deployooor](./deployooor.md) - Arbitrary WASM contract deployment
- [Game Room](./game_room.md) - Multiplayer game coordination
- [Drain Protection](./drain_protection.md) - Smart wallet security
- [Darkbet Exchange](./darkbet_exchange.md) - Betting exchange

## Consensus & Protocol

- [Consensus](./consensus.md) - PoW consensus algorithm
- [Tau](./tau.md) - Token distribution and staking
- [Slashing](./slashing.md) - Validator punishment mechanism
- [OCap](./ocap.md) - Object-capability security model
- [Entropy](./entropy.md) - Randomness generation
- [Transaction Lifetime](./tx_lifetime.md) - Transaction processing lifecycle
- [Money V3 Migration](./money_v3_migration.md) - Privacy-first DeFi tokens with Poseidon-only circuits (HARD FORK)

## Reference

- [Opcode Universe](./opcode_universe.md) - ZK circuit opcodes reference
- [Opcodes](./opcodes.md) - Opcode implementations
- [ZKVM Primitives](./zkvm_primitives.md) - Circuit primitive functions
- [Field Arithmetic](./field_arithmetic.md) - Finite field math in circuits
- [Security Analysis](./security-analysis.md) - Audit findings and analysis
- [Contract Invoke API](./contract_invoke_api.md) - Inter-contract calling convention
- [Wallet](./wallet.md) - Wallet architecture and key management
- [Anonymous Assets](./anonymous_assets.md) - Privacy token model

## Testing & Development

- [Test Harness Guide](./test_harness_guide.md) - Writing contract integration tests
- [Localnet Contract Testing](./localnet_contract_testing.md) - Local devnet testing
- [Debugging FAQ](./debugging_faq.md) - Common issues and solutions
- [Async Serial Lifetime Bug](./async_serial_lifetime_bug.md) - Rust 1.90+ compatibility

## Legacy

Historical documents moved to [./legacy/](.legacy/):
- [money-vulnerability-analysis.md](./legacy/money-vulnerability-analysis.md) - Fork decision rationale (2024)
- [money-version-bridge.md](./legacy/money-version-bridge.md) - Fork vs bridge explanation (2024)

## Directory Structure

```
doc/src/arch/
├── README.md              # This file - navigation index
├── consensus.md           # PoW consensus
├── dao.md                 # Governance contract
├── identity.md            # O-Cap credentials
├── opcode_universe.md     # ZK opcodes
├── security-analysis.md   # Audit findings
├── *.md                   # Individual contract docs
├── legacy/                 # Historical documents
│   ├── money-vulnerability-analysis.md
│   └── money-version-bridge.md
├── net/                   # P2P network docs
│   └── ...
└── sc/                    # Smart contract docs
    └── ...
```
