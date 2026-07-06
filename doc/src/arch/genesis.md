# Genesis

Nine contracts are deployed at genesis, each at a deterministic ContractId. This
page is the **single source of truth** for the genesis contract set. Every other
document that references genesis contracts links here rather than repeating the list.

## Contract List

| Counter | Name | Crate | Consensus | Role |
|---------|------|-------|-----------|------|
| 2 | **Deployooor** | `dwow_deployooor_contract` | Yes (infrastructure) | WASM contract deployment, singleton enforcement, manifest storage |
| 3 | **Promissory Note** | `dwow_promissory_note_contract` | No | Universal DeFi primitive — tokens, transfers, swaps, redemption |
| 4 | **NativeToken** | `dwow_native_token_contract` | Yes | Block rewards, fee payment, supply audit |
| 5 | **Identity** | `dwow_identity_contract` | No | Credential issuance, selective disclosure, capability proofs |
| 6 | **Oracle** | `dwow_oracle_contract` | No | External data feeds — price, randomness, attestation data |
| 7 | **Attestation** | `dwow_attestation_contract` | No | Trust verification — on-chain attestations from trusted issuers |
| 8 | **Purse** | `dwow_purse_contract` | No | Fungible capability container — hidden balances via Pedersen commitments |
| 9 | **Box** | `dwow_box_contract` | No | Capability delegation — Put/Take with linear consumption via nullifier |
| 10 | **MultiSig** | `dwow_multisig_contract` | No | Private threshold voting — N-of-M groups, zero-knowledge ballots |

## ContractId Derivation

Every genesis contract ID is derived deterministically:

```
ContractId = poseidon_hash([42, 0, counter])
```

Where `42` is the `CONTRACT_ID_PREFIX` constant and `0` is the x-coordinate
(`pallas::Base::zero()`). The x-coordinate is zero because 0 is not a valid
x-coordinate for any Pallas curve point — this means a signature can never be
produced for these IDs, preventing anyone from claiming to be the deployer of
a genesis contract.

Counter starts at 2. Counters 0 and 1 are unused. The constants are defined in
`src/sdk/src/crypto/contract_id.rs` as `lazy_static!` values.

## Consensus-Critical vs. Ecosystem

Only two contracts are **consensus-critical**: Deployooor (counter 2) and
NativeToken (counter 4). The chain cannot function without them — Deployooor
provides the deployment infrastructure that every contract depends on, and
NativeToken handles block rewards and fee payment.

The remaining seven contracts are **ecosystem infrastructure**. They are deployed
at genesis to provide canonical well-known ContractIds for composable O-Cap
primitives. Any contract can reference `PURSE_CONTRACT_ID` for balance tracking
or `MULTISIG_CONTRACT_ID` for threshold voting without worrying about
fragmentation from replica deployments. They play zero role in block validation,
fee payment, or coinbase rewards — they are genesis-deployed purely for
ecosystem convenience, not consensus necessity.

## Bootstrap Sequence

During `dwowd` startup, `init_linear()` embeds each contract's WASM binary at
compile time via `include_bytes!()` and stores it via `set_contract_data()`.
Manifests are stored under `_manifest`-suffixed keys for manifest-based
capability resolution. The full sequence is:

1. Store Deployooor WASM (infrastructure — no manifest needed)
2. Store NativeToken WASM (consensus-critical — no manifest needed)
3. Store PromissoryNote WASM + manifest
4. Store Identity WASM + manifest
5. Store Oracle WASM + manifest
6. Store Attestation WASM + manifest
7. Store Purse WASM + manifest
8. Store Box WASM + manifest
9. Store MultiSig WASM + manifest
10. Create genesis block at height 1 (zero reward — see Cumulative Supply Bootstrap)

## Adding a New Genesis Contract

When adding a new contract to genesis (counter 11 and beyond), these files must
be updated:

| File | Change |
|------|--------|
| `src/sdk/src/crypto/contract_id.rs` | Add `lazy_static!` for new ContractId, update `GENESIS_CONTRACT_IDS_BYTES` array size |
| `src/sdk/src/crypto/mod.rs` | Add new ContractId to `pub use` re-exports |
| `bin/dwowd/src/lib.rs` | Add `include_bytes!` + `set_contract_data` block in `init_linear()` |
| `bin/dwowd/src/tests/genesis.rs` | Add to `GenesisHarness::new()` |
| `contrib/docker/darkwow-testnet/Dockerfile` | Add `zkas rebuild` + WASM `cargo build` + `cp` lines |
| `Cargo.toml` | Add contract to workspace members |
| **This page** | Add row to the contract table |

That's it. No other documentation needs updating — every other page references
this one rather than repeating the list.

## Genesis Block

The genesis block at height 1 is a **zero-reward bootstrap**. It exists to instantiate the chain and provide a deterministic anchor — no coins are minted. Block 2 is the first block with a coinbase reward.

| Field | Value | Rationale |
|-------|-------|-----------|
| `height` | 1 | First block |
| `previous` | `[0u8; 32]` | No predecessor |
| `timestamp` | 0 | Deterministic marker |
| `target` | `u32::MAX` | No PoW required |
| `nonce` | 0 | Not mined |
| `total_reward` | 0 | Zero reward (see Cumulative Supply Bootstrap) |
| `coinbase` | `None` | No coinbase transaction |
| `contract_calls` | `[]` | No WASM execution at genesis |
| `anchor_tx_id` | `[0x44, 0x52, 0x4B, 0x57, 0..]` | Network magic bytes ("DRKW") binding |

The genesis block is committed via `connect_block()` with `contracts_batch = None` — WASM execution is bypassed. The block's entire role is structural: establish height 1, provide a merkle root, and carry the network identity in the anchor field.

## Cumulative Supply Bootstrap

The cumulative supply chain uses a Pedersen commitment accumulator:

```
S_H = S_{H-1} + C_H    where C_H = pedersen_commit(reward(H), blind(H))
```

This invariant is validated by `pow_reward_v1` in the NativeToken WASM contract and verified by the Lean4 proof in `SupplyChain.lean`.

### The Setter/Getter Circularity

The WASM contract that validates `S_H = S_{H-1} + C_H` is also the contract that **persists** `S_H` to storage. At genesis (height 1):

1. The host would build a coinbase with `C_1` and `S_1 = identity + C_1`
2. The WASM contract must validate `S_1 = identity + C_1` by reading `S_0 = identity` from storage
3. But `S_1` has never been written — the contract hasn't run yet
4. Validation fails, the block is rejected, `S_1` is never persisted
5. Block 2 can't read `S_1`, fails — cascade failure

This is a **setter/getter deadlock**: you need `S_{H-1}` to compute `S_H`, but `S_{H-1}` only exists after a previous block's contract validated and persisted it. At height 1 there is no previous block.

### Solution: Zero-Reward Genesis

Genesis has zero reward (`C_1` = identity). Therefore:

```
S_1 = identity + identity = identity
TOTAL_SUPPLY = 0
```

The chain starts at the identity element of the Pedersen group — the natural neutral element for the additive homomorphism. Block 2 is the first real coinbase:

```
S_2 = identity + C_2 = C_2
```

The WASM contract validates `S_2 = identity + C_2` successfully (identity is always the starting point), persists `S_2`, and all subsequent blocks extend normally.

### Why Identity Works

The mass balance proof `S_H = sum_{i=1..H} C_i` holds for all H ≥ 1 with the convention that the sum over an empty set (genesis) is identity. The Lean4 inductive proof in `SupplyChain.lean` covers this:

- **Base case**: H = 0 → `S_0 = identity`, `supply_0 = 0`
- **Inductive step**: `S_H = S_{H-1} + C_H`, `supply_H = supply_{H-1} + reward(H)`
- **Corollary**: `S_H = sum_{i=1..H} C_i` for all H

### Emission Schedule

The emission schedule starts at height 2:

```
expected_reward(0) = 0              (pre-genesis)
expected_reward(1) = 0              (genesis bootstrap)
expected_reward(2) = R₀ × 2^(-0/H) (first real coinbase, ~13.84 DRKW)
```

Where `R₀ = 1,383,764,049` base units and `H = 1,051,920` blocks. The single-block offset from the theoretical emission start has no material impact on the supply schedule over millions of blocks.

### Alternatives Considered

| Approach | Outcome |
|----------|---------|
| **WASM execution at genesis** | Requires RandomX VM allocation at bootstrap, complex error handling. Rejected. |
| **Hard-coded cumulative state** | Magic numbers in code, fragile to emission schedule changes. Rejected. |
| **Zero-reward genesis** (chosen) | Clean bootstrap, identity start, no circularity. Accepted. |

## See Also

- [Formal Specification](formal-specification.md) — One-page architecture reference
- [Contract Trust Model](contract-trust-model.md) — How genesis trust tier works
- [O-Cap Model](ocap.md) — How genesis primitives compose
- [Wallet Architecture](wallet.md) — How the wallet discovers genesis contracts
- [Cumulative Supply Chain Proof](../../proofs/lean/src/DarkFi/SupplyChain.lean) — Inductive proof of the mass balance invariant
- Source: `src/sdk/src/crypto/contract_id.rs`, `bin/dwowd/src/lib.rs`, `src/sdk/src/blockchain.rs`
