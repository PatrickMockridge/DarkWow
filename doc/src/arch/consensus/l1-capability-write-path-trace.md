# L1 Capability Write-Path — ρ-Calculus Trace

This is the per-action ρ-calculus trace for the six L1 capability write-path operations. It instantiates
the generic `WritePath` skeleton (`wallet.md §6.4.1`) against each contract's barb table, `witness_map`,
derived rules, and `constrain_instance` public-input order. Notation per `type-system.md §0`/`§1.1` and
`ocap.md §6`.

It is the **input to the HAZOP** (`l1-write-path-hazop.md`): the seams the trace makes visible are the
deviation points the HAZOP then analyzes.

## The generic `WritePath` (reference — `wallet.md:791-808`)

```
WritePath(cid, action, params) =
  νseed.(
    exercise!(caps, action, params, seed)
      . resolve!(manifest, cid)
      . select!(caps, requiredBarbs)
      . ( bindInputs!(witness_map, caps, params)
          | computeDerived!(rules, bound)
          | extractInstances!(opcodes, bound) )
      . createProof!(circuit, seed)
      . assembleParams!(bound, schema)
      . publish!(note!(new_state), nullifier!)
      . finalize!(fee)
  )
```

The derived-rule table is closed (`wallet.md:763-776`); **operands are 0-based witness-slot indices** —
a rule may reference only bound witness slots, never another rule's intermediate output. Domain constants
are the 7 `DRK_POSEIDON_DOMAIN_*` values. The 8 write-path invariants are `wallet.md:810-819`.

## Consensus-critical seams (shared by all six actions)

1. **`↓nullify` (single-use)** — exec `db_contains_key` reuse check.
2. **`↓prove-inclusion` (Merkle existence)** — old state anchored at a root in the contract's `*_roots` tree.
3. **Value conservation** — `↓conserve` (Pedersen for purse, cross-proof per-token for PN), the anti-inflation gate.
4. **Merkle-triple congruence (T5)** — `(leaf_position, merkle_path, merkle_root)` all from one tree `τ_c`.
5. **Ordering DAG** — `↓nullify` → `↓prove-inclusion` → `↓spend` → `↓denominate` → `↓commit`.

---

## 1. Box `put` (opcode 0x01)

consume+create over `(box_id, contents_commit, state_nonce)`.

```
Put =
  νseed.(
    ↓spend        — owner_pub == poseidon(7, owner_secret)
      . ↓nullify — nullifier == poseidon(1, owner_secret, box_id, old_state_nonce)
      . ↓prove-inclusion — root == merkle_root(leaf_pos, path, old_leaf),
                 old_leaf = poseidon(5, box_id, old_contents_commit, old_state_nonce)
      . ↓commit  — apply appends new_leaf = poseidon(5, box_id, new_contents_commit, new_state_nonce),
                 marks nullifier
      . ↓encrypt — produce-side note {commitment, state_nonce} → new owner
  )
```

Witness sources (`box/manifest.toml:36-51`): `param:box_id, param:old_state_nonce, param:new_state_nonce,
param:old_contents_commit, param:new_contents_commit, derived:nullifier:8,0,1, merkle_root,
derived:leaf:0,4,2, secret, leaf_position, merkle_path, tx_commitment, param:tx_nonce,
derived:tx_binding:11,12`.

Public inputs (`box.md:45`, `put.zk`): `nullifier, expected_root, new_leaf, tx_binding, tx_nonce`.

**No seam** — box separates `old_state_nonce`/`new_state_nonce` (witness slots 1/2) and the nullifier binds
the OLD nonce only. This is the reference pattern the purse trace below fails to follow.

## 2. Box `take` (opcode 0x02)

consume-only (terminal; no produce-side note).

```
Take =
  νseed.(
    ↓spend . ↓nullify — nullifier == poseidon(1, owner_secret, box_id, state_nonce)
           . ↓prove-inclusion — root == merkle_root(leaf_pos, path, old_leaf),
                 old_leaf = poseidon(5, box_id, contents_commit, state_nonce)
           . ↓commit — mark nullifier (no new leaf)
  )
```

Witness sources (`box/manifest.toml:53-68`): `param:box_id, param:contents_commit, param:state_nonce,
derived:nullifier:5,0,2, merkle_root, secret, leaf_position, merkle_path, tx_commitment, param:tx_nonce,
derived:tx_binding:8,9`.

Public inputs: `nullifier, expected_root, tx_binding, tx_nonce`.

**No seam.**

## 3. Purse `deposit` (opcode 0x01)

consume+create over `(purse_id, state_nonce)`, balance hidden in Pedersen commitment.

```
Deposit =
  νseed.(
    ↓spend . ↓nullify — nullifier == poseidon(1, owner_secret, purse_id, state_nonce)
           . ↓prove-inclusion — root == merkle_root(leaf_pos, path, old_leaf),
                 old_leaf = poseidon(5, purse_id, old_balance, state_nonce)
           . ↓denominate — token_commit == poseidon(2, asset_id, token_blind)
           . ↓conserve — Pedersen old_commit + deposit_commit == new_commit
           . ↓commit — apply appends new_leaf = poseidon(5, purse_id, new_balance, state_nonce)
           . ↓encrypt — note {asset_id, value, balance_blind, commitment}
  )
```

Witness sources (`purse/manifest.toml:40-66`): `param:purse_id, param:old_balance, note:balance_blind,
param:deposit_amount, blind:deposit, param:new_balance, derived:blind_sum:2,4, param:state_nonce,
derived:nullifier:15,0,7, merkle_root, derived:leaf:0,5,7, derived:pedersen_x:1,2, derived:pedersen_y:1,2,
derived:pedersen_x:5,6, derived:pedersen_y:5,6, secret, derived:owner_pub:15, leaf_position, merkle_path,
tx_commitment, param:tx_nonce, derived:tx_binding:19,20`.

Public inputs: `nullifier, expected_root, old_commit_x, old_commit_y, new_commit_x, new_commit_y, new_leaf,
tx_binding, tx_nonce`.

**SEAM (purse-nonce-share):** `derived:nullifier:15,0,7` and `derived:leaf:0,5,7` **both** use `state_nonce`
(slot 7) as the nonce. The nullifier binds the old nonce; the produced leaf also carries the *same* nonce.
A deposit→withdraw chain must therefore keep `state_nonce` constant (the withdraw's `old_leaf` must equal the
deposit's `new_leaf`), which makes the deposit and withdraw nullifiers **identical** — the chaining collision.

## 4. Purse `withdraw` (opcode 0x02)

```
Withdraw =
  νseed.(
    ↓spend . ↓nullify — nullifier == poseidon(1, owner_secret, purse_id, state_nonce)
           . ↓prove-inclusion — old_leaf = poseidon(5, purse_id, old_balance, state_nonce)
           . ↓bound — 0 < withdraw_amount <= old_balance
           . ↓denominate . ↓conserve — Pedersen old_commit == new_commit + withdraw_commit
           . ↓commit — new_leaf = poseidon(5, purse_id, new_balance, state_nonce)
           . ↓encrypt — note {asset_id, value, balance_blind, commitment}
  )
```

Witness sources (`purse/manifest.toml:68-94`): same shape as deposit with `blind:withdraw` and
`derived:blind_sub:2,4` in place of `blind:deposit`/`derived:blind_sum:2,4`.

**SEAM (purse-nonce-share)** — identical to deposit.

## 5. PN `transfer` (opcode 0x04) — **multi-proof**

Burn-per-input (`RevokeV2`) + mint-per-output (`TransferV2`), proofs in that order (`entrypoint/mod.rs:393-432`),
plus cross-proof value conservation.

```
Transfer =
  νseed.(
    ( for each input: RevokeV2 burn
        ↓spend — pub = poseidon(7, spend_secret)
        ↓nullify — nullifier == poseidon(1, spend_secret, coin),
                   coin = poseidon(4, pub, value, asset_id, coin_spend_hook, user_data, commitment_blind)
        ↓prove-inclusion — root == merkle_root(leaf_pos, path, coin)
        ↓denominate — token_commit == poseidon(2, asset_id, asset_id_blind)
        ↓signature — signature_secret = poseidon(7, spend_secret, nullifier);
                     signature_public = poseidon(7, signature_secret)
    | for each output: TransferV2 mint
        ↓commit — new coin leaf
        ↓encrypt — note {value, asset_id, spend_hook, user_data, commitment_blind, value_blind, token_blind, memo, commitment}
    )
    . ↓conserve — Σ input value_commit == Σ output value_commit (per token_commit group)
  )
```

Witness sources — RevokeV2 (`promissory_note/manifest.toml:250-269`): `secret, note:value, note:asset_id,
note:spend_hook, note:user_data, note:commitment_blind, note:value_blind, note:token_blind,
blind:user_data_blind, leaf_position, merkle_path, derived:signature_secret:0,0, tx_commitment, tx_nonce,
derived:tx_binding:12,13`.

Public inputs (RevokeV2, from `client/revoke.rs:72-83`): `nullifier, value_commit_x, value_commit_y,
token_commit, merkle_root, user_data_enc, spend_hook, signature_public, tx_binding, tx_nonce`.

**SEAM (pn-nested-derived):** the burn's `nullifier`, `coin`, and `signature_secret` are a **nested chain** —
`signature_secret = poseidon(7, secret, nullifier)` where `nullifier = poseidon(1, secret, coin)` and
`coin = poseidon(4, pub, …)`. The manifest's `derived:signature_secret:0,0` computes `poseidon(7, secret,
secret)` (both operands slot 0), not the circuit's `secret+nullifier` form; the derived-rule table
(`wallet.md:763-776`) cannot express an operand that is itself a derived output.

**SEAM (pn-multi-proof):** the manifest declares one `proof_circuit` per function; `transfer` needs
`RevokeV2` (burn) + `TransferV2` (mint) — two circuits — while `ManifestContractClient::build` emits exactly
one proof.

## 6. PN `redeem` (opcode 0x01) — **multi-proof**

Burn-per-input (`RevokeV2`) + one zero-value receipt (`RedeemV2`).

```
Redeem =
  νseed.(
    ( for each input: RevokeV2 burn — as in Transfer )
    | RedeemV2 receipt
        ↓commit — zero-value receipt coin
        ↓encrypt — receipt note (value == 0, ZK-constrained)
    )
    // deliberately BREAKS ↓conserve — value destruction
  )
```

Witness sources — RedeemV2 (`promissory_note/manifest.toml:203-218`) with `public_inputs` declared
(`:219-228`).

**SEAM (pn-multi-proof)** — as in Transfer (burn + receipt = two circuits).

---

## Seams summary (→ HAZOP input)

| # | Seam | Action | Trace step |
|---|---|---|---|
| S1 | `state_nonce` shared across old/new leaf + nullifier | purse deposit/withdraw | `↓nullify` + `↓commit` |
| S2 | nested derived chain `coin → nullifier → signature_secret` | PN transfer/redeem (burn) | `↓nullify` + `↓signature` |
| S3 | burn+mint = two proofs vs one `proof_circuit` | PN transfer/redeem | whole trace |
| S4 | `derived:signature_secret:0,0` computes `poseidon(7,secret,secret)` | PN burn | `↓signature` |

These four seams are the deviation points the HAZOP (`l1-write-path-hazop.md`) analyzes with guide words.
