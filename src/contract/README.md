This directory contains the native WASM contracts on DarkWow.

## What is here

`src/contract/` holds **34 directories**: 32 contracts, the `entropy` library (rlib-only, not a
deployable contract) and `test-harness` (test infrastructure). Nine contracts are genesis:
`native_token`, `deployooor`, `promissory_note`, `box`, `purse`, `identity`, `oracle`,
`attestation` and `multisig`. Thirty-one carry a `manifest.toml`; `deployooor` is deliberate
plaintext infrastructure with none.

The canonical catalog, maturity labels and per-contract documentation links are in
[doc/src/contracts.md](../../doc/src/contracts.md). Every contract directory has its own
`README.md`; where a README and a contract page disagree, the code and the page under
`doc/src/contract/` are authoritative.

**Status is not uniform.** Some arms are deliberately refused while their authority or custody
model is absent: the stablecoin's custody arms (`CustodyModelUnavailable`), bearer_bond's
issuer-authority arms, drain_protection's member exit, and bridge's deposit in the default build.
Check the contract page and the manifest before assuming an endpoint is live.

**Randomness**: see [provable_randomness.md](../../doc/src/contract/provable_randomness.md).

## Promissory Note (the standard)

`promissory_note` is the standard DeFi token contract — Poseidon-only, no EC operations, fungible
with hidden token ids. `native_token` is the consensus-native token for PoW rewards and fees. The
upstream `money` and `money_v2` contracts were removed; [MIGRATION.md](MIGRATION.md) records the
migration. See [promissory_note/README.md](promissory_note/README.md).

---

## ZKas circuit reference

The per-circuit and per-opcode analysis that used to fill this section was a hand-maintained
snapshot and had drifted: it named `dao/exec.zk`, `create_claim.zk` and an `assert_lte_u64.zk`
safemath gadget that do not exist, and it declared bridge `deposit.zk` both "fake" and "fixed".
The current references are:

- [Opcodes and Formal Verification](../../doc/src/arch/zk/opcodes.md) — opcode table and proof status
- [Opcode Universe](../../doc/src/arch/zk/opcode_universe.md)
- [zkVM Primitive Layer](../../doc/src/arch/zk/zkvm_primitives.md)
- `scripts/check-circuit-instance-derivation.sh` — the Orchard-class gate (177 sources, 0 unclassified)

Do not restore a per-contract circuit table here; the gate's output is the census.

## Token Design Philosophy

DarkWow's token contracts (PromissoryNote, NativeToken) follow a **minimal infrastructure** philosophy:

> **Tokens are pipework, not reactors.**
>
> - Tokens move value. That's their job.
> - Business logic lives in smart contracts (DEX, stablecoin, etc.)
> - Tokens are the most frequently called contracts - simplicity minimizes attack surface
> - A bug in a token cascades to every operation; a bug in a smart contract is isolated

**Practical implications:**
- NativeToken: Minimal viable circuits for consensus (fees, rewards). One job, do it well.
- PromissoryNote: Poseidon-only design for DeFi tokens. Zero EC operations in ZK = zero heap bugs.

### PromissoryNote Design Principles

PromissoryNote is the cornerstone of DarkWow's DeFi ecosystem:

1. **Poseidon-only ZK circuits**: All cryptographic operations in ZK use Poseidon hash. No EC operations in ZK circuits.

2. **EC operations pushed to smart contracts**: Pedersen commitments and other EC operations happen in the contract verification layer, not in ZK circuits. This keeps ZK circuits simple and auditable.

3. **Privacy-first token model**:
   - Commitment = `poseidon_hash(pub, value, asset_id, spend_hook, user_data, blind)`
   - Nullifier = `poseidon_hash(secret, commitment)` or `poseidon_hash(secret, asset_id)` for auth
   - Value commitment = `poseidon_hash(value, blind)`

4. **Token authorization via backing proof**: MintV1 proves knowledge of the backing secret directly against the stored token_auth_parent commitment in a single step.

5. **Function IDs** (`src/contract/promissory_note/manifest.toml`):
   - `0x00` - `register_type`: Register a token type
   - `0x01` - `redeem`: Burn a note and produce a zero-value receipt
   - `0x02` - `issue`: Issue notes against a registered type
   - `0x03` - `revoke`: Consume a note via its nullifier
   - `0x04` - `transfer`: Private token transfer
   - `0x05` - `otc_swap`: Atomic OTC token swap

This separation ensures ZK circuits remain minimal while complex business logic lives in audited smart contracts.

## Testing Infrastructure

Two pipelines for contract testing at different levels:

### Lightweight Pipeline (`bin/dwowd/src/tests/pipeline.rs`)

For deployment verification without ZK proof generation. Fast CI/CD checks.

```bash
# Test any contract by name
cargo test --package dwowd test_pipeline
CONTRACT_NAME=promissory_note cargo test --package dwowd test_pipeline
CONTRACT_NAME=stablecoin cargo test --package dwowd test_pipeline
```

### Heavyweight Pipeline (`bin/dwowd/src/tests/heavyweight_pipeline.rs`)

For full contract execution testing with real ZK proofs. Uses `ContractHarness` trait for generic ZK circuit access.

```bash
# Requires release mode or increased stack size due to halo2's computational intensity
cargo test --package dwowd --release test_heavyweight_dex
cargo test --package dwowd --release test_heavyweight_promissory_note

# Alternative: increase stack size (the repository default is 64 MiB)
export RUST_MIN_STACK=67108864
cargo test --package dwowd test_heavyweight_dex
```

**Why the stack limit?** halo2's polynomial arithmetic uses deep recursion, and building
several proving keys (DEX has 8 circuits, PromissoryNote 5) exceeds the default stack.

See [Contract Deployment Pipeline](../../doc/src/arch/dwowd_contract_pipeline.md) for full details.

### ContractHarness Trait

All test harnesses implement `ContractHarness` for generic ZK circuit access:

```rust
pub trait ContractHarness {
    fn name(&self) -> &str;
    fn circuits(&self) -> Vec<&'static str>;
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary>;
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey>;
}
```

Implemented by `BoxHarness` and `PurseHarness` (`test-harness/src/harness/{box,purse}.rs`).
The per-contract `*Harness` structs exist but do not implement this trait.

## Heavyweight Test Status

Heavyweight tests drive real contract endpoints through the accept path. The suite lives in
`bin/dwowd/src/tests/heavyweight_pipeline.rs` as `test_heavyweight_<contract>` (45 tests).

Run one:
```bash
cargo test --release -p dwowd test_heavyweight_stablecoin
```

Run the suite through the harness:
```bash
./bin/dwowd/src/tests/heavyweight.sh --all
```

Set `DWOW_TEST_LOGS=1` or a contract's own `msg!` output is discarded. Test status is the run's own
output, not a table here — a hand-maintained pass list goes stale the moment an arm changes.

## Official Contracts

All 32 contracts are official; there is no separate "unofficial" set. The canonical list with
maturity labels and crate names is [doc/src/contracts.md](../../doc/src/contracts.md). Nine are
genesis (listed under "What is here"). Thirty-one directories carry a `manifest.toml`
(`deployooor` does not); every contract directory has a `README.md`.

Architecture and implementation pages live under `doc/src/contract/` and are indexed by
[SUMMARY.md](../../doc/src/SUMMARY.md). The hand-written link list that used to live here pointed at
`doc/src/arch/` pages that do not exist; do not restore it — the catalog and `SUMMARY.md` are the
two indices.
