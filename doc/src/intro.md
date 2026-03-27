# DarkFi Development Fork

This is a development fork containing work in progress that may or may not be merged into the official DarkFi repository.

## The `dev` Branch

This fork's `dev` branch (`PatrickM123/darkfi:dev`) contains all additions and changes compared to official DarkFi master. It includes:

- New smart contracts not in official DarkFi (Bridge, DEX, Identity, Stablecoin)
- Expanded architecture documentation
- Alternative design approaches and analysis
- Work-in-progress implementations

The official DarkFi master branch (`darkrenaissance/darkfi:master`) should be consulted for the canonical state of the project.

## Contents

This fork includes:

### Smart Contracts
- **Bridge Contract**: Cross-chain asset transfers with Object Capability Security
- **DEX Contract**: Atomic swap DAO with incremental transparency roadmap
- **Identity Contract**: Minimal credential proofs using ZK-verified competency DAGs
- **Stablecoin Contract**: Monero-collateralized stablecoin with P2P Oracle design
- **Escrow Contract**: Hashed Timelock with public key variant, trustless conditional payments
- **DAO-Escrow Contract**: DAO-controlled endowment for community insurance with cooperative governance roadmap

### Architecture Documentation
- Honest analysis of SPV de-anonymization problem
- Incremental transparency approach (Level 0-3 privacy gradient)
- ZK-verified competency DAGs for identity
- P2P Oracle design for stablecoin price discovery
- Field arithmetic constraints and zkVM primitive analysis
- Escrow and DAO-Escrow contract documentation with cooperative governance roadmap

### Development Documentation
- Contract developer guides
- Common patterns for new contracts
- ZK circuit documentation
- Build and test instructions

## Relationship to Official Repo

This fork diverges from the official DarkFi repository. Some features here may never be merged upstream. The official documentation should be consulted for the canonical state of the project.

## Status

All contracts in this fork are **skeleton implementations** demonstrating design concepts. They have not been audited, formally verified, or tested in production. Use at your own risk.

## Building

```bash
# Build contracts
cd src/contract/<name>
make

# Build documentation
cd doc
mdbook build
```
