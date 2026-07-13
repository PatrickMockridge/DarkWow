# Contract Developer Guide

You want to build a privacy-preserving smart contract on DarkWow.
Here's your path.

## What you need to know

DarkWow contracts are WASM modules that execute inside ZK proofs. Every
state transition is verified in zero-knowledge by default — you don't
bolt on privacy, it's the substrate. Contracts are written in Rust,
compiled to WASM, and proved via ZK circuits written in ZKAS (a
zero-knowledge assembly language compiled by the `zkas` compiler).

DarkWow's architecture does three things differently from every other
smart contract platform:

### The money split: NativeToken vs PromissoryNote

DarkWow splits the monetary layer into two contracts:

- **NativeToken** — Consensus-critical only. Block rewards (coinbase),
  transaction fees, and supply audit. Rock-dumb by design: no multi-token,
  no auth, no freezing. Minimal consensus attack surface.
- **PromissoryNote** — DeFi token operations. Transfer, mint, burn, freeze,
  and token authorization. All user-facing token logic.

Upstream DarkFi has a single monolithic `money` contract with 8 functions
in one enum (FeeV1 through BurnV1) sharing 7 database trees. DarkWow's
split isolates consensus-critical logic from DeFi — a promissory_note bug
cannot halt the chain, and a native_token bug is contained to coinbase/fees.

### O-Cap governance primitives

Six composable governance contracts replace the monolithic DAO:

| Contract | What it does | When to use it |
|----------|-------------|----------------|
| [Identity](contract/identity.md) | Capability-based identity and credentials | User authentication, reputation |
| [Oracle](contract/oracle.md) | External data feeds and reporting | Price feeds, external state verification |
| [Attestation](contract/attestation.md) | Verifiable claims | Trust verification, credential issuance |
| [Purse](contract/purse.md) | Capability-based resource container | Holding and transferring fungible assets |
| [Box](contract/box.md) | Capability restrictor | Spending limits, time locks, conditions |
| [MultiSig](contract/multisig.md) | Threshold-based n-of-m authorization | Shared control, DAO treasury, team wallets |

These compose into arbitrary governance structures. A Purse inside a Box
with MultiSig authorization = a time-locked team treasury. An Identity
backed by Attestation = a verifiable credential. Each primitive does one
thing; you compose them to do everything.

### The manifest system

Every contract declares its capabilities in a `manifest.toml` file.
The wallet uses these manifests for capability discovery — it doesn't
need a hardcoded ABI for your contract. Write the manifest, and the
wallet knows how to find and decode your contract's notes.

See [Manifest System](arch/manifest.md) for the full specification.

## Case study: dao_escrow

The [DAO Escrow](contract/dao_escrow.md) contract demonstrates how
O-Cap primitives compose in practice. It supports three operating modes:

- **Escrow-Only**: Purse holds funds, MultiSig controls release
- **Treasury-Only**: Box restricts spending, Identity authorizes
- **Treasury+Endowment**: Full governance with Oracle price feeds and
  Attestation-based proposal verification

Read the [DAO Escrow documentation](contract/dao_escrow.md) as a
worked example of O-Cap composition.

## Your development path

1. **Read** [Smart Contract Inherent Safety](dev/contracts/safety.md) first.
   It documents 20 real vulnerabilities found in DarkWow's contracts.
   Estimated read: 45 minutes. This is not optional.

2. **Set up** with the [Developer Quick Start](dev/quickstart.md).

3. **Learn the standards** in [Contract Standards](dev/contracts/standards.md).

4. **Understand the contract model** in [Smart Contracts](contracts.md) —
   the canonical catalog with all 32 contracts, their function codes, and
   maturity status.

5. **Write your circuit** using [zkas](zkas/zkas.md) and the
   [ZK proof writing guide](zkas/writing-zk-proofs.md).

6. **Test** through the [five-level testing pipeline](dev/testing/overview.md):
   Level 1 (lightweight) → Level 2 (full ZK) → Level 3 (localnet) → Level 4 (devnet).

7. **Deploy** via the [Deployooor](contract/deployooor.md) contract.

## Reference

- [Contract Invoke API](arch/contract_invoke_api.md) — How contracts are called
- [WASM Host Functions](arch/contract_invoke_api.md) — Available host calls
- [ZK Circuit Troubleshooting](dev/zk-circuit-troubleshooting.md)
- [Rust-WASM Interaction](dev/rust-wasm-interaction.md)
