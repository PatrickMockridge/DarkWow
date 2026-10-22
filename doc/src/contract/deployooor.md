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
    singleton: bool,          // Reject if another contract claims `singleton_name`
    singleton_name: String,   // Logical name for the singleton check
}
```

`singleton` and `singleton_name` carry the genesis singleton rule — see the `singleton`
tree under [Database Trees](#database-trees) below, and [Genesis](../arch/genesis.md).

The `ContractId` is derived from the deployer's public key via Poseidon hash:
`ContractId = derive_public(public_key)`.

Deployments are detected by `dwowd` during block application via the
`apply_block_with_uncles()` post-processing hook, which scans for `DeployV1`
calls and registers the WASM binary in the contract store. This is the same
path used in production; Level 1 lightweight tests deploy contracts through
Deployooor to validate the real deployment flow (WASM exports, lock status,
ContractId derivation).

## LockV1 (0x01)

Once locked, a contract's WASM code becomes **permanently immutable** — no further
code updates can be deployed. The lock state is stored in the `lock` sled tree.

**Who can lock:** Only the original deployer, and it is a proof that enforces this. `LockV1`
carries a `LockV2` ZK proof that the caller knows the secret behind `params.public_key`: the
circuit derives the public key from the witnessed secret with `ec_mul_base` and constrains it
equal to the pair the call publishes, so a prover can only produce it for a key it holds. The
proof is bound to the transaction carrying it — it commits to `tx_commitment` and to a
per-call `tx_nonce` — so one cannot be lifted from another transaction and replayed here.

**Irrevocable:** Locking is a one-way transition. The lock tree entry goes from
`false` (deployed, mutable) → `true` (locked, immutable). Once locked:
- `DeployV1` is rejected with `DeployError::ContractLocked`
- A second `LockV1` is rejected with `DeployError::ContractLocked`

**Params:** `LockParamsV1` carries `public_key: PublicKey` and `tx_nonce: pallas::Base`
(64 bytes encoded). `build_lock_call` in `client/lock_v1.rs` builds the witnesses from the
deployer's `Keypair` and secret and returns a `LockPlan`; `LockPlan::prove(tx_commitment)`
produces the proof, because the frame commitment is not known until the call exists.

**Wallet CLI:**
```
dwow_wallet contract lock <deploy_auth>
```
Where `<deploy_auth>` is the hex-encoded secret key of the original deployer.
The wallet derives the public key, builds a LockV1 transaction targeting the
Deployooor contract, attaches a Native Token fee, and broadcasts via P2P.

## ZK Circuits

| Circuit | Used by | What it proves |
|---------|---------|----------------|
| `lock.zk` | `LockV1` (0x01) | The caller knows the secret behind `params.public_key`, and the proof belongs to the transaction carrying it |

`DeployV1` (0x00) carries no proof (`manifest.toml` declares `requires_proof = false`).
Circuits are registered by the contract's `init_contract` into the `_zkas` tree, which is
where the host reads them back from by namespace.

## The WASM content commitment

`DeployV1` stores `wasm_content_commitment(wasm)` under `info ‖ b"wasm_hash" ‖ contract_id`. The
commitment folds 64-byte chunks of the module with `poseidon`, each with its length, and folds the
module's total length last; before 2026-10-22 it was `poseidon([0, len])` — a commitment to the
module's **size**, so two equal-length modules stored the same value. The deployooor spec now reads
the stored value back and compares it with the module it deployed: that read is what makes the field
an integrity check rather than a number nothing consumed (`OBL-C250`).

The genesis deployment table carries a manifest in `ix` for each of the nine contracts and the host
writes it under `contract_id ‖ b"_manifest"` (`apply_genesis_deployments`). `native_token` and
`deployooor` were the two empty entries until their manifests existed; they now carry theirs.

## Database Trees

| Tree | Purpose |
|------|---------|
| `info` | Contract version and state |
| `lock` | Per-contract lock flags |
| `singleton` | Singleton enforcement — prevents duplicate deployments |

`singleton` is real — `DEPLOY_CONTRACT_SINGLETON_TREE = "singleton"` in `lib.rs` — but
**`manifest.toml` declares only `info` and `lock`** (`F-543`). No gate reads a manifest's
`[[trees]]` block, so nothing notices either way; the disagreement is recorded rather than
repaired here, because a contract's `manifest.toml` is a build input and deployooor is a genesis
contract, so editing it stales the artifact and re-rolls the pin.

## Integration with Wallet

The `dwow_wallet` wallet integrates with Deployooor via:
- `apply_tx_deploy_data()` — scanning for new deployments
- `deploy_contract()` — creating new deployment transactions

Contract deployment requires fee payment via NativeToken::FeeV3.

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis


- [Contract Metadata](../arch/contract-metadata.md) — On-chain metadata carried in `ix`
- [Deployooor Spec](../spec/contract/deploy/deploy.md) — Full specification
- [Contract Development Guide](../dev/contracts.md)
- [Testing Overview](../dev/testing/overview.md) — Deployooor-based Level 1 tests
- [Developer Quick Start](../dev/quickstart.md)
