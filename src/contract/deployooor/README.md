# Deployooor — Contract Deployment Infrastructure

## What It Is

Deployooor is the deployment factory: it validates WASM binaries, deploys them at
a key-derived ContractId, and locks deployed contracts immutably. It is **pure WASM
with no ZK circuits and no manifest** — `deploy` and `lock` are plaintext contract
calls authorized by the deployer's transaction signature. It is infrastructure, not
a capability contract (`wallet.md` §0.1).

**Trust tier:** consensus-critical (genesis counter 2). Required for every
user-deployed contract.

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `deploy` | — (native) | Validate WASM binary + exports, deploy at key-derived ContractId |
| `0x01` | `lock` | — (native) | Lock a deployed contract immutable (requires deployer's key) |

## ContractId Derivation

A user-deployed contract lives at the key-derived id (NOT a well-known genesis id):

```
contract_id = ContractId::derive_public(public_key)
            = poseidon_hash([CONTRACT_ID_PREFIX, pk_x, pk_y])     # CONTRACT_ID_PREFIX = 42
```

Genesis contracts are deployed at their **well-known** ids
`poseidon_hash([42, 0, counter])` by `apply_genesis_deployments`, not through this
`derive_public` path. Deployooor's own id is `DEPLOYOOOR_CONTRACT_ID = poseidon_hash([42, 0, 2])`.

## Deploy Flow (`deploy`)

Exec (`deploy_process_instruction_v1`) validates, in order:

1. **Lock check** — reject if `lock[contract_id]` is set.
2. **WASM validation** — `wasmparser::validate(wasm_bincode)`.
3. **Export scan** — requires `memory`, `__initialize`, `__entrypoint`, `__update`
   and `__metadata`.

Apply writes `lock[contract_id] = false` (deployed, mutable). The runtime then
materializes the WASM and calls `__initialize` with the deploy `ix` verbatim.

## Lock Flow (`lock`)

Exec requires the contract to exist and be unlocked; Apply sets
`lock[contract_id] = true`, making it immutable.

## State Trees

| Tree | Purpose |
|------|---------|
| `info` | Contract version (`db_version`) |
| `lock` | Contract lock status — `contract_id → locked(bool)` |

## Authorization

There is no ZK proof and no manifest. Each endpoint's `get_metadata` publishes
`signature_pubkeys = vec![params.public_key]`; the host verifies the transaction's
Schnorr signature over the calls and proofs against that table
(`Transaction::verify_sigs`). `contract_id = derive_public(deploy_pk)` means only the
holder of that key can redeploy or lock the contract; an unsigned call or one signed
by any other key is rejected.

## References

- [Deployooor Specification](../../../doc/src/contract/deployooor.md)
- [Contract Deployment Pipeline](../../../doc/src/arch/dwowd_contract_pipeline.md)
- [Genesis Contracts](../../../doc/src/arch/genesis.md)
- Source: `src/contract/deployooor/`
