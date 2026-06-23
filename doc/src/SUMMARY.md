# Summary

# About

- [Development Fork Info](intro.md)
- [DarkWow](README.md)
- [What's Different from Upstream](about/differences_from_upstream.md)
- [DarkWow for Dummies](about/for-dummies.md)
- [Start Here](start-here.md)
- [Philosophy](philosophy/philosophy.md)
  - [Ideology](philosophy/ideology.md)
  - [Books](philosophy/books.md)
  - [Learn](philosophy/learn.md)

# User Guide

- [Testnet Bootstrapping Plan](testnet/bootstrapping.md)
- [Running a Node](testnet/node.md)
- [Tokens](testnet/token.md)
- [Payments](testnet/payment.md)
- [DAO [DEPRECATED]](testnet/dao.md)
- [Contracts](testnet/contract.md)
- [Merge Mining](testnet/merge-mining.md)
- [Bridge, Wrapping & Stablecoins](arch/monero.md)
- [Mining on Testnet](testnet/testnet-mining.md)
- [Block Explorer](testnet/block-explorer.md)
- [DarkIRC](misc/darkirc/darkirc.md)
  - [Private Message](misc/darkirc/private_message.md)
- [Node Configurations](misc/nodes/node-configurations.md)
  - [Public Node Configurations](misc/nodes/public-guide.md)
  - [Tor Nodes](misc/nodes/tor-guide.md)
  - [I2p Nodes](misc/nodes/i2p-guide.md)
  - [Nym Nodes](misc/nodes/nym-guide.md)
- [Network Troubleshooting](misc/network-troubleshooting.md)

# Contracts

- [Contracts](contracts.md) ← Canonical catalog (all 32)
- [Security Audit](contract/audit.md)

## Genesis

- [Genesis Contracts](arch/genesis.md) ← Canonical list
- [NativeToken](contract/native_token.md)
- [Deployooor](contract/deployooor.md)
- [Promissory Note](contract/promissory_note.md)
  - [Bearer Bond](contract/bearer_bond.md)
  - [PN Intermediaries](contract/promissory_note_intermediaries.md)
- [Identity](contract/identity.md)
- [Oracle](contract/oracle.md)
- [Attestation](contract/attestation.md)
- [Purse](contract/purse.md)
- [Box](contract/box.md)
- [MultiSig](contract/multisig.md)

## DeFi

- [Stablecoin](contract/stablecoin.md)
- [DEX](contract/dex.md)
- [Bridge](contract/bridge.md)
- [OTC Swap](contract/otc_swap.md)
- [Escrow](contract/escrow.md)
- [Auction](contract/auction.md)
- [Pool Stake](contract/pool_stake.md)

## Gaming

- [Baccarat](contract/baccarat.md)
- [DarkToshi Dice](contract/darktoshi_dice.md)
- [Roulette](contract/roulette.md)
- [Slot](contract/slot.md)
- [Lottery](contract/lottery.md)
- [Game Room](contract/game_room.md)
  - [Game Room App Layer](contract/game_room_app_layer.md)
- [DarkBet Exchange](contract/darkbet_exchange.md)
- [Betting Stake](contract/betting_stake.md)
- [Entropy Module](contract/entropy.md)
- [Provable Randomness](contract/provable_randomness.md)

## DAO & Governance

- [DAO Escrow](contract/dao_escrow.md)
- [DAO](contract/dao.md) [DEPRECATED]
- [Drain Protection](contract/drain_protection.md)

## Identity & Reputation

Identity and Attestation are [genesis contracts](arch/genesis.md) — see Genesis section above.
- [Composability](contract/composability.md)
  - [Recruitment Pipeline Case Study](contract/recruitment_pipeline.md)
- [Subscription](contract/subscription.md)

## Labor & Markets

- [Labor Market](contract/labor_market.md)
- [Insurance Market](contract/insurance_market.md)
  - [Risk Market Ecosystem](contract/risk_market_ecosystem.md)
- [Tender](contract/tender.md)

## Infrastructure

Oracle is a [genesis contract](arch/genesis.md) — see Genesis section above.
- [Relayer Endowment](contract/relayer_endowment.md)
- [Transaction Commitment](contract/tx-commitment.md)
- [Tau Task Delegation](contract/tau.md)

# Relayer Operations

- [Universal Relayer](relayer/relayer.md)
  - [Pool Stake Contract](relayer/pool_stake.md)
  - [Relayer Endowment Contract](relayer/endowment.md)
  - [Relayer Economics](relayer/relayer_economics.md)

# Developer Doc

- [Developer Quick Start](dev/quickstart.md)
- [Contributing & Developer Guide](dev/contrib/contrib.md)
  - [Contract Overview](dev/contracts.md)
  - [ZK Circuit Troubleshooting](dev/zk-circuit-troubleshooting.md)
- [AI-Assisted Development](dev/ai-assisted-development.md)
- [Architecture](arch/README.md)
  - [Formal Specification](arch/formal-specification.md) ← Start here
  - [DarkWow Daemon](dwowd.md)
  - [Overview](arch/overview.md)

## Core Architecture
  - [Wallet Architecture](arch/wallet.md)
  - [Contract Manifest](arch/manifest.md)
  - [Contract Trust Model](arch/contract-trust-model.md)
  - [Contract Deployment Pipeline](arch/dwowd_contract_pipeline.md)
  - [ZK Engineering Posture](arch/zk-engineering-posture.md)
  - [O-Cap & Composable Privacy](arch/ocap.md)
  - [Quantum-OS & Promissory Note Bridge](arch/quantum-os.md)
  - [Anonymous Assets](arch/anonymous_assets.md)

## Consensus
  - [Consensus](arch/consensus/consensus.md)
  - [Stratum Protocol](arch/consensus/stratum.md)
  - [Uncle Merkle](arch/consensus/uncle_merkle.md)
  - [Scaling & Sharding](arch/consensus/scaling.md)
  - [Linear Blockchain (theory)](arch/consensus/linear_blockchain.md)
  - [Chain Architecture (implementation)](arch/consensus/chain_architecture.md)
  - [Linear zkVM](arch/consensus/linear_zkvm.md)
  - [Caribina Finality](arch/caribina.md)

## ZK Primitives
  - [Spend Hooks](arch/zk/spend_hook.md)
  - [Field Arithmetic](arch/zk/field_arithmetic.md)
  - [zkVM Primitive Layer](arch/zk/zkvm_primitives.md)
  - [Opcodes](arch/zk/opcodes.md)
  - [Opcode Status](arch/zk/opcodes-status.md)
  - [Opcode Universe](arch/zk/opcode_universe.md)
  - [ZK Verification](arch/zk/zk_verification.md)
  - [MerkleRoot Depth](arch/zk/merkle_depth.md)
  - [Quantum Threat](arch/quantum-threat.md)

## Smart Contracts
  - [Smart Contracts](arch/sc/sc.md)
    - [Transaction lifetime](arch/sc/tx-lifetime.md)
  - [Contract Invocation API](arch/contract_invoke_api.md)
  - [Contract Metadata](arch/contract-metadata.md)
  - [Identity Contract (O-Cap)](arch/identity.md)

## Developer Tooling
  - [Security Analysis](arch/security-analysis.md)
  - [Debugging FAQ](arch/debugging_faq.md)
  - [Testing Overview](dev/testing/overview.md)
    - [Level 1: Lightweight Tests](dev/testing/level-1-lightweight.md)
    - [Python Contract Simulations](dev/testing/python-simulations.md)
    - [Level 2: Heavyweight Tests](dev/testing/level-2-heavyweight.md)
    - [Level 3: Containerized Localnet](dev/testing/level-3-localnet.md)
    - [Level 4: Containerized Devnet Node](dev/testing/level-4-devnet.md)
    - [Build Resource Tuning](dev/testing/build-resource-tuning.md)
    - [Build Resource HAZOP](dev/testing/build-resource-hazop.md)
  - [Wallet Testing](dev/testing/wallet-testing.md)
  - [Contract Testing Guide](dev/contracts_testing.md)
  - [Genesis Harness](arch/genesis_harness.md)
  - [Contract Testing Pipeline](arch/pipeline.md)
  - [Test Harness Guide](arch/test_harness_guide.md)
  - [Localnet Contract Testing](arch/localnet_contract_testing.md)
  - [Local Devnet Setup](localnet-dev.md)
- [Native Mining Workflow](dev/native-workflow.md)
- [Bridge Node (Docker)](dev/bridge-node.md)
  - [Public Key Constraint Hook](arch/pubkey-constraint-hook.md)
  - [Tooling](arch/tooling.md)

## Network & Services
  - [P2P Network](arch/net/p2p-network.md)
  - [Network Types](arch/network-types.md)
  - [Services](arch/services.md)
  - [Sync Module](arch/sync.md)
  - [Slashing & Economic Security](arch/slashing.md)

## Economics
  - [Merge Mining](arch/merge-mining.md)
  - [Monero Merge Mining](arch/monero-merge-mining.md)
  - [Mining Tokenomics](arch/mining-tokenomics.md)
  - [Caveat Emptor: Pricing, Coverage & Adversarial Analysis](arch/economics-caveat-emptor.md)

## Contract Implementations
  - [Contract Safety (Formal Verification)](dev/contracts/safety.md) ← Start here
  - [Contract Standards](dev/contracts/standards.md)
  - [AuthMint Security Analysis](dev/contracts/auth_mint_security_analysis.md) [HISTORICAL]
  - [Bridge Contract](dev/contracts/bridge.md)
  - [DEX Contract](dev/contracts/dex.md)
  - [Identity Contract](dev/contracts/identity.md)
  - [Stablecoin Contract](dev/contracts/stablecoin.md)
  - [NativeToken Contract](dev/contracts/native_token.md)
  - [Rust-WASM Interaction](dev/rust-wasm-interaction.md)
  - [Building SDKs and Apps](dev/building_sdks_apps.md)
- [zkas](zkas/index.md)
  - [Writing ZK Proofs](zkas/writing-zk-proofs.md)
  - [Bincode](zkas/bincode.md)
  - [zkVM](zkas/zkvm.md)
  - [Examples](zkas/examples.md)
    - [Anonymous voting](zkas/examples/voting.md)
    - [Anonymous payments](zkas/examples/sapling.md)
## JSON-RPC API Reference
  - [dwowd JSON-RPC API](clients/dwowd_jsonrpc.md)
  - [darkfid JSON-RPC API](clients/darkfid_jsonrpc.md) [LEGACY]

# Crypto

- [FFT](crypto/fft.md)
- [ZK explainer](crypto/zk_explainer.md)
- [Research](crypto/research.md)
- [Rate-Limit Nullifiers](crypto/rln.md)
- [Key Recovery Scheme](crypto/key-recovery.md)
- [Reading maths books](crypto/reading-maths-books.md)

# User Interface

- [UI](ui/ui.md)

# DEP

- [DEP 0001: Version Message Info (accepted)](dep/0001.md)
- [DEP 0002: Smart Contract Composability (deprecated)](dep/0002.md)
- [DEP 0003: Token Mint Authorization (accepted)](dep/0003.md)
- [DEP 0004: Client wallet WASM modules (draft)](dep/0004.md)
- [DEP 0006: App Identifier for Version and Verack Messages (draft)](dep/0006.md)
- [DEP 0007: Network profiles (accepted)](dep/0007.md)
- [DEP 0008: Transaction-local State (draft)](dep/0008.md)

# Specs

- [Notation](spec/notation.md)
- [Concepts](spec/concepts.md)
- [Cryptographic Schemes](spec/crypto-schemes.md)
- [Contracts]()
  - [Deployooor](spec/contract/deploy/deploy.md)
    - [Concepts](spec/contract/deploy/concepts.md)
    - [Scheme](spec/contract/deploy/scheme.md)
  - [Vesting](spec/contract/vesting/vesting.md)
    - [Concepts](spec/contract/vesting/concepts.md)
    - [Model](spec/contract/vesting/model.md)
    - [Scheme](spec/contract/vesting/scheme.md)

# P2P API Tutorial

- [Learn](learn/learn.md)
- [P2P API Tutorial](learn/dchat/dchat.md)
  - [Async Rust Fundamentals](learn/dchat/async-rust-fundamentals.md)
  - [Deployment](learn/dchat/deployment/part-1.md)
    - [Getting started](learn/dchat/deployment/getting-started.md)
    - [Writing a daemon](learn/dchat/deployment/writing-a-daemon.md)
    - [Sessions](learn/dchat/deployment/sessions.md)
    - [Settings](learn/dchat/deployment/settings.md)
    - [Start-Run-Stop](learn/dchat/deployment/start-stop.md)
    - [Seed](learn/dchat/deployment/seed-node.md)
    - [Deploy](learn/dchat/deployment/deploy.md)
    - [Error Handling](learn/dchat/deployment/error-handling.md)
  - [Creating dchatd](learn/dchat/creating-dchatd/part-2.md)
    - [Message](learn/dchat/creating-dchatd/message.md)
    - [Understanding Protocols](learn/dchat/creating-dchatd/protocols.md)
    - [ProtocolDchat](learn/dchat/creating-dchatd/protocol-dchat.md)
    - [Register protocol](learn/dchat/creating-dchatd/register-protocol.md)
    - [Sending messages](learn/dchat/creating-dchatd/sending-messages.md)
    - [Accept addr](learn/dchat/creating-dchatd/accept-addr.md)
    - [Handling RPC requests](learn/dchat/creating-dchatd/rpc-requests.md)
    - [StoppableTask](learn/dchat/creating-dchatd/stoppable-task.md)
    - [Adding methods](learn/dchat/creating-dchatd/rpc-methods.md)
    - [Pong](learn/dchat/creating-dchatd/pong.md)
  - [Creating dchat-cli](learn/dchat/creating-dchat-cli/part-3.md)
    - [UI](learn/dchat/creating-dchat-cli/ui.md)
    - [Using dchat](learn/dchat/creating-dchat-cli/using-dchat.md)
  - [Net tools](learn/dchat/network-tools/part-4.md)
    - [get_info](learn/dchat/network-tools/get-info.md)
    - [Attaching dchat](learn/dchat/network-tools/attaching-dnet.md)
    - [Using dnet](learn/dchat/network-tools/using-dnet.md)

# Misc

- [vanityaddr](misc/vanityaddr.md)
- [darkIRC Specification](misc/darkirc/specification.md)
- [tau](misc/tau.md)
- [dnetview](misc/dnetview.md)
- [FAQ](misc/faq.md)
- [Glossary](glossary/glossary.md)

# Legacy Architecture

> **ARCHIVED**: The following documents describe the original overlay-DAG
> blockchain consensus, which has been replaced by [Uncle Merkle](arch/consensus/uncle_merkle.md).
> **Exception:** The [Event Graph](arch/legacy/event_graph.md) is still active —
> it is the P2P messaging DAG used by darkirc, not blockchain consensus.

- [Event Graph](arch/legacy/event_graph.md) *(active — P2P messaging layer)*
  - [Event Graph Network Protocol](arch/legacy/event_graph_network_protocol.md) *(active)*

# Changelogs

- [Localnet Testing (April 2026)](changelogs/2026-04-localnet-testing.md)
