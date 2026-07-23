# Circuit Hardening Handover — 2026-07-23

## What Was Done

7 commits shipped across 3 remediation sessions. 23 circuits fixed across 6 contracts.

### Shipped

| RC | Finding | Circuits Fixed | Contracts | Commit |
|----|---------|---------------|-----------|--------|
| F1 | proofs.len() zip truncation guard | 1 | chain | `49fbb65` |
| RC2 | zero_cond Merkle bypass (less_than_strict guard) | 6 | bearer_bond, bridge(5) | `04481a1` |
| RC5 | bearer_bond missing PN fixes (signature binding, coin_value=ZERO) | 2 | bearer_bond | `04481a1` |
| RC1-A | bool_check on u64 amounts (removed) | 11 | stablecoin(7), dex(4) | `d8ae756` |
| RC1-B | coin_public unbound (constrain_equal_base to mint_public) | 1 | promissory_note | `c1c539a` |

### Pattern Established

Each fix follows the same type-system approach:
- Circuit vulnerability = capability exhibited without possessing the required name (type-system.md §5)
- Fix = add cryptographic constraint binding the witness to the secret
- Reference circuits with correct patterns: FeeCollect_V1 for EC binding, promissory_note burn_v1 for zero_cond guard + signature_secret binding

## What Remains

### RC3: Domain Separation (177 circuits across 30 contracts)

**The fix**: For each .zk file, at the start of the `circuit` block, declare domain constants via `witness_base(N)` and prepend them to every `poseidon_hash` call. Pattern from `native_token/proof/mint_v2.zk`:

```
circuit "Xxx_V2" {
    DOMAIN_NULLIFIER = witness_base(1);
    DOMAIN_TOKEN_COMMIT = witness_base(2);
    DOMAIN_TX_BINDING = witness_base(3);
    DOMAIN_COIN_COMMIT = witness_base(4);
    DOMAIN_USER_DATA_ENC = witness_base(6);
    DOMAIN_SIGNATURE_SECRET = witness_base(7);

    # Then: nf = poseidon_hash(DOMAIN_NULLIFIER, coin_secret, C);
    #       C = poseidon_hash(DOMAIN_COIN_COMMIT, pub_x, pub_y, ...);
```

After circuit changes, each contract needs:
1. Compile: `make -C src/contract/<name> all`
2. Add V2 namespace constants to `src/contract/<name>/src/lib.rs`
3. Update `include_bytes!` + `zkas_db_set` in entrypoint `init_contract()`
4. Switch `get_metadata` functions from V1→V2 namespaces
5. Update manifest: `proof_circuit` fields, `[[circuits]]` table, `version`

**Priority order** (plan at `/home/patrick/.claude/plans/agile-floating-karp.md`):
1. stablecoin (10 circuits) — most critical, asset issuance
2. bridge (7 circuits) — cross-chain wrapping
3. promissory_note (5 circuits) — bearer instruments
4. dex (8 circuits) — exchange
5. identity (8 circuits) — credentials
6. Remaining 20 contracts (139 circuits)

### RC4: base_div Replacement (~12 circuits across ~8 contracts)

Replace `base_div(a, b)` (field division) with quotient-remainder constraints or cross-multiplication. Pattern from already-fixed `dex/proof/execute_swap_slippage_v1.zk`. Circuits listed in plan §RC4.

## Key Files

- Plan: `/home/patrick/.claude/plans/agile-floating-karp.md`
- Domain constants: `src/sdk/src/crypto/constants.rs` (7 constants defined, limbs 1-7)
- Correct EC binding pattern: `native_token/proof/fee_collect_v2.zk:44-51` or `native_token/proof/fee_collect_v1.zk:44-51`
- Correct zero_cond guard pattern: `promissory_note/proof/burn_v1.zk:74-75`
- Correct V2 circuit pattern: `native_token/proof/mint_v2.zk` (full example with domain constants + M8 fix)
- Branch: `linear-master` on `codeberg-tor:PatrickM123/darkwow.git` (mirror: `github.com:PatrickMockridge/DarkWow.git`)
