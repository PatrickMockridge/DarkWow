# Deployooor Contract

The Deployooor contract is DarkWow's non-native smart contract deployment system.
It is the production path for deploying custom WASM smart contracts on-chain.

## Architecture

```
src/contract/deployooor/
├── src/
│   ├── entrypoint.rs      # init, process_instruction, apply, get_metadata
│   ├── lib.rs             # DeployFunction enum (DeployV1=0x00, LockV1=0x01)
│   ├── error.rs           # DeployError
│   ├── model.rs           # DeployParamsV1, DeployUpdateV1, LockUpdateV1
│   ├── deploy_v1.rs       # DeployV1 entrypoint logic
│   └── lock_v1.rs         # LockV1 entrypoint logic
├── tests/
├── Cargo.toml
└── README.md
```

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| `DeployV1` | 0x00 | Deploy a new WASM contract on-chain |
| `LockV1` | 0x01 | Lock the contract so its code becomes immutable |

## DeployV1 (0x00)

Deploys a WASM contract using `DeployParamsV1`:

```rust
struct DeployParamsV1 {
    wasm_bincode: Vec<u8>,    // The WASM binary
    public_key: PublicKey,    // Deployer's public key
    ix: Vec<u8>,              // Deployment payload (init params or ContractMetadata)
}
```

The `ContractId` is derived from the deployer's public key via Poseidon hash:
`ContractId = derive_public(public_key)`.

Deployments are detected by `dwowd` during block application via the
`apply_block_with_uncles()` post-processing hook, which scans for `DeployV1`
calls and registers the WASM binary in the contract store. This is the same
path used in production; Level 1 lightweight tests deploy contracts through
Deployooor to validate the real deployment flow (WASM exports, lock status,
ContractId derivation).

## LockV1 (0x01)

Once locked, a contract's WASM code becomes permanently immutable — no further
code updates can be deployed. The lock state is stored in the `lock` database
tree.

## No ZK Circuits

Deployooor is a system contract and does not use ZK proofs. Deployment
authorization is handled by signature verification at the consensus layer.

## Database Trees

| Tree | Purpose |
|------|---------|
| `info` | Contract version and state |
| `lock` | Per-contract lock flags |

## Integration with Wallet

The `dww` wallet integrates with Deployooor via:
- `apply_tx_deploy_data()` — scanning for new deployments
- `deploy_contract()` — creating new deployment transactions

Contract deployment requires fee payment via NativeToken::FeeV1.

## See Also

- [Contract Metadata](../arch/contract-metadata.md) — On-chain metadata carried in `ix`
- [Deployooor Spec](../spec/contract/deploy/deploy.md) — Full specification
- [Contract Development Guide](../dev/contracts.md)
- [Testing Overview](../dev/testing/overview.md) — Deployooor-based Level 1 tests
- [Developer Quick Start](../dev/quickstart.md)
