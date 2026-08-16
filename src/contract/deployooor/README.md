# Deployooor — Contract Deployment Infrastructure

## The Capability

Deployooor is the deployment factory: it validates WASM binaries, deploys them at
a key-derived ContractId, and locks deployed contracts immutably. It is **pure WASM
with no ZK circuits** — `deploy` and `lock` are native contract calls authorized by
the deployer's transaction signature. It is the second sanctioned citizen
(`wallet.md` §0.1).

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
3. **Export scan** — requires `memory`, `__initialize`, `__entrypoint`, `__update`,
   `__metadata` (and optional `__spend_hook`).
4. **Import scan** — reject `db_clear_all`, `db_drop_tree`, `db_drop_all`, `exec_dangerous`.
5. **Singleton enforcement** — if `singleton` is set, the `singleton_name` must not
   already be claimed.
6. **wasm_hash** — `poseidon_hash([0, wasm_bincode.len()])`.

Apply writes the lock marker (`lock[contract_id] = 0`) and the wasm hash into `info`.
The runtime then materializes the WASM and calls `__initialize` with the deploy `ix`.

## Lock Flow (`lock`)

Exec requires the contract to exist and be unlocked; Apply sets
`lock[contract_id] = 1`, making it immutable. Only a transaction signed by the deploy
key (whose public key derives the `contract_id`) can lock it.

## State Trees

| Tree | Purpose |
|------|---------|
| `info` | Contract metadata — `db_version`, wasm hashes |
| `lock` | Contract lock status — `contract_id → locked(bool)` |
| `singleton` | Singleton name → ContractId (lazy, checked on deploy) |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `deployment_right` | `0` | `SecretKey, Commitment, ContractId, FuncId` | — (consumable) |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `deploy` | `all(deployment_right)` | `deployment_right` | — | `Commit, Dispatch, Gate` |
| `lock` | `all(deployment_right)` | `deployment_right` | — | `Commit, Dispatch, Gate` |

## Authorization

There are no ZK circuits and no contract-level signature checks — the `deployment_right`
is modeled in the manifest, but the effective authorization is the **transaction-layer
signature** bound to the deploy key. `contract_id = derive_public(deploy_pk)` means only
the holder of that key can redeploy or lock the contract. `required_barbs` here are
framework-level transaction barbs (commit/dispatch/gate), not contract circuits.

## References

- [Deployooor Specification](../../../doc/src/contract/deployooor.md)
- [Contract Deployment Pipeline](../../../doc/src/arch/dwowd_contract_pipeline.md)
- [Contract Manifest](../../../doc/src/arch/manifest.md)
- [Genesis Contracts](../../../doc/src/arch/genesis.md)
- Source: `src/contract/deployooor/`
