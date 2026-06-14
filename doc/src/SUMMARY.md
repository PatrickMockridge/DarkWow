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

- [NativeToken](contract/native_token.md)
- [Promissory Note](contract/promissory_note.md)
  - [Bearer Bond](contract/bearer_bond.md)
  - [PN Intermediaries](contract/promissory_note_intermediaries.md)
- [Deployooor](contract/deployooor.md)
- [Stablecoin](contract/stablecoin.md)
- [DEX](contract/dex.md)
- [DAO](contract/dao.md) [DEPRECATED]
  - [DAO-Escrow](contract/dao_escrow.md)
- [Security Audit](contract/audit.md)
- [Bridge](contract/bridge.md)
- [Auction](contract/auction.md)
- [Escrow](contract/escrow.md)
- [OTC Swap](contract/otc_swap.md)
- [Subscription](contract/subscription.md)
- [Attestation](contract/attestation.md)
    - [Composability](contract/composability.md)
      - [Recruitment Pipeline Case Study](contract/recruitment_pipeline.md)
- [Oracle](contract/oracle.md)
- [Drain Protection](contract/drain_protection.md)
- [Relayer Endowment](contract/relayer_endowment.md)
- [Tau Task Delegation](contract/tau.md)
- [Entropy Module](contract/entropy.md)
- [Provable Randomness](contract/provable_randomness.md)
- [Game Room](contract/game_room.md)
  - [Game Room App Layer](contract/game_room_app_layer.md)
- [Baccarat](contract/baccarat.md)
- [DarkToshi Dice](contract/darktoshi_dice.md)
- [Lottery](contract/lottery.md)
- [Roulette](contract/roulette.md)
- [Slot](contract/slot.md)
- [Betting Stake](contract/betting_stake.md)
- [Insurance Market](contract/insurance_market.md)
  - [Risk Market Ecosystem](contract/risk_market_ecosystem.md)
  - [Pool Stake](contract/pool_stake.md)
- [Labor Market](contract/labor_market.md)
- [Tender](contract/tender.md)
- [DarkBet Exchange](contract/darkbet_exchange.md)

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
  - [DarkWow Daemon](dwowd.md)
  - [Overview](arch/overview.md)
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

## Smart Contracts
  - [Smart Contracts](arch/sc/sc.md)
    - [Transaction lifetime](arch/sc/tx-lifetime.md)
  - [Contract Invocation API](arch/contract_invoke_api.md)
  - [Contract Deployment Pipeline](arch/dwowd_contract_pipeline.md)
  - [Contract Manifest](arch/manifest.md)
  - [Contract Trust Model](arch/contract-trust-model.md)
  - [Wallet Architecture](arch/wallet.md)
  - [Anonymous assets](arch/anonymous_assets.md)
  - [O-Cap & Composable Privacy](arch/ocap.md)
    - [Identity Contract](arch/identity.md)

## Developer Tooling
  - [Security Analysis](arch/security-analysis.md)
  - [Debugging FAQ](arch/debugging_faq.md)
  - [Testing Overview](dev/testing/overview.md)
    - [Level 1: Lightweight Tests](dev/testing/level-1-lightweight.md)
    - [Python Contract Simulations](dev/testing/python-simulations.md)
    - [Level 2: Heavyweight Tests](dev/testing/level-2-heavyweight.md)
    - [Level 3: Containerized Localnet](dev/testing/level-3-localnet.md)
    - [Level 4: Containerized Devnet Node](dev/testing/level-4-devnet.md)
  - [Wallet Testing](dev/testing/wallet-testing.md)
- [Native Mining Workflow](dev/native-workflow.md)
- [Bridge Node (Docker)](dev/bridge-node.md)
  - [Public Key Constraint Hook](arch/pubkey-constraint-hook.md)
  - [Tooling](arch/tooling.md)

## Network & Services
  - [P2P Network](arch/net/p2p-network.md)
  - [Network Types](arch/network-types.md)
  - [Services](arch/services.md)
  - [Slashing & Economic Security](arch/slashing.md)

## Economics
  - [Merge Mining](arch/merge-mining.md)
  - [Monero Merge Mining](arch/monero-merge-mining.md)
  - [Mining Tokenomics](arch/mining-tokenomics.md)
  - [Caveat Emptor: Pricing, Coverage & Adversarial Analysis](arch/economics-caveat-emptor.md)

- Contract Implementations
  - [Bridge Contract](dev/contracts/bridge.md)
  - [DEX Contract](dev/contracts/dex.md)
  - [Identity Contract](dev/contracts/identity.md)
  - [Stablecoin Contract](dev/contracts/stablecoin.md)
  - [NativeToken Contract](dev/contracts/native_token.md)
  - [Building SDKs and Apps](dev/building_sdks_apps.md)
- [zkas](zkas/index.md)
  - [Writing ZK Proofs](zkas/writing-zk-proofs.md)
  - [Bincode](zkas/bincode.md)
  - [zkVM](zkas/zkvm.md)
  - [Examples](zkas/examples.md)
    - [Anonymous voting](zkas/examples/voting.md)
    - [Anonymous payments](zkas/examples/sapling.md)
- JSON-RPC API Reference
  - [dwowd JSON-RPC API](clients/dwowd_jsonrpc.md)

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
