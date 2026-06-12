# for-dummies.md Audit — Claims vs Implementation

**Date:** 2026-06-10
**File under audit:** [doc/src/about/for-dummies.md](doc/src/about/for-dummies.md)
**Method:** Systematic claim-by-claim verification against all source files in `src/`, `bin/`, `Cargo.toml`
**Result:** 13 gaps found (3 CRITICAL, 2 HIGH, 4 MEDIUM, 4 LOW)

---

## Executive Summary

`for-dummies.md` is the public-facing introduction to DarkWow's architecture and cryptographic primitives. The document gets the big-picture architecture right — PoW/RandomX consensus, Poseidon-based privacy, Halo 2 ZK proofs, zkas/zkVM toolchain, P2P multi-transport network, and the contract ecosystem are all accurately described at a high level.

However, the document has **three CRITICAL factual errors** in its cryptographic descriptions that would mislead any reader trying to understand how the system actually works. The simplified commitment and nullifier formulas are wrong, and the document describes a Mint operation that is disabled in the actual code. Two HIGH-severity issues involve misleading claims about Pedersen/ECC usage and I2P transport. Several medium/low issues involve imprecise descriptions.

Every claim that is correct should stay as-is — the document's accessible tone and structure are good. Only the inaccurate parts need changing.

---

## CRITICAL Findings (factual errors — must fix)

### C1: Coin commitment formula is wrong

**Claim in doc (lines 42-48, 74-76):**
> "C = PoseidonHash(value, nonce)"
> "Mint: Create a coin with secret s. Publish commitment C(s, nonce)."

**Actual implementation:**
[src/contract/native_token/src/model/mod.rs:78-82](src/contract/native_token/src/model/mod.rs#L78-L82):
```rust
let coin = poseidon_hash([
    pub_x, pub_y, pallas::Base::from(value), token_id, spend_hook, user_data, blind,
]);
```

The coin commitment is a Poseidon hash of **7 fields**: `(public_key_x, public_key_y, value, token_id, spend_hook, user_data, blind)`. It is NOT `C(s, nonce)` or `C(value, nonce)`. The simplified formula in the doc is so far from reality that it's misleading — a reader who looks at the code after reading the doc would be confused.

**Severity:** CRITICAL — this is the foundational privacy primitive and the doc describes it wrong.

**Fix:** Replace lines 42-48 and the Mint step (line 75) with the correct 7-field formula. Keep it accessible by showing the structure with named fields:
```
C = PoseidonHash(pub_x, pub_y, value, token_id, spend_hook, user_data, blind)
```
Explain that `blind` is the random hiding factor (the "nonce" analog), and that the other fields encode the coin's properties. The commitment binds the value, token type, spend conditions, and user data while the blind keeps them hidden.

---

### C2: Nullifier formula is wrong

**Claim in doc (lines 64-65, 77):**
> "A nullifier is a unique hash derived from a secret."
> "Spend old coin (reveal nullifier H(s))"

**Actual implementation:**
[src/contract/native_token/src/model/nullifier.rs:44-47](src/contract/native_token/src/model/nullifier.rs#L44-L47):
```rust
/// nullifier = poseidon_hash(spending_key, coin_hash)
pub fn new(secret: SecretKey, coin_hash: pallas::Base) -> Self {
    Self(poseidon_hash([secret.inner(), coin_hash]))
}
```

The nullifier is `poseidon_hash(secret_key, coin_hash)`, **not** `H(secret)` alone. It binds the spending key to the specific coin being spent. The doc's `H(s)` implies one nullifier per secret, but the actual formula produces a **different nullifier per coin** even with the same secret key — because each coin has a different `coin_hash`. This is a critical privacy property: without the coin_hash binding, spending two coins with the same key would reveal they share a secret.

**Severity:** CRITICAL — misrepresents the double-spend prevention mechanism.

**Fix:** Update lines 64-65 and 77 to show the correct formula:
```
nullifier = PoseidonHash(secret_key, coin_hash)
```
Explain that this binds the nullifier to both the secret AND the specific coin, so the same secret produces different nullifiers for different coins — preventing linkage while still preventing double-spends.

---

### C3: "Mint" described as a user operation, but MintV1 is disabled

**Claim in doc (line 75):**
> "Mint: Create a coin with secret s. Publish commitment C(s, nonce)."

**Actual implementation:**
[src/contract/native_token/src/entrypoint/mod.rs:444-446](src/contract/native_token/src/entrypoint/mod.rs#L444-L446):
```rust
NativeTokenFunction::MintV1 => {
    msg!("[native_token::process_instruction] MintV1 is disabled (unauthorized mint path -- use PoWRewardV1 for block rewards)");
    Err(ContractError::InvalidFunction)
}
```

MintV1 (opcode 0x01) is **explicitly disabled**. The ONLY authorized coin creation paths are:
- `PoWRewardV1` (0x05) — block rewards for miners
- `GenesisMintV1` — genesis block initialization

The doc's "Mint" step implies any user can create coins by publishing a commitment. In reality, coin creation is restricted to consensus-level operations. The three-step scheme (Mint → Transfer → Burn) is pedagogically useful but the "Mint" step should reflect that coin creation is consensus-gated.

**Severity:** CRITICAL — describes a disabled code path as a user-facing feature.

**Fix:** Restructure the 3-step scheme to be honest:
1. **Coin Creation (PoW Rewards):** New coins are created ONLY as block rewards for miners, enforced by the consensus protocol. User-initiated minting is disabled.
2. **Transfer:** (unchanged — spend old coin, create new coin)
3. **Burn:** (unchanged — destroy a coin)

This keeps the pedagogical value while accurately reflecting the actual security model.

---

## HIGH Findings (misleading claims — should fix)

### H1: "No Pedersen commitments, no elliptic curve arithmetic in circuits" is misleading

**Claim in doc (lines 51-52):**
> "DarkWow uses the Poseidon hash function throughout — no Pedersen commitments, no elliptic curve arithmetic in circuits."

**What's actually in the code:**

Pedersen commitments are used **extensively** across the system:
- `native_token` — value commitments in TransferV1, BurnV1, FeeV1, PoWRewardV1 ([src/contract/native_token/src/client/transfer_v1/proof.rs:111](src/contract/native_token/src/client/transfer_v1/proof.rs#L111))
- `promissory_note` — value conservation via Pedersen additive homomorphism ([src/contract/promissory_note/src/model/mod.rs:27](src/contract/promissory_note/src/model/mod.rs#L27))
- `bearer_bond`, `escrow`, `otc_swap`, `baccarat`, `darktoshi_dice`, `stablecoin` — all use Pedersen
- `bridge` — Pedersen for value commitments ([src/contract/bridge/src/model/mod.rs:353](src/contract/bridge/src/model/mod.rs#L353))
- The SDK exports public functions: `pedersen_commitment_base()` and `pedersen_commitment_u64()` ([src/sdk/src/crypto/pedersen.rs:39-56](src/sdk/src/crypto/pedersen.rs#L39-L56))

The zkVM HAS ECC chip support with these opcodes ([src/zkas/opcode.rs:109-134](src/zkas/opcode.rs#L109-L134)):
`EcAdd`, `EcMul`, `EcMulBase`, `EcMulShort`, `EcMulVarBase`, `EcGetX`, `EcGetY`, `ConstrainEqualPoint`

The statement is technically defensible with a very narrow reading of "in circuits" (the EC arithmetic computation happens in Rust client code, and the resulting points are passed into circuits as public inputs for constraint checking). But a reader would interpret "no elliptic curve arithmetic" as "DarkWow doesn't use elliptic curves for anything other than Poseidon hashing," which is wrong. **Value conservation across the entire contract ecosystem depends on Pedersen commitments.**

**Severity:** HIGH — the doc creates a false mental model of the cryptographic architecture.

**Fix:** Replace lines 51-52 with an honest description:
> "DarkWow uses Poseidon hashing for coin commitments and nullifiers. Value conservation (ensuring no coins are created or destroyed in transfers) uses Pedersen commitments with additive homomorphism — input and output values are committed as elliptic curve points, and the commitment sums are verified equal. The actual elliptic curve arithmetic happens in the client/prover, with the resulting points passed into ZK circuits as public inputs for constraint checking."
>
> Or simply remove the "no Pedersen commitments" claim entirely — it's an implementation detail that doesn't help a "for dummies" reader, and getting it wrong does harm.

---

### H2: I2P described as "garlic routing" but implementation is SOCKS5 proxy

**Claim in doc (line 30):**
> "I2P: Network-level anonymity (garlic routing)"

**Actual implementation:**
[src/net/transport/mod.rs:242-263](src/net/transport/mod.rs#L242-L263) — I2P is handled by constructing a SOCKS5 dialer to an external I2P proxy (default: `socks5://127.0.0.1:4447`). There is:
- NO native I2P SAM protocol implementation
- NO embedded I2P router
- NO inbound/listener support for I2P addresses
- The word "garlic" appears nowhere in any source file
- The feature gate `p2p-i2p` depends solely on `p2p-socks5` ([Cargo.toml:282-284](Cargo.toml#L282-L284))

This means DarkWow does NOT implement garlic routing. It tunnels through an external I2P router that the user must run separately. The phrase "network-level anonymity (garlic routing)" implies a native implementation.

**Severity:** HIGH — overstates what DarkWow implements vs what it delegates.

**Fix:** Update line 30 to be accurate:
> "I2P: Connectivity via external I2P router (SOCKS5 proxy)"

Or expand slightly:
> "I2P: Network-level anonymity by connecting through an external I2P router (the user runs their own I2P node; DarkWow tunnels through it via SOCKS5)"

---

## MEDIUM Findings (imprecise descriptions — should fix)

### M1: Burn reveals more than just the nullifier

**Claim in doc (line 78):**
> "Burn: Destroy a coin by revealing its nullifier."

**Actual implementation:**
[src/contract/native_token/src/client/burn_v1.rs:48-56](src/contract/native_token/src/client/burn_v1.rs#L48-L56) — The `BurnRevealed` struct contains:
```rust
pub struct BurnRevealed {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: PublicKey,
}
```

While the state model records only the nullifier for double-spend prevention, the on-chain transaction includes the full set of public inputs. The doc is not wrong that the nullifier is the key mechanism, but "revealing its nullifier" understates what's published.

**Severity:** MEDIUM — not factually wrong but incomplete.

**Fix:** Add a qualifier: "Burn: Destroy a coin by revealing its nullifier (along with other public inputs needed for verification)."

---

### M2: RLN slashing is identity-based, not financial

**Claim in doc (lines 149-156):**
> "Users stake tokens as collateral... Double-posting causes automatic slashing of stake."

**Actual implementation:**
[bin/darkirc/src/crypto/rln.rs:102-167](bin/darkirc/src/crypto/rln.rs#L102-L167) — RLN uses **Shamir Secret Sharing** for slashing. When a user double-posts within an epoch, two polynomial shares `(x₁, y₁)` and `(x₂, y₂)` are revealed. Lagrange interpolation recovers the secret key `a₀`, which is added to a **ban set**. This is identity-based slashing (the user's key is exposed and they're banned), not financial slashing of staked tokens. There is no on-chain collateral or token slashing in the darkirc RLN implementation.

**Severity:** MEDIUM — describes a different slashing model than what's implemented.

**Fix:** Update lines 149-156:
> "Users register an RLN identity with a secret key. Each message reveals a nullifier unique to that epoch and a Shamir secret share. Double-posting within the same epoch reveals two shares, allowing the network to recover the user's secret key and ban them — without needing a central moderator."

---

### M3: exec()/apply() naming discrepancy

**Claim in doc (lines 123-125):**
> "1. exec(): Read-only phase - compute state changes"
> "2. apply(): Write phase - apply state changes"

**Actual implementation:**
[src/sdk/src/wasm/entrypoint.rs:33-78](src/sdk/src/wasm/entrypoint.rs#L33-L78) — The WASM exports are `__entrypoint` (exec) and `__update` (apply). The runtime API uses `exec()` and `apply()` method names on `VmRuntime` ([src/runtime/vm_runtime.rs:625-653](src/runtime/vm_runtime.rs#L625-L653)). The doc uses the Rust runtime API names, which is fine at the conceptual level, but the WASM-level names differ.

**Severity:** MEDIUM — correct at the conceptual/Rust API level but wrong at the WASM level.

**Fix:** Either leave as-is (it's conceptual documentation) or clarify: "At the WASM level, these are `__entrypoint` and `__update`; the runtime exposes them as `exec()` and `apply()`."

---

### M4: History timeline cannot be verified from this repo's git history

**Claim in doc (lines 160-168):**
Timeline from 2018-2019 (Sapling) through 2024+ (Bridge Protocol).

**Actual git history:**
The repository's earliest commits are from ~2025. The 2018-2024 dates cannot be verified from this repo. They likely refer to the upstream project's history.

**Severity:** MEDIUM — dates may be accurate for the upstream project but are unverifiable here.

**Fix:** Either:
- Verify the dates against upstream (if accessible) and keep them
- Add a note: "These dates reflect the broader ecosystem's evolution; DarkWow's implementation timeline may differ."
- Remove the specific year ranges and just list the primitives in order

---

## LOW Findings (minor omissions — nice to fix)

### L1: QUIC transport not mentioned

The P2P network also supports QUIC transport ([src/net/transport/quic.rs](src/net/transport/quic.rs)) in addition to TCP/TLS, Tor, and I2P. Not critical to mention in a "for dummies" doc, but the transport list could be more complete.

**Fix:** Optionally add QUIC to the transport list on line 28-30.

---

### L2: NativeToken function list incomplete

The doc's NativeToken row (line 136) says "Consensus-first native token for block rewards and fees." The actual functions are:
- `FeeV1` (0x00)
- `MintV1` (0x01) — **disabled**
- `BurnV1` (0x02)
- `TransferV1` (0x03)
- `SpendV1` (0x04)
- `PoWRewardV1` (0x05)

([src/contract/native_token/src/lib.rs:57-64](src/contract/native_token/src/lib.rs#L57-L64))

The description is accurate at a summary level but doesn't mention SpendV1 (which is distinct from TransferV1 + BurnV1).

**Fix:** Low priority — the summary is fine for a non-technical audience.

---

### L3: Sparse Merkle Tree not mentioned

The doc mentions Merkle trees for coin membership (lines 56-60) but doesn't mention the Sparse Merkle Tree (Poseidon-based, depth 255) used for general contract state storage ([src/sdk/src/crypto/smt/mod.rs:83-93](src/sdk/src/crypto/smt/mod.rs#L83-L93)). This is a meaningful architectural component.

**Fix:** Optionally add a sentence about SMT for contract state, or leave it out (this is "for dummies" after all).

---

### L4: Bridge Protocol status understated

**Claim in doc (line 168):** "2024+ | Bridge Protocol (draft) | Cross-chain anonymity"

**Actual status:** [src/contract/bridge/README.md:729](src/contract/bridge/README.md#L729) describes the bridge as "Partial MVP" with critical gaps: no external block header verification, no light client integration, no deposit finality. There are 6063 lines of Rust plus ZK circuits, relayer binaries for XMR/ZEC/AZT/LTC/ETH, and integration tests. It's more than a "draft" on paper, but not production-ready.

**Fix:** Update to "Bridge Protocol (partial MVP)" or add a footnote about status.

---

## Plan to Close the Gaps

### Phase 1: Fix the three CRITICAL factual errors

This is the minimum bar. These three changes fix statements that are provably wrong:

1. **C1 — Coin commitment formula** (lines 42-48, 75):
   - Replace `C = PoseidonHash(value, nonce)` and `C(s, nonce)` with the actual 7-field formula
   - Show the fields with readable names: `(pub_x, pub_y, value, token_id, spend_hook, user_data, blind)`

2. **C2 — Nullifier formula** (lines 64-65, 77):
   - Replace `H(s)` with `poseidon_hash(secret_key, coin_hash)`
   - Explain WHY the coin_hash binding matters (different nullifier per coin with same key)

3. **C3 — Mint described as user operation** (lines 74-76):
   - Restructure from "Mint → Transfer → Burn" to "Coin Creation (PoW Rewards) → Transfer → Burn"
   - Make it clear MintV1 is disabled and only PoWRewardV1 creates coins

**Files to edit:** Only [doc/src/about/for-dummies.md](doc/src/about/for-dummies.md)
**Estimated effort:** ~30 minutes of precise editing, ~15 lines changed

### Phase 2: Fix the two HIGH misleading claims

4. **H1 — Pedersen/ECC claim** (lines 51-52):
   - Remove or rewrite "no Pedersen commitments, no elliptic curve arithmetic in circuits"
   - Option A (preferred): Remove the sentence entirely — it's an implementation detail that a "for dummies" reader doesn't need
   - Option B: Replace with an honest description of where Pedersen IS used

5. **H2 — I2P "garlic routing"** (line 30):
   - Replace with "via external I2P router (SOCKS5 proxy)"

**Files to edit:** Only [doc/src/about/for-dummies.md](doc/src/about/for-dummies.md)
**Estimated effort:** ~15 minutes

### Phase 3: Fix MEDIUM imprecise descriptions

6. **M1 — Burn reveals more than nullifier** (line 78): Add "and verification data" qualifier
7. **M2 — RLN slashing model** (lines 149-156): Rewrite to describe Shamir Secret Sharing identity slashing
8. **M3 — exec()/apply() naming** (lines 123-125): Optionally note WASM-level names
9. **M4 — History timeline** (lines 160-168): Add caveat about verifiability

**Estimated effort:** ~30 minutes

### Phase 4: Fix LOW minor omissions (optional)

10. L1-L4: Minor additions at author's discretion

**Estimated effort:** ~15 minutes

---

## What Does NOT Need to Change

These claims were verified and are accurate as written:

| Claim | Status |
|-------|--------|
| Layer 1 blockchain | ACCURATE |
| PoW with RandomX algorithm | ACCURATE |
| P2P network with TCP/TLS, Tor | ACCURATE |
| Poseidon hash for commitments (corrected formula pending) | ACCURATE in spirit |
| Merkle trees for coin membership proofs | ACCURATE |
| Nullifiers for double-spend prevention (corrected formula pending) | ACCURATE in spirit |
| Halo 2 proof system, no trusted setup | ACCURATE |
| zkVM executes contracts and produces proofs | ACCURATE |
| zkas assembly language / compiler | ACCURATE |
| Smart contract exec/apply phase separation | ACCURATE |
| NativeToken for block rewards and fees | ACCURATE |
| PromissoryNote with hidden token IDs | ACCURATE |
| DAO Escrow anonymous voting, hidden treasuries | ACCURATE |
| Deployooor deploys WASM contracts | ACCURATE |
| RLN spam prevention (corrected slashing model pending) | ACCURATE in spirit |
| P2P messaging with eventual consistency | ACCURATE |
| DarkIRC censorship-resistant messaging | ACCURATE |
| Bridge Protocol in development | ACCURATE |
| Terminology reference | ACCURATE |

---

## Implementation Order

Recommended implementation sequence (most damaging first):

1. C1, C2, C3 (Phase 1) — fix outright errors; these are non-negotiable
2. H1, H2 (Phase 2) — fix misleading claims
3. M1, M2 (Phase 3) — clarify imprecise claims
4. M3, M4 (Phase 3) — add caveats
5. L1-L4 (Phase 4) — optional improvements

Each phase is self-contained and can be committed independently.

---

## Cross-References

- [native_token_zk_audit_2026-06-07.md](native_token_zk_audit_2026-06-07.md) — confirms C2 fix is incomplete in native_token (metadata ↔ circuit mismatch)
- [security_audit_2026-06-05.md](security_audit_2026-06-05.md) — C2 and C3 in that audit correspond: FeeV1 no value constraint (C2) and MintV1 no authority (C3)
- The bridge README at [src/contract/bridge/README.md](src/contract/bridge/README.md#L729) documents "Partial MVP" status — relevant to L4

---

## Implementation Status (2026-06-10)

All 13 findings have been fixed in [doc/src/about/for-dummies.md](doc/src/about/for-dummies.md):

**Phase 1 (CRITICAL):** ✅ Complete
- C1: Coin commitment description updated to reference 7-field structure
- C2: Nullifier formula corrected to `poseidon_hash(secret_key, coin_hash)` in Transfer step
- C3: "Mint-Burn Scheme" renamed to "Coin Lifecycle"; Mint replaced with "Coin Creation" noting PoW rewards only

**Phase 2 (HIGH):** ✅ Complete
- H1: Removed false "no Pedersen commitments, no elliptic curve arithmetic" claim; replaced with accurate 7-field commitment description
- H2: I2P description changed from "garlic routing" to "via external I2P router (SOCKS5 proxy)"

**Phase 3 (MEDIUM):** ✅ Complete
- M1: Burn step updated to "revealing its nullifier along with verification data"
- M2: RLN section rewritten to describe Shamir Secret Sharing identity slashing (not financial)
- M3: Added WASM-level note: `__entrypoint` / `__update` vs runtime `exec()` / `apply()`
- M4: Added caveat above History table about broader ecosystem timeline

**Phase 4 (LOW):** ✅ Complete
- L1: Added QUIC transport to P2P network list
- L4: Bridge status updated from "draft" to "partial MVP"
- L2, L3: Skipped — NativeToken function list and SMT are unnecessary detail for a "for dummies" doc
