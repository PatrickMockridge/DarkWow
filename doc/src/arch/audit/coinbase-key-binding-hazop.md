# Coinbase key-binding — HAZOP + bow-tie

Guide-word deviation analysis over the coinbase note key ↔ `header.miner` binding, plus a bow-tie.
Guide words: NO / NOT / PART OF / AS WELL AS / REVERSE / OTHER THAN / EARLY / LATE, plus MORE / LESS
(`l1-write-path-hazop.md`, `compile-fragilities-hazop.md`). Each finding cites `file:line` on `linear-master`.

## Subject and observed failure

`daemon_sync_integration` (10/10) fails on the coinbase key-binding consensus rule:

```
accept_block height 2: Custom("Block 2: coinbase note is bound to a key that is not header.miner")
```

The rule (`bin/dwowd/src/block_acceptor.rs:341-347`): for every non-genesis block, the coinbase note's
`commitment_attrs.public_key` must equal `header.miner`, so a reward note is only spendable by the declared
miner. Genesis is exempt (no miner identity — `miner: [0u8;32]`, `init_genesis`).

## Central root cause

**The built-in miner's block builder never writes `header.miner`.** `create_block_with_uncles`
(`src/linear/src/block.rs:683`) constructs the header with `miner: [0u8; 32]` and a comment
"Placeholder — miner sets reward public key (pk_H)". No step after `create_block` fills that field. Meanwhile
the coinbase note binds a **real** derived key: `MiningRecipient::from_account(mgr, height)` derives
`sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, height.to_le_bytes())`
(`crates/dwow-accounts/src/lib.rs:1265-1273`), and the note's `commitment_attrs.public_key` is set to that
recipient's public key (`src/contract/native_token/src/client/pow_reward.rs:128`,
`bin/dwowd/src/registry/model.rs:710,724`).

So the two compared quantities diverge by construction on the built-in-miner path: the note key is the derived
per-height `pk_H`, `header.miner` is `[0u8;32]`. The check (added in `55a04076c9`) is **correct** and is what
surfaced the defect; the defect is in the block builder, not the check.

Only the **stratum / mm_rpc template path** sets `header.miner` correctly:
`generate_linear_block_template` (`bin/dwowd/src/registry/model.rs:724`) writes
`miner: recipient_config.recipient.public().to_bytes()`. The built-in miner (`prepare_block`, `lib.rs` →
`create_block`, `block.rs`) does not. `daemon_sync_integration` exercises the built-in miner, hence the failure.

## Mapped path

| step | where | value |
|---|---|---|
| recipient derivation | `crates/dwow-accounts/src/lib.rs:1265-1273` | `sk_H = derive_instance(owner, NATIVE_TOKEN_CONTRACT_ID, height.to_le_bytes())` |
| note key | `src/contract/native_token/src/client/pow_reward.rs:128` | `recipient.public()` (the derived `pk_H`) |
| `header.miner` (template path) | `bin/dwowd/src/registry/model.rs:724` | `recipient_config.recipient.public()` — correct |
| `header.miner` (built-in miner) | `src/linear/src/block.rs:683` | `[0u8; 32]` placeholder — **never filled** |
| check | `bin/dwowd/src/block_acceptor.rs:341-347` | `note.key == header.miner` (genesis exempt) |

## HAZOP findings

### V1 — NO (miner identity) → `create_block_with_uncles` never sets `header.miner`

- **Node:** `src/linear/src/block.rs:683` `miner: [0u8; 32]`.
- **Deviation:** the field is written as a zero placeholder and no later step on the built-in-miner path
  (`prepare_block`, `lib.rs`) overwrites it. The coinbase note, built from the same `height`, binds the derived
  `pk_H` — so `header.miner == [0u8;32]` while `note.key == pk_H`.
- **Mechanism:** two independent block-construction paths exist — `generate_linear_block_template` (stratum /
  mm_rpc) sets `header.miner`; `prepare_block` → `create_block` does not. The check cannot distinguish "no
  miner" (genesis) from "miner not set" (a bug) for height ≥ 2, so it rejects the block.
- **Invariant violated:** `uncle_merkle.md` §"Miner identity in the header" — `header.miner` SHALL be the
  coinbase recipient's `pk_H`.
- **Structural fix:** the built-in-miner path must set `header.miner` to the coinbase recipient's public key
  (mirror `model.rs:724`), or `create_block` must take and write it.

### V2 — PART OF (coverage) → only one of the two mining paths is bound

- **Node:** `generate_linear_block_template` (sets `miner`) vs `prepare_block`/`create_block` (does not).
- **Deviation:** the key-binding requirement is satisfied on the template path but not the built-in-miner path,
  so the invariant holds only "as well as" the template is used.
- **Invariant violated:** build-path uniformity — every block-construction path SHALL produce a header that
  satisfies the same consensus rules.
- **Structural fix:** route both paths through one header-construction function that writes `header.miner`.

### V3 — OTHER THAN (key) → the note key and `header.miner` are derived from different things

- **Node:** `pow_reward.rs:128` (note key = `recipient.public()`) vs `block.rs:683` (`miner = [0u8;32]`).
- **Deviation:** the note's key is the per-height derived `pk_H`; `header.miner` is the zero placeholder. On the
  template path they agree only because `model.rs:724` copies the same recipient in.
- **Invariant violated:** the binding the check enforces.
- **Structural fix:** make `header.miner` a single source of truth for the recipient key (set it where the
  recipient is derived, not duplicated per path).

### V4 — EARLY / LATE (the check vs the builder) → the check landed without the builder

- **Node:** the check `block_acceptor.rs:341` (added `55a04076c9`) vs the builder `block.rs:683` (predates it).
- **Deviation:** the enforcement was added before the built-in-miner builder was taught to satisfy it — a
  classic "barrier without the prevention upstream". The barrier then rejects legitimate miner output.
- **Invariant violated:** a new consensus rule SHALL be accompanied by the corresponding production change on
  every path that must satisfy it.
- **Structural fix:** treat the check as the barrier and fix the builder (V1/V2); do not weaken the check.

## Bow-tie

```
THREATS                                        TOP EVENT                                  CONSEQUENCES
────────                                        ─────────                                  ────────────
built-in miner leaves  ──┐                                                              ┌── miner cannot
  header.miner = [0;32]   │                                                              │   produce blocks
                          │  ┌───────────────────────────────────────────────┐          │   (liveness)
                          ├─▶│ coinbase note bound to a key ≠ header.miner  │──────────┤
note binds derived pk_H    │  │ (reward minted to the wrong / unset party)  │          ├── if the check were
  (derive_instance)       ──┘  └───────────────────────────────────────────────┘          │   absent: reward
                                                                                          │   theft (note spendable
                                                                                          │   by a non-miner)
       ┌──────────────── preventive barriers ────────────────┐                ┌────────────── recovery ──────────────┐
       │ key-binding check  block_acceptor.rs:341            │                │ pinned genesis hash (genesis        │
       │ mass-balance check block_acceptor.rs:321            │                │   authenticity — unaffected)        │
       │ deterministic key path (no RNG) model.rs:201-208    │                │ daemon_sync / plaintext_rewards     │
       │ single recipient-config  model.rs:50-64             │                │   suite (caught this)               │
       └─────────────────────────────────────────────────────┘                │ wallet deterministic decrypt (fails  │
                                                                             │   to find its coinbase → surfaces)  │
                                                                             └──────────────────────────────────────┘
```

## Verification

- Root cause is `block.rs:683` (`miner: [0u8;32]` placeholder, never filled) vs `model.rs:724` (template path
  fills it), confirmed by reading both builders and the check — consistent with the observed `height 2` failure
  and the genesis exemption.
- The check (`block_acceptor.rs:341`, `55a04076c9`) is a correct barrier; the defect is upstream in the
  built-in-miner builder.
- Fix (follow-up, after agreement): set `header.miner` on the `prepare_block`/`create_block` path to the
  coinbase recipient's public key, and route both paths through one header constructor.
