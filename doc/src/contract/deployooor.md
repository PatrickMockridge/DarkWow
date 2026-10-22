# Deployooor Contract

The Deployooor contract is DarkWow's non-native smart contract deployment system.
It is the production path for deploying custom WASM smart contracts on-chain.

Deployooor is **plaintext infrastructure**: it is not a capability contract and it is
not on the ZK path. It has two endpoints, no circuits, no manifest, and no state
beyond the set of deployed-and-locked contract ids.

## Architecture

```
src/contract/deployooor/
├── src/
│   ├── entrypoint.rs          # init, metadata/exec/apply dispatch
│   ├── lib.rs                 # DeployFunction (DeployV1=0x00, LockV1=0x01)
│   ├── error.rs               # DeployError
│   ├── model.rs               # DeployUpdateV1, LockParamsV1, LockUpdateV1
│   ├── entrypoint/
│   │   ├── deploy_v1.rs       # DeployV1 logic
│   │   └── lock_v1.rs         # LockV1 logic
│   └── client/                # DeployCallBuilder, LockCallBuilder
├── tests/
├── Cargo.toml
└── README.md
```

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| `DeployV1` | 0x00 | Deploy a new WASM contract on-chain |
| `LockV1` | 0x01 | Lock the contract so its code becomes immutable |

Both functions are `is_zk: false` and are authorized by a **transaction signature**,
not a ZK proof. Each endpoint's `get_metadata` publishes exactly one signer,
`signature_pubkeys = vec![params.public_key]`; the host verifies the transaction's
Schnorr signature over the calls and proofs against that table
(`Transaction::verify_sigs`). A call carrying no signature, or a signature from any
other key, is rejected. This is the rule in
[the deploy spec](../spec/contract/deploy/scheme.md#signatures).

## DeployV1 (0x00)

Deploys a WASM contract using `DeployParamsV1`:

```rust
struct DeployParamsV1 {
    wasm_bincode: Vec<u8>,    // The WASM binary
    public_key: PublicKey,    // Deployer's public key; also the signature key
    ix: Vec<u8>,              // The new contract's `__initialize` payload
}
```

The `ContractId` is derived from the deployer's public key via Poseidon hash:
`ContractId = derive_public(public_key)`. `ix` is passed verbatim to the new
contract's `__initialize`; Deployooor assigns it no other meaning.

`DeployV1` checks that the contract is not locked, runs `wasmparser::validate` on the
module, and requires the five exports `memory`, `__initialize`, `__entrypoint`,
`__update` and `__metadata`. On success it returns `DeployUpdateV1 { contract_id }`,
and `apply` writes `lock[contract_id] = false` (deployed, mutable).

Deployments are detected by `dwowd` during block application via the
`apply_block_with_uncles()` post-processing hook, which scans for `DeployV1`
calls and registers the WASM binary in the contract store. This is the same
path used in production; Level 1 lightweight tests deploy contracts through
Deployooor to validate the real deployment flow (WASM exports, lock status,
ContractId derivation).

## LockV1 (0x01)

Once locked, a contract's WASM code becomes **permanently immutable** — no further
code updates can be deployed. The lock state is stored in the `lock` sled tree.

**Who can lock:** Only the original deployer. `LockV1` publishes the deployer's
`params.public_key` as its signature key, so the transaction must be signed by the
secret behind that key. No other key can lock the contract.

**Irrevocable:** Locking is a one-way transition. The lock tree entry goes from
`false` (deployed, mutable) → `true` (locked, immutable). Once locked:
- `DeployV1` is rejected with `DeployError::ContractLocked`
- A second `LockV1` is rejected with `DeployError::ContractLocked`

**Params:** `LockParamsV1` carries only `public_key: PublicKey`. `LockV1` checks
that the contract id exists in the `lock` tree and is not already locked, then
returns `LockUpdateV1 { contract_id }`; `apply` writes `lock[contract_id] = true`.

**Wallet CLI:**
```
dwow_wallet contract lock <deploy_auth>
```
Where `<deploy_auth>` is the hex-encoded secret key of the original deployer.
The wallet derives the public key, builds a LockV1 transaction targeting the
Deployooor contract, signs the lock call's signature row with that key, attaches a
Native Token fee, and broadcasts via P2P.

## ZK Circuits

None. Deployooor has no proof directory, no `manifest.toml`, and no circuits: it is
not a capability contract, and both endpoints are authorized by the deploy key's
transaction signature.

## Genesis

Deployooor is one of the nine genesis contracts and is deployed by the genesis block
at its well-known `ContractId`. Its genesis entry passes `&[]` as `ix`: like
`native_token`, it is a bespoke citizen whose wallet paths are embedded, so a
deployed manifest would be a second declaration nothing reads. The other seven
genesis contracts deploy their `manifest.toml` in `ix`, which the host writes under
`contract_id ‖ b"_manifest"` (`apply_genesis_deployments`).

The genesis deployment's `public_key` is a fixed deterministic value because it is
part of the transaction preimage; binding is by the canonical table position and the
pinned genesis hash, not by `derive_public`.

## Database Trees

| Tree | Purpose |
|------|---------|
| `info` | Contract version (`db_version`) |
| `lock` | Per-contract lock flags, `contract_id → bool` |

## Integration with Wallet

The `dwow_wallet` wallet integrates with Deployooor via:
- `apply_tx_deploy_data()` — scanning for new deployments
- `deploy_contract()` — creating new deployment transactions, signed by the deploy key
- `lock_contract()` — creating the lock transaction, signed by the deploy key

Contract deployment requires fee payment via NativeToken::FeeV3.

## See Also
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis
- [Deployooor Spec](../spec/contract/deploy/deploy.md) — Full specification
- [Contract Development Guide](../dev/contracts.md)
- [Testing Overview](../dev/testing/overview.md) — Deployooor-based Level 1 tests
- [Developer Quick Start](../dev/quickstart.md)
