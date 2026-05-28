# AuthMint Capability: HAZOP and Red Team Analysis

> **HISTORICAL DOCUMENT — May 2026**
>
> The two-step AuthTokenMintV1 + MintV1 auth model analyzed in this document has been
> removed. The replacement is a single-step MintV1 that proves knowledge of the
> backing secret directly against the stored `token_auth_parent` commitment.
> See [promissory_note.md](promissory_note.md) for current documentation.
>
> This analysis is preserved for archival reference. The o-cap principles discussed
> here informed the current design.

## The AuthMint Capability Model

`auth_mint` is PromissoryNote's mechanism for authorizing token creation. It is an **implicit object-capability**: possession of the `mint_secret` scalar grants the capability to authorize minting for a specific `token_id`. The capability is exercised through a ZK proof that reveals nothing about the secret — only a nullifier (one-shot public trace) and the derived public key.

This is fundamentally a **capability to spend** — the authority proves they can authorize the creation of new coins. The entire token supply of any non-native asset depends on this capability.

### Full Lifecycle

```
TokenMintV1 (opcode 0x00)
│   mint_secret chosen by token creator
│   token_auth_parent = poseidon_hash(mint_secret)     ← PUBLIC input
│   token_id = poseidon_hash(token_auth_parent, user_data, blind)
│   token_id added to registry Merkle tree
│   initial coin minted
│
├─► AuthTokenMintV1 (opcode 0x01)   ← CAPABILITY EXERCISE
│   │  ZK proof: "I know mint_secret such that
│   │             poseidon_hash(mint_secret) == mint_public AND
│   │             token_id is in registry (Merkle proof)"
│   │  nullifier = poseidon_hash(mint_secret, token_id)
│   │  nullifier written to SMT     ← one-shot, never removed
│   │
│   └─► MintV1 (opcode 0x02)        ← CAPABILITY REDEEM (unlimited times)
│       │  ZK proof constrains auth_nullifier as public input
│       │  On-chain check: nullifier exists in SMT (binary gate)
│       │  On-chain check: token_registry_root == current root
│       │  On-chain check: coin doesn't already exist
│       │  New coin added to coins tree
│       │
│       └─► Coin lifecycle
│           Burn: prove coin_secret → nullifier = poseidon_hash(coin_secret, coin)
│           Transfer: burn + mint to new public key
│           Authority has NO special post-mint powers over coins
```

### Capability Generation

The capability originates in **TokenMintV1** ([token_mint_v1.zk](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/proof/token_mint_v1.zk)):

```
token_auth_parent = poseidon_hash(mint_secret)    // public — links authority to token
token_id = poseidon_hash(token_auth_parent, user_data, blind)   // binds authority into token ID
```

The `mint_secret` is chosen by the token creator. It is never transmitted or stored on-chain — it exists only in the authority's possession. The `token_auth_parent` is a public input to the TokenMintV1 circuit, creating a permanent on-chain record of which authority controls the token.

### Capability Exercise

**AuthTokenMintV1** ([auth_token_mint_v1.zk](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/proof/auth_token_mint_v1.zk)) proves knowledge of `mint_secret`:

```
mint_public = poseidon_hash(mint_secret)           // public input
nullifier = poseidon_hash(mint_secret, token_id)   // public input, one-shot
root = merkle_root(leaf_pos, path, token_id)       // public input, proves registry membership
```

The entrypoint ([entrypoint/mod.rs:436-461](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/src/entrypoint/mod.rs#L436-L461)):
1. Verifies `token_id` exists in the token registry (line 445)
2. Verifies the nullifier has NOT already been spent (line 453) — this is the one-shot check
3. Writes the nullifier to the SMT via `apply_auth_token_mint` (line 690)

The nullifier is **deterministic**: `poseidon_hash(mint_secret, token_id)` is always the same for the same inputs. This means each `(mint_secret, token_id)` pair can only be exercised once — the SMT duplicate check rejects subsequent attempts.

### Capability Revocation

**The auth_mint capability cannot be revoked.** Once a nullifier is written to the SMT, no mechanism removes it. The `mint_secret` holder's authority is permanent for that `token_id`. If the secret is compromised, the only mitigation is to create a new `token_id` and migrate all holders — a catastrophic UX event for a stablecoin or wrapped asset.

There is no:
- Nullifier deletion or expiry
- mint_secret rotation
- Multi-sig threshold for AuthTokenMintV1
- Timelock or delay between AuthTokenMintV1 and MintV1

---

## HAZID — Hazard Identification

| ID | Hazard | Severity | Category |
|----|--------|----------|----------|
| H1 | Authority mints all supply to own key — token is permissioned in practice | Low | Trust |
| H2 | Authority mints dust to grief holders (UTXO bloat) | Low | Availability |
| H3 | Authority mints once then loses/compromises mint_secret — future liquidity requires new token_id | Medium | Key mgmt |
| H4 | mint_secret compromise — attacker gains permanent mint authority | **High** | Key mgmt |
| H5 | token_auth_parent revealed publicly — authority identity is permanently linkable | Low | Privacy |
| H6 | No secret rotation — suspected compromise has no recovery path | **High** | Key mgmt |
| H7 | Deterministic nullifier enforces one AuthTokenMintV1 per (secret, token_id) — multi-mint requires secret management | Medium | Usability |
| H8 | No per-token supply cap — authority can inflate arbitrarily | Medium | Economic |
| H9 | token_registry_root check uses current root — registry change between Auth and Mint breaks mint | Low | Liveness |
| H10 | No timelock between Auth and Mint — authority can front-run holder expectations | Low | Timing |
| H11 | One AuthTokenMintV1 authorizes **unlimited** MintV1 calls — the nullifier is a binary gate, not a counter | **Medium** | Design |

---

## HAZOP — Guideword Analysis

Standard process-industry HAZOP applied to the `AuthTokenMintV1 → MintV1` transition.

| Guideword | Deviation | Finding |
|-----------|-----------|---------|
| **NO** | AuthTokenMintV1 never called | MintV1 fails: nullifier leaf is zero → `AuthProofInvalid`. Safe. |
| **NO** | Nullifier not written to SMT | Same as above. Nullifier is the gate. Safe. |
| **MORE** | AuthTokenMintV1 called twice with same (secret, token_id) | Second call: SMT leaf is non-zero → `DuplicateNullifier`. Safe. |
| **MORE** | Multiple MintV1 calls with same auth nullifier | **No check prevents this.** Entrypoint only verifies nullifier exists, not how many times it was used. MintV1 creates different coins with the same `auth_nullifier` public input — one AuthTokenMintV1 authorizes unlimited MintV1 calls. |
| **MORE** | Authority mints unlimited supply | No supply cap. Valid ZK proof + valid nullifier = mint succeeds. Economic trust assumption. |
| **LESS** | Nullifier written but MintV1 never called | Capability wasted — one-shot nullifier burned with no coin created. Authority must use a different secret (or different token_id) for next mint. |
| **AS WELL AS** | Authority knows both mint_secret and recipient's coin_secret | Authority can mint to recipient then burn the coin. Recipient never actually holds value the authority can't destroy. |
| **PART OF** | TokenMintV1 done, AuthTokenMintV1 done, but no MintV1 | Nullifier spent, token registered, but supply unchanged. The capability was exercised but never redeemed. |
| **REVERSE** | MintV1 before AuthTokenMintV1 | Nullifier doesn't exist in SMT → `AuthProofInvalid`. Safe. |
| **OTHER THAN** | Non-PromissoryNote contract tries to call AuthTokenMintV1 | WASM dispatch is contract-local. Contract A cannot call Contract B's functions directly. Safe. |
| **EARLY** | MintV1 uses stale token_registry_root | Entrypoint compares `params.auth_proof.token_registry_root` against current on-chain root. Stale proof → `TokenNotRegistered`. Safe. |
| **LATE** | MintV1 called arbitrarily long after AuthTokenMintV1 | Nullifier persists forever. No expiry. Could be used years later. Feature or bug depending on context. |
| **BEFORE** | AuthTokenMintV1 without prior TokenMintV1 | `token_id` not in registry → `TokenNotRegistered`. Safe. |
| **AFTER** | AuthTokenMintV1 after token deregistration | Token registry is append-only. No deregistration exists. The authority is permanent. |

---

## Red Team vs Blue Team Exercise

**Setting:** PromissoryNote is deployed. Alice is a stablecoin issuer (holds `mint_secret` for `token_id_A`). Bob is a stablecoin holder. Mallory, Sybil, Eve, and Olivia are adversaries with different capabilities and goals.

**Blue Team (Protocol):** The auth_mint capability model ensures only the mint_secret holder can authorize mints. Coins are owned by their recipient's public key (embedded in the Poseidon coin commitment). The authority has no special post-mint powers — no freeze, no clawback, no blacklist. Token holders verify the authority via the publicly-visible `token_auth_parent`.

---

### Mallory — Malicious Authority Insider

Mallory **is** the token authority. She holds `mint_secret` legitimately. Her goal is to extract value from token holders or destroy the token's utility.

#### Attack M1: The Supply Bomb

Mallory calls `AuthTokenMintV1` once, then `MintV1` in a loop with different coin attributes, minting `MAX_U64` tokens to her own keys. She dumps the entire supply on the market.

**Blue Team response:** Supply inflation is visible on-chain — every mint creates a new coin in the coins Merkle tree. The market observes the inflation and prices it in. This is an **economic trust assumption**, not a technical vulnerability. The protocol correctly enforces that only Mallory (the mint_secret holder) can authorize mints. The protocol does not, and cannot, enforce that Mallory acts in token holders' interests.

**Verdict:** Accepted risk. Mitigation (supply caps, multi-sig mint authorization) could be added in future protocol versions (see Recommendations).

#### Attack M2: The Hostage Coin

Mallory mints 1000 USDC to Bob's public key. She then contacts Bob: "Pay me 0.5 ETH or I'll destroy your coins." She claims she can burn them because she's the token authority.

**Blue Team response:** Mallory **cannot** burn Bob's coins. The Burn circuit ([burn_v1.zk](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/proof/burn_v1.zk)) derives the nullifier as `poseidon_hash(coin_secret, coin)`. Mallory doesn't know Bob's `coin_secret` — only Bob does. The token authority has zero post-mint control over minted coins. The coin commitment is `poseidon_hash(recipient_pub, value, token_id, spend_hook, user_data, blind)` — Mallory knows the `token_id` but not the `recipient_pub`'s preimage.

**Verdict:** Attack fails. The o-cap model correctly isolates mint authority from coin ownership.

#### Attack M3: The Dust Storm

Mallory mints 1-token dust coins to 10,000 random addresses, bloating the UTXO set and increasing Merkle tree depth for all holders.

**Blue Team response:** Mallory pays transaction fees for each mint. At current fee levels, this is economically irrational for anything beyond a nuisance. There is no per-address or per-block rate limit on mints — this is a deferred architectural concern (see safety.md, "Rate Limiting as Defense-in-Depth").

**Verdict:** Low-impact nuisance. Economic disincentive from fees provides partial mitigation.

#### Attack M4: mint_secret Cross-Linking

Mallory creates three tokens using the same `mint_secret`. The `token_auth_parent` is identical for all three, making the linkage trivially observable.

**Blue Team response:** This is by design. The `token_auth_parent` is a public input so holders can verify which authority controls their token. Using the same secret across tokens creates a public link — this is the authority's choice. The privacy model protects **holders**, not authorities.

**Verdict:** Not a vulnerability. Documented design choice.

---

### Sybil — External Attacker, No Authority

Sybil has no special access. She observes the blockchain and can submit transactions. Her goal is to mint tokens she's not authorized to mint, or exploit the auth_mint mechanism for profit.

#### Attack S1: Forged Authorization

Sybil observes `token_auth_parent` and `token_id` on-chain. She tries to call `AuthTokenMintV1` with a fabricated proof.

**Blue Team response:** The `auth_token_mint_v1.zk` circuit constrains `mint_public = poseidon_hash(mint_secret)`. Sybil cannot generate a valid ZK proof for a `mint_secret` she doesn't know — that would require a Poseidon preimage attack (2^255 security). The WASM runtime rejects proofs that don't verify against the circuit's verification key.

**Verdict:** Attack fails. Cryptographic security holds.

#### Attack S2: Nullifier Replay — The Unlimited Mint Loophole

Sybil observes a valid `AuthTokenMintV1` transaction from Alice. The nullifier `N = poseidon_hash(mint_secret, token_id)` is now public. Sybil constructs her own `MintV1` call, referencing Alice's nullifier `N` as the `auth_nullifier` public input, but with her own coin attributes (minting to her own public key).

**Let's trace the on-chain checks:**

1. `MintV1` entrypoint ([line 502](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/src/entrypoint/mod.rs#L502)): `smt.get_leaf(&params.auth_proof.nullifier.inner()) == pallas::Base::zero()` → **PASSES**. The nullifier was written by Alice's AuthTokenMintV1, so it's non-zero.

2. Coin uniqueness check ([line 477](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/src/entrypoint/mod.rs#L477)): `db_contains_key(coins_db, &serialize(&params.coin))` → **PASSES**. Sybil's coin (with her public key) is different from any coin Alice minted.

3. Token registry root check ([line 494](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/src/entrypoint/mod.rs#L494)): **PASSES** if the registry root hasn't changed.

4. ZK proof verification: The Mint_V1 circuit constrains `auth_nullifier` and `coin` as public inputs. Sybil generates a valid proof using Alice's `N` as `auth_nullifier` and her own coin attributes → **PASSES**.

**Sybil successfully mints tokens using Alice's authorization.** And she can do this repeatedly — every MintV1 call with the same `N` and different coin attributes produces a new, valid mint.

**Blue Team response:** This is the finding from the HAZOP — **one AuthTokenMintV1 authorizes unlimited MintV1 calls.** The nullifier is a binary gate ("auth happened"), not a counter ("auth used N times"). There is no on-chain state tracking how many mints have consumed a given auth nullifier.

**Verdict:** **Real finding.** Severity: Medium. Exploitation requires observing a valid AuthTokenMintV1 (which may be in the same transaction as the legitimate MintV1, making front-running possible but not silent theft). The legitimate authority would notice unexpected supply inflation.

**Mitigation:** Add a per-nullifier mint counter in the SMT. MintV1 checks `counter < max_mints_per_auth` and increments. Or, restructure the nullifier to be consumed (deleted or toggled) on first MintV1, enforcing exactly one mint per auth.

#### Attack S3: Stale Root Replay

Sybil obtains a valid MintV1 proof from an old block, before a new token was added to the registry. She tries to replay it.

**Blue Team response:** The entrypoint checks `params.auth_proof.token_registry_root != current_root` ([line 494](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/src/entrypoint/mod.rs#L494)). After any new token is registered, the root changes, and old proofs are invalidated.

**Verdict:** Attack fails. Root check prevents cross-era replays.

---

### Eve — Passive Eavesdropper

Eve only observes the blockchain. She never submits transactions. Her goal is to extract information.

#### Attack E1: Authority Fingerprinting

Eve observes `token_auth_parent` in every `TokenMintV1` call. She builds a mapping of `token_auth_parent → [token_ids]`, linking every token to its authority. She can determine exactly how many token types each authority controls.

**Blue Team response:** `token_auth_parent` is intentionally public. The privacy model protects coin holders (who can transfer and burn without revealing their identity), not token authorities. Authorities are expected to be known entities (stablecoin issuers, bridges).

**Verdict:** Not a vulnerability. By design.

#### Attack E2: Supply Reconnaissance

Eve counts the number of `MintV1` calls referencing each `token_id`. She computes the exact circulating supply of every token.

**Blue Team response:** Mint events are inherently observable — each creates a new coin in the public Merkle tree. The privacy model protects **which addresses hold which coins**, not the existence of coins or their aggregate supply. Transparent supply is required for stablecoin solvency verification.

**Verdict:** Not a vulnerability. Supply transparency is a feature.

#### Attack E3: Nullifier Correlation Across Tokens

Eve observes that two different `token_id`s have `token_auth_parent` values that, when hashed together, produce a recognizable pattern. She attempts to correlate authorities across tokens.

**Blue Team response:** The `token_auth_parent` is directly visible, making correlation trivial — no cryptanalysis needed. Same authority, same `token_auth_parent`. This is Eve's Attack E1.

**Verdict:** Same as E1. By design.

---

### Olivia — Oracle Manipulator

Olivia controls or influences an external data source that a bridge or stablecoin contract relies on to trigger `AuthTokenMintV1`.

#### Attack O1: Bridge Oracle Manipulation

A bridge contract monitors an external chain for deposit events. When a deposit is confirmed, the bridge's relayer calls `AuthTokenMintV1` + `MintV1` to issue wrapped tokens. Olivia manipulates the deposit oracle (e.g., a fake Bitcoin block header) to trigger unauthorized mints.

**Blue Team response:** The auth_mint circuit correctly constrains knowledge of `mint_secret` — the ZK proof verifies. The vulnerability is in **who controls the client that calls AuthTokenMintV1**, not in the circuit itself. If the bridge relayer's `mint_secret` is stored in a hot wallet that auto-signs based on oracle data, Olivia's oracle manipulation causes the relayer to authorize mints for deposits that never happened.

This is an oracle and key-management problem, not an auth_mint circuit problem. Mitigations include:
- Multi-sig or threshold signature for AuthTokenMintV1 calls
- Oracle decentralization (M-of-N attestations — deferred architectural concern)
- Human-in-the-loop for large mints

**Verdict:** Out of scope for the auth_mint circuit. The circuit correctly enforces `mint_secret` knowledge. The attack exploits trust in the oracle → relayer → auth_mint pipeline.

---

## Key Findings

### F1: One Auth = Unlimited Mints (Severity: Medium)

The auth nullifier in the SMT is a binary "auth happened" flag. `MintV1` checks that the nullifier **exists** (non-zero leaf), not that it hasn't been **used** before. A single `AuthTokenMintV1` authorizes an unlimited number of `MintV1` calls, each creating a different coin.

The one-shot property applies to `AuthTokenMintV1` itself (deterministic nullifier = can't call AuthTokenMintV1 twice with same secret+token_id), but the authorization it produces is reusable.

**Attack scenario:** Sybil observes Alice's `AuthTokenMintV1` nullifier `N`. Sybil calls `MintV1` referencing `N` with her own coin attributes, minting tokens to herself. She can repeat this indefinitely until the token_registry_root changes.

**Impact:** If the design intent is "one auth = one mint," this is a bug. If the intent is "one auth = permission to mint that token type," the naming and documentation are misleading, and the one-shot nullifier design implies a constraint that doesn't exist.

**Recommendation:** Either:
- Document that one auth authorizes unlimited mints (R2), or
- Add a counter/deletion so MintV1 consumes the nullifier after first use (R1)

### F2: No Secret Rotation (Severity: High)

`mint_secret` is permanent. There is no mechanism to rotate, revoke, or expire it. If a token authority's `mint_secret` is compromised — server breach, insider exfiltration, side-channel attack — the attacker gains **permanent** mint authority for that `token_id`.

The only mitigation is to create a new `token_id` and migrate all holders, which requires every holder to burn their old coins and accept new ones. For a stablecoin with thousands of holders, this is operationally catastrophic.

**Impact:** A single key compromise permanently destroys a token's integrity. No recovery path exists.

**Recommendation:** Add a `RotateMintAuthorityV1` function that updates the `token_auth_parent` in the token registry, gated by a proof of the old `mint_secret` (R3).

### F3: No Supply Cap (Severity: Medium)

`AuthTokenMintV1` + `MintV1` has no per-token supply limit. A malicious or compromised authority can mint arbitrary supply. This is an economic trust assumption: holders trust the authority not to inflate.

For wrapped assets, the bridge's collateral backs the supply — but the bridge contract, not the auth_mint circuit, enforces this. For algorithmic stablecoins, the stability mechanism enforces supply — again, external to auth_mint.

**Impact:** The auth_mint circuit provides no defense-in-depth against supply inflation. Every token depends entirely on its authority's honesty.

**Recommendation:** Add an optional `supply_cap: Option<u64>` to the token registry, checked during `MintV1` against a running supply counter (R4).

### F4: Public Authority Linkage (Severity: Low)

`token_auth_parent` is a public input in `TokenMintV1`. This creates a permanent, publicly-verifiable link between every token and its authority. Anyone can answer "who controls this token?" by comparing `token_auth_parent` to known authority public keys.

**Impact:** Low. This is a deliberate design choice. Token holders need to verify which authority backs their tokens. The privacy model protects holders, not authorities. Authorities are expected to be publicly known entities (issuers, bridges, protocols).

**Recommendation:** Document this transparency property explicitly. It is a feature for auditability, not a privacy leak.

---

## Recommendations

| ID | Recommendation | Priority | Effort | Requires |
|----|---------------|----------|--------|----------|
| R1 | **Fix unlimited mints**: Add a per-nullifier counter or consume the nullifier on first MintV1, enforcing one (or N) mints per auth | P2 | Medium | Consensus change |
| R2 | **Document current behavior**: If the unlimited-mint behavior is intentional, document it in code comments, the auth_mint circuit header, and the capability model | P1 | Low | None |
| R3 | **Secret rotation**: Add `RotateMintAuthorityV1` — updates `token_auth_parent` in registry, gated by ZK proof of old `mint_secret` | P3 | High | New circuit + contract function |
| R4 | **Supply cap**: Add optional `supply_cap: Option<u64>` to token registry, checked and incremented at MintV1 | P3 | Medium | Contract function change |
| R5 | **Timelock**: Add `auth_block_height` to the nullifier record, enforce `current_block - auth_block >= MIN_MINT_DELAY` at MintV1 | P3 | Low | Contract function change |

---

## References

- [auth_token_mint_v1.zk](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/proof/auth_token_mint_v1.zk) — AuthMint ZK circuit
- [mint_v1.zk](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/proof/mint_v1.zk) — Mint ZK circuit
- [token_mint_v1.zk](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/proof/token_mint_v1.zk) — Token creation ZK circuit
- [entrypoint/mod.rs](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/src/entrypoint/mod.rs) — On-chain dispatch and validation
- [model/mod.rs](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/contract/promissory_note/src/model/mod.rs) — State structs and nullifier derivation
- [safety.md](./safety.md) — General contract safety principles
