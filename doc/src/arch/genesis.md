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

The genesis block at height 1 SHALL be structurally identical to every subsequent
block. It SHALL carry a PoWRewardV1 coinbase with a full ZK proof, coin commitment,
nullifier, value commitment, token commitment, and encrypted note. The nullifier
`nf = poseidon_hash(sk_H, C)` is the block's validity proof — the same
nullifier-based signing model specified in [Consensus & Coinbase](consensus-coinbase.md).

| Field | Value | Rationale |
|-------|-------|-----------|
| `height` | 1 | First block |
| `previous` | `[0u8; 32]` | No predecessor |
| `timestamp` | 0 | Deterministic marker — identical across all nodes |
| `target` | `u32::MAX` | Any hash passes — no PoW required for genesis |
| `nonce` | 0 | Not mined |
| `total_reward` | `expected_reward(1)` = `INITIAL_REWARD` | ~13.84 DRKW — full coinbase reward |
| `coinbase` | `CoinbaseTransaction` | ZK Mint_V1 proof, coin C_1, nullifier nf_1, encrypted note |
| `contract_calls` | `[PoWRewardV1]` at `transactions[0].contract_calls[0]` | Function code 0x05 — same as every block |
| `coin_merkle_root` | Merkle root after C_1 | Coin commitment tree after genesis coin |
| `nullifier_root` | SMT root after nf_1 | Nullifier SMT after genesis nullifier |
| `anchor_tx_id` | `[0x44, 0x52, 0x4B, 0x57, 0..]` | Network magic bytes ("DRKW") binding |

The genesis block SHALL be committed through the standard block acceptance path
(`accept_block`), which executes WASM (`pow_reward_v1`), reads cumulative supply
from the execution overlay, and commits block + contracts + supply_chain atomically.
The genesis block SHALL NOT bypass WASM execution.

### Genesis Miner Identity

The genesis block's coinbase nullifier `nf_1` is computed from the per-block
derived key `sk_1 = derive_instance(sk_genesis, NATIVE_TOKEN_CONTRACT_ID, 1.to_le_bytes())`.
The genesis miner identity `sk_genesis` is the well-known key declared in the
node's `keys.toml` under the section that creates genesis (typically `[node0]`).
The `init_genesis()` function in `bin/dwowd/src/lib.rs` reads this key from the
configured AccountManager and derives `sk_1` deterministically.

Any node configured with the same `[node0]` secret will produce an identical
genesis block. Nodes joining an existing network verify the genesis hash against
their local genesis block — the genesis miner identity is NOT a consensus rule,
it is a local configuration choice. The network's genesis is identified by its
block hash, not by the miner who created it.

## Cumulative Supply Bootstrap

The cumulative supply chain uses a Pedersen commitment accumulator:

```
S_H = S_{H-1} + C_H    where C_H = pedersen_commit(reward(H), blind(H))
```

This invariant is validated by `pow_reward_v1` in the NativeToken WASM contract
and verified by the Lean4 proof in `SupplyChain.lean`.

### Genesis Bootstrap

At height 1, no previous cumulative state exists in storage. The WASM entrypoint
handles this gracefully:

```rust
// Missing keys default to identity/zero:
let current_supply = db_get(info_db, TOTAL_SUPPLY)?
    .unwrap_or(0);                                  // missing → 0
let old_cumulative = db_get(info_db, CUMULATIVE_VALUE_COMMIT)?
    .unwrap_or(pallas::Point::identity());          // missing → identity
let old_blind = db_get(info_db, CUMULATIVE_BLIND)?
    .unwrap_or(pallas::Scalar::zero());             // missing → zero

// Bootstrap guard: skip blind check when no prior state exists
if current_supply > 0 && pr.old_cumulative_blind != old_blind {
    // Blind validation — only enforced after genesis
}
```

At genesis `current_supply == 0`, so the blind check is skipped. The first
coinbase proceeds normally:

```
S_1 = identity + C_1      where C_1 commits to INITIAL_REWARD
TOTAL_SUPPLY = INITIAL_REWARD
```

The keys `TOTAL_SUPPLY`, `CUMULATIVE_VALUE_COMMIT`, and `CUMULATIVE_BLIND` are
written to the WASM info tree by `apply_pow_reward()` during genesis execution.
All subsequent blocks read these values normally — no further special cases.

### Why `unwrap_or(identity)` Works

The mass balance proof `S_H = sum_{i=1..H} C_i` holds for all H ≥ 1 with the
convention that the sum over an empty set is identity. This is the same inductive
proof in `SupplyChain.lean`:

- **Base case**: H = 0 → `S_0 = identity`, `supply_0 = 0`
- **Step H = 1**: `S_1 = identity + C_1`, `supply_1 = 0 + INITIAL_REWARD`
- **Inductive step**: `S_H = S_{H-1} + C_H`, `supply_H = supply_{H-1} + reward(H)`
- **Corollary**: `S_H = sum_{i=1..H} C_i` for all H

No special bootstrap case. No setter/getter circularity. The `unwrap_or(identity)`
pattern at the WASM layer resolves what genesis.md previously described as a
circularity — the code already handles missing keys gracefully.

### Emission Schedule

```
expected_reward(0) = 0              (pre-genesis)
expected_reward(1) = INITIAL_REWARD (genesis coinbase, ~13.84 DRKW)
expected_reward(2+) = decay formula (continuous exponential decay from R₀)
```

Where `R₀ = 1,383,764,049` base units and `H = 1,051,920` blocks. The emission
schedule starts at height 1 — genesis is the first point on the decay curve,
not a zero-reward preamble.

### Why Full-Reward Genesis

| Property | Zero-Reward (old) | Full-Reward (chosen) |
|----------|-------------------|---------------------|
| Genesis nullifier | Absent | Present — nf_1 proves miner controls sk_H |
| Block construction path | Special case (bypasses WASM) | Same path as all blocks |
| Type system coherence | Genesis violates nullifier non-zero rule | Genesis compliant with all type rules |
| Wallet scan | No coinbase to decrypt at height 1 | Wallet decrypts genesis coinbase normally |
| Supply audit | Bootstrap special case (heights 1-2) | Clean cumulative supply from block 1 |
| Inductive proof | H=0 base case, H=2 first real block | H=0 base case, H=1 first block — identical structure |

## See Also

- [Formal Specification](formal-specification.md) — One-page architecture reference
- [Contract Trust Model](contract-trust-model.md) — How genesis trust tier works
- [O-Cap Model](ocap.md) — How genesis primitives compose
- [Wallet Architecture](wallet.md) — How the wallet discovers genesis contracts
- [Cumulative Supply Chain Proof](../../proofs/lean/src/DarkFi/SupplyChain.lean) — Inductive proof of the mass balance invariant
- Source: `src/sdk/src/crypto/contract_id.rs`, `bin/dwowd/src/lib.rs`, `src/sdk/src/blockchain.rs`
