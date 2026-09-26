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

**The nine results, so the table above is usable without a Rust build.** Each string is the base58
rendering of the derived field element, and `bin/dwowd/src/tests/genesis_contract_ids.rs` pins all
nine: a change to the prefix, a counter or the hash fails that test rather than shipping silently.
("Regenerate them with the crate and pin them" was the external report's finding 7 — the derivation
was documented and its *output* was not.)

| Counter | Contract | ContractId (base58) |
|---|---|---|
| 2 | Deployooor | `EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN` |
| 3 | Promissory Note | `21LYoifepcySKhyDA1vzxRDWGHyDizPQ8f11zSqhep7t` |
| 4 | NativeToken | `DgmXpuU1EcM54E8GuNTAkBUThcCoYzGN5kRCNXA4cPtw` |
| 5 | Identity | `AyJtw5sxYrKBkeec73hxLDUPh6ZY32gXRagWcZ3hctBA` |
| 6 | Oracle | `DkrPpNQERff36c7B7qhryCfpnzUjGKcYipbpegKxVYnr` |
| 7 | Attestation | `5sKmJNgZCjJ2sFjxzpwS9R1sLyrNc9DZ6gL1KcDHihfe` |
| 8 | Purse | `8v6z9CTT7ed8fDdyY9iNBL9DY8co5GqMyNT7azQcFZPB` |
| 9 | Box | `2afzyKdAkNu9tVZe7aug7xPBcgjGkGZRC3bH6doDWEy7` |
| 10 | MultiSig | `G7ZRpbi8AQeU38JGYehRRyirPpcM3WXqSZGxXCQVLYpn` |

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

The nine genesis contracts are **not** stored by `init_linear()`. Their WASM binaries
(embedded at compile time via `include_bytes!()`) and manifests ride **inside the genesis
block** as deployment transactions. The deployment transactions are built by
`build_genesis_deployment_txs()` (`bin/dwowd/src/lib.rs`) and materialized during
genesis-block execution by `apply_genesis_deployments()`
(`src/linear/src/execution.rs`). Each deployment is a call to the Deployooor contract
carrying a `DeployParamsV1` payload; the genesis-deployment rule deploys the WASM at the
well-known ContractId and invokes the contract's `__initialize` entrypoint with empty init
params. Manifests are stored under `_manifest`-suffixed keys for manifest-based capability
resolution.

The full sequence is:

1. `init_linear()` constructs the genesis block when `create_genesis = true`.
2. `init_genesis()` builds the PoWRewardV1 coinbase (transaction 0) and appends the nine
   deployment transactions at positions 1..=9, in the order: Deployooor, NativeToken,
   PromissoryNote, Identity, Oracle, Attestation, Purse, Box, MultiSig.
   **These positions are not the counters in the table above, and the two must not be indexed by
   each other**: NativeToken has counter 4 but is deployed at position 2, and Promissory Note has counter
   3 at position 3. `genesis_contracts()` (`src/linear/src/execution.rs:915-927`) is the authority for
   the order and `contract_id.rs`'s constants are the authority for the ids; the ordering trap is
   recorded in that file's own comment (`:139-149`), where `GENESIS_CONTRACT_IDS_BYTES` had to be
   reordered to match the deployment array. Deployooor and
   NativeToken are deployed with EMPTY manifest bytes by design — the wallet handles
   those two natively (Path 1, wallet.md §6.4) rather than through manifest-declared
   capability discovery, so deploying their manifests would make the wallet scan them
   twice. The seven ecosystem contracts carry their `manifest.toml`. Both Deployooor
   and NativeToken do have a `manifest.toml` in-tree; those are interface documentation,
   not consensus artefacts, and are not deployed.
3. The genesis block is committed through the standard acceptance path (`accept_block`),
   which executes WASM — the deployment rule materializes each contract and calls
   `__initialize` (empty init params), and `pow_reward_v1` writes the cumulative supply
   bootstrap state.
4. The genesis block at height 1 carries a full `INITIAL_REWARD` coinbase (see
   Cumulative Supply Bootstrap).

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

The genesis block at height 1 SHALL obey the same structural rules as every
subsequent block. Structural identity is defined by the block validator
(`validate_block_structure`), not by byte counts or transaction counts.
It SHALL carry a PoWRewardV1 coinbase (plaintext — no ZK proof): the reward value is
in the clear (`effective_value`, `total_pin`), and the spendable note's value is bound
to consensus by the plaintext note preimage, so there is no concealed amount anywhere in
the coinbase. The coin commitment, nullifier, value commitment and token commitment are
verified by the WASM entrypoint in plaintext Pedersen/poseidon arithmetic
([Consensus & Coinbase](consensus-coinbase.md) §2.5). The `Output` the call carries does
still contain an AEAD-encrypted note *record*; that ciphertext is built with a derived ephemeral key
(`encrypt_deterministic`, §2.7) and never a random one. What it hides and what it does not is stated
normatively in [privacy-model.md](privacy-model.md) §2.1 "The AEAD-note caveat": it does **not** hide
the value, which rides in plaintext call data; it hides the miner's commitment blinds (`value_blind`,
derived from `sk_H`), which must stay private so the reward note behaves like any other spendable
note. The nullifier
`nf = poseidon_hash(sk_H, C)` is the block's validity proof — the same
nullifier-based signing model specified in [Consensus & Coinbase](consensus-coinbase.md).

### Genesis Is A Pure Function

**The genesis ceremony SHALL be a pure function of its inputs.** This is the requirement. The
structural rules in the table below and the identity rules that follow are its evidence, and the
pinned hash is its witness. Normatively:

1. **No ambient authority.** The ceremony SHALL NOT read a clock, a random source, the network, or any
   file whose contents are not one of its declared arguments. Its inputs are exactly: the genesis
   miner secret, the network magic bytes, and the contract WASM bytes with their manifests. Reading
   those inputs from the environment (`AccountManager::open`, a configured datadir) is plumbing that
   *supplies arguments*; it is not part of the function.
2. **Stages are pure transitions.** Where the ceremony is carved into sequential stages — build the
   coinbase, deploy the nine contracts, compute the merkle root, assemble the header, accept — each
   stage SHALL be a pure state transition, and their composition SHALL be a pure state transition. The
   genesis hash SHALL be a projection of the state those stages produce, so that the same inputs from
   the same starting state yield the same hash on every node.
3. **Embedded contract code is a quoted argument, not an effect.** The contracts' bytes arriving via
   `include_bytes!` is reflection: the code is data the function *consumes*, so it does not breach
   (1). Executing those entrypoints under `accept_block` is the corresponding evaluation step.

Three properties follow, and they are consequences rather than separate goals:

| property | why it follows from purity |
|---|---|
| **Totality** — no panic path | A panic is an effect: it aborts the computation instead of returning a value. Totality is purity's absence of the panic effect. |
| **Determinism** — same inputs, same hash | A random source or a clock is an effect, so a pure function has neither; single-valuedness is then definitional for a function, not something to be established by hashing twice. |
| **Reproducibility** — the artifact is a function of the source | Purity pushed through the compiler: nothing about *where* or *when* the source was compiled may enter the artifact. |

**Why one requirement rather than three.** Stated separately, each invites its own heuristic — "no
unwrap" as a lint campaign, "determinism" as two hashes compared at runtime, "reproducible" as a hash
that happened to match on one machine. Stated as purity, they are one property with one failure mode,
and a violation is a specific unaccounted effect rather than a test that went red somewhere.

**How the reproducibility clause is met, measured 2026-09-24.** It is not free, and it is not a
convention. A panic location is `Location { file, line, col }` — a data-section string and two
integers, none of them debuginfo — so a *comment-only* edit shifts the line numbers of every panic site
after it and moves the artifact. Two build settings remove that dependence, and together they take all
nine genesis contracts to clean under `contrib/wasm_artifact_check.sh --genesis`:

* **`[profile.release] strip = "symbols"`** in the workspace root, which drops the wasm *name* section
  — where the panic-marker strings live (`panic_bounds_check`, `rust_begin_unwind`, `core::panicking`).
  It removes those strings and nothing about the code: the Code section is byte-identical afterwards.
  A profile setting rather than a Makefile flag, deliberately — the container builds contracts with
  bare cargo, not through the Makefiles, so a flag would have to be mirrored or the container's genesis
  hash would diverge from the host's.
* **`-Zlocation-detail=none`** on the contract build, which empties the file and line of every
  `Location` built by a first-party crate. This is the real removal of the embedded `src/…` paths:
  measured on `native_token`, its three (`sdk/src/crypto/pedersen.rs`, `sdk/src/crypto/merkle_node.rs`,
  `contract/native_token/src/model/mod.rs`) go to zero, and with `strip` applied alongside it the
  artifact drops 417,500 → 373,496 bytes.

Neither is sufficient alone — `strip` leaves the paths, and the location flag leaves the marker
strings — and the combination's measured effect is the one this section requires: inserting a comment
into `sdk/src/crypto/merkle_node.rs` changes the artifact *without* the levers (`1c89c9d4…` against
`27ad85fc…`) and does **not** change it with them (`e539f88c…` either way). That retires the stated
cost of every comment-only deferred item whose price was "a comment still moves the pin".

What it does **not** buy is totality: `alloc`'s `handle_alloc_error` and `core::fmt` panic by design and
are linked into all nine, which is why four of them carry no first-party panic site and still carry
panic machinery. The artifact-level invariant is the one `contrib/wasm_artifact_check.sh` checks — no
marker strings, no embedded first-party path — and its header states what that is narrower than.

One boundary on the sentence above, because it is easy to generalise past its evidence: it is a claim
about files the contracts **link**. A comment in `src/sdk/src/**` or in a contract's own sources is
compiled into the artifact, and the levers are what stop it moving. A `src/sdk/**` *leaf crate* that no
contract depends on — the Python bindings, say — never changed the built bytes at all: an edit there
still moves **every** contract's `.source_hash`, because the manifest globs all of `src/sdk/**`, so the
rebuild obligation is real, while the artifact — and therefore the hash this document is about — is
unchanged.

The tree already had one leg of that recorded. `OBL-C61`'s note carries the measured pair from
2026-09-23: rebuilding `purse`'s artifact "from a pristine sdk and from one carrying it gives
`28819142…` and `370aa49b…`" — where the second value is an sdk *lib* edit and the first is no edit at
all. The other leg was measured on 2026-09-24 by another session: rebuilding `purse` after an edit to
`src/sdk/python/src/contract/bridge/withdraw_v1.rs`, a leaf crate no contract links, landed on
`28819142…` — the pristine value — with only `.source_hash` moving. So an sdk edit is a *staleness*
event for all 32 artifacts and a *pin* event only when the crate it touches is one the contracts
compile in.

Neither leg is a diff against a committed artifact — `*.wasm` is gitignored, so there is no HEAD copy to
compare — which makes the evidence agreeing measurements of a value rather than a repository diff. Said
here rather than left implicit, because "the artifact is identical" and "two readings of a gitignored
file agree, one of them recorded in `OBL-C61`" are different claims and only the second is true.


This is also what makes the account align with the process calculus the rest of this specification is
built on — [type-system.md](type-system.md) §0 derives the type system from the ρ-calculus, and §1
defines a type as a behavioural position whose barbs are its observable actions. The observable
consequences of a pure ceremony *are* barbs, so determinism here is observational determinism, and the
reflection step in (3) is exactly ρ's `quote`/`eval` pair. The λ-calculus core — abstraction,
application, substitution, nothing else — is what makes the requirement checkable: a stage that is a
term can be modelled as one, and a term in a dependently-typed language has no clock, no RNG and no
panic to hide behind.


|-------|-------|-----------|
| `height` | 1 | First block |
| `previous` | `[0u8; 32]` | No predecessor |
| `version` | `BlockVersion::CURRENT` | Same version byte as every block |
| `merkle_root` | computed from the genesis transactions | Same merkle rule as every block |
| `timestamp` | 0 | Deterministic marker — identical across all nodes |
| `target` | `u32::MAX` | Any hash passes — no PoW required for genesis |
| `nonce` | 0 | Not mined |
| `total_reward` | `expected_reward(1)` = `INITIAL_REWARD` | ~13.84 DRKW — full coinbase reward |
| `coinbase` | `CoinbaseTransaction` | Plaintext PoWRewardV1 (no ZK proof), coin C_1, nullifier nf_1, plus an AEAD note *record* for wallet discovery (derived key) |
| `contract_calls` | `[PoWRewardV1]` at `transactions[0].contract_calls[0]` | Function code 0x05 — same as every block |
| `uncle_merkle_root` | `[0u8; 32]` | No uncles at genesis |
| `randomx_key` | `blake3(height.to_le_bytes())` | Deterministic from height — carries no key material at genesis |
| `miner` | `[0u8; 32]` | No miner identity in the header — the coinbase binds the mining key |
| `commitment_merkle_root` | `[0u8; 32]` | Decorative at genesis (see note below) |
| `nullifier_root` | `[0u8; 32]` | Decorative at genesis (see note below) |
| `anchor_tx_id` | configured `magic_bytes` (`[0x44, 0x52, 0x4B, 0x57, ...]` = "DRKW") | Network magic bytes binding — the first 4 bytes equal the configured `magic_bytes`, not a fixed value |
| `anchor_monero_height` | `MoneroBlockHeight(0)` | No Monero anchor at genesis |
| `anchor_monero_hash` | `[0u8; 32]` | No Monero anchor at genesis |
| `finality_flags` | `0` | No finality anchor at genesis |
| `fee_window_flags` | `FeeWindowFlags::default()` | Empty fee window |
| `pow_source` | `PowSource::Native` | Not merge-mined |

> **Decorative roots.** `commitment_merkle_root` and `nullifier_root` are
> `[0u8; 32]` at genesis and are never computed or verified by the acceptor —
> every current code path (validation, chain state, wire codecs) reads and
> writes zeros. `nullifier_root`, when populated, is a blake3 root over the
> block's nullifier set, **not** an SMT (see the `BlockHeader` field docs in
> `src/linear/src/block.rs`). Both fields are carried for forward
> compatibility only.

The genesis block SHALL be committed through the standard block acceptance path
(`accept_block`), which executes WASM (`pow_reward_v1`), reads cumulative supply
from the execution overlay, and commits block + contracts + supply_chain atomically.
The genesis block SHALL NOT bypass WASM execution.

### Structural Identity — Precise Definition

"The genesis block SHALL be structurally identical to every subsequent block"
means identical in these five dimensions:

| Dimension | Rule | Enforced By | Spec Ref |
|-----------|------|-------------|----------|
| **Header format** | All `BlockHeader` fields present, same encoding, same version byte | `BlockHeader::decode()` | consensus.md |
| **Transaction ordering** | Exactly one coinbase at `transactions[0]`; FeeCollectV1 (0x06) at final position iff `total_fees > 0`; otherwise absent | `validate_block_structure()` | fee-spec.md §2.1, §4.4 |
| **Execution path** | Committed through `accept_block` with WASM execution; SHALL NOT bypass WASM | `accept_block()` | genesis.md §Genesis Block |
| **Fee lifecycle** | `fees_db[height]` seeded to 0 by `apply_pow_reward`; accumulated via FeeV3 (0x08) plaintext addition; verified (`total_fees == fees_db[height]`) and zeroed by FeeCollectV1 | `apply_pow_reward`, `apply_fee`, `apply_fee_collect` | fee-spec.md §14 |
| **Coinbase structure** | PoWRewardV1 (0x05) plaintext (no ZK proof), coin commitment, nullifier, value commitment, token commitment; the call's `Output` carries an AEAD note *record* for discovery (derived key, no randomness) | `pow_reward_v1` (WASM) | consensus-coinbase.md |

The 9 contract deployment transactions at `transactions[1..=9]` are a one-time
bootstrap event. They pass `validate_block_structure()` via `is_genesis_deployment_tx()`
checks that reject deployment transactions in non-genesis blocks. They do not
violate structural identity because structural identity concerns the RULES every
block follows, not the specific content of any block.

The genesis block is the first block accepted under these rules. It is the
"template" only in the sense that `validate_block_structure()` accepts it, and
`validate_block_structure()` accepts every subsequent block under the same
criteria. There is no separate "genesis construction path" that regular blocks
must emulate — there is one validator, and all blocks pass it.

### Genesis Miner Identity

The genesis block's coinbase nullifier `nf_1` is computed from the per-block
derived key `sk_1 = derive_instance(sk_genesis, NATIVE_TOKEN_CONTRACT_ID, 1.to_le_bytes())`.
The genesis miner identity `sk_genesis` is the well-known key declared in the
node's `keys.toml` under the section that creates genesis (typically `[node0]`).
The `init_genesis()` function in `bin/dwowd/src/lib.rs` reads this key from the
configured AccountManager and derives `sk_1` deterministically.

Any node configured with the same `[node0]` secret will produce an identical
genesis block — the coinbase is plaintext (no ZK proof), so no
randomness enters the block. The network's genesis is identified by its block hash, not
by the miner who created it.

**The hash is PINNED.** `bin/dwowd/genesis_hash.txt` holds the canonical genesis hash —
`blake3` of the mining blob, 64 lowercase hex, no `0x` — and is compiled in via
`include_str!`, so `init_genesis` in `bin/dwowd/src/lib.rs` HARD-ERRORS on a mismatch
("GENESIS HASH MISMATCH") rather than warning. An all-zeros value is the placeholder
sentinel, which warns instead of enforcing; the pin is currently POPULATED, so it enforces.

Because there is exactly ONE pin, there can be exactly one genesis identity for the whole
repository: the devnet node0 key declared in `contrib/docker/darkwow-testnet/keys.toml`
(`755c6e8a…`) with the `DRKW` network magic. Every caller of `init_genesis` — including the
test suite, which defines it once as `GENESIS_KEYS_TOML` / `DRKW_MAGIC` in
`bin/dwowd/src/tests/modules/chain_setup.rs` — must use that identity, or the block it
builds will not match the pin. Note the genesis hash therefore depends on the key, the magic
bytes and the compiled contract WASM; it does not depend on wall-clock time
(`timestamp = 0`).

`genesis_hash.txt` is regenerated only as part of a deliberate genesis re-roll: run
`CREATE_GENESIS=true` (i.e. `darkwow node --role genesis`) into a FRESH datadir, copy the
`Computed hash:` value the node logs, and rebuild. A datadir that already holds a height ≥ 1
chain cannot be re-rolled in place — `init_linear` refuses.

## Cumulative Supply Bootstrap

The cumulative supply chain uses a Pedersen commitment accumulator:

```
S_H = S_{H-1} + C_H    where C_H = pedersen_commit(reward(H), blind(H))
```

This invariant is validated by `pow_reward_v1` in the NativeToken WASM contract, and the
same induction is written in `SupplyChain.lean` as `total_supply_theorem`.

**What that proof does and does not establish.** It establishes that the running total equals
the sum of the emission schedule — a structural induction that holds for *any* schedule. It does
**not** establish that the schedule is capped: `total_reward_bounded`, the assumption that the
sum never exceeds `MAX_SUPPLY`, has no consumer, so the cap is unproved here and the proof would
not notice if it were false. The Pedersen homomorphism that would make the commitment chain mean
what this section reads it as is likewise an *assumption*
(`Axioms.pedersen_additive_homomorphism`, `HAZOP.High` HIGH-6), consumed by no proof term.

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

This is the canonical emission/supply section — other docs that restate these
numbers (consensus-coinbase.md §4, the docker READMEs, fee-spec.md) link here.
The executable source of truth is `sim/crypto.py` and
[`reward::expected_reward`](../../../src/sdk/src/blockchain.rs).

```
R(0) = 0                                        (pre-genesis)
R(1) = R₀                                       (genesis coinbase, ~13.84 DRKW)
R(h) = max(R₀ × 2^(-(h-1)/H), R_tail)  for h ≥ 2 (continuous exponential decay)
```

Where `R₀ = 1,383,764,049` base units and `H = 1,051,920` blocks, floored at
`R_tail = 79,853,981` base units (~0.80 DRKW per block, perpetual). The
21,000,000 DRKW figure is the tail-onset reference supply, NOT a hard cap. The
emission schedule starts at height 1 — genesis is the first point on the decay
curve, not a zero-reward preamble. Note the decay exponent is `height - 1`: the
first decayed reward appears at height 2 as `R₀ × 2^(-1/H)`.

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
- [Cumulative Supply Chain Proof](../../../proofs/lean/src/DarkFi/SupplyChain.lean) — Inductive proof of the mass balance invariant
- Source: `src/sdk/src/crypto/contract_id.rs`, `bin/dwowd/src/lib.rs`, `src/sdk/src/blockchain.rs`
