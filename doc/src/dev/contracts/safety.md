# Smart Contract Inherent Safety

## Fundamentals

Smart contract safety begins with a counterintuitive principle: **the safest code is the code you never write**. Every feature added to a contract is a potential vulnerability. Every code path is an attack surface. Every authorization check is a point of failure.

This is not a statement about code quality — it's about combinatorial complexity. A contract with 3 functions has a manageable set of state transitions to audit. A contract with 12 functions, ACL-gated minting, governance-controlled parameters, and cross-contract child calls has an exponentially larger space of possible interactions to verify.

### The Principle of Minimum Functionality

```
Security ∝ 1 / (features × code_paths × authorization_gates)
```

Three corollaries follow:

1. **Isolate blast radius**: Put the minimum viable logic in the most frequently called contracts. Sophisticated business logic goes in less critical contracts where failures are contained.

2. **Remove, don't gate**: If you think a feature needs an ACL gate to be safe, ask whether the feature should exist at all. Authorization is itself an attack surface — every permission check is a place an attacker can try to bypass.

3. **Separate concerns by failure cost**: A bug in a DEX loses user funds for that trade. A bug in the consensus token loses block rewards for every miner. These are not the same severity.

---

## Design Exemplar: NativeToken vs MoneyV3

DarkWow's token architecture is the concrete expression of these principles. It splits token functionality across two contracts with deliberately asymmetric safety requirements.

### NativeToken: Consensus Safety by Minimum Functionality

NativeToken handles exactly what consensus requires — block rewards, fee payment, and value transfer. It is **deliberately minimal**:

| What it does | What it deliberately omits |
|---|---|
| PoW block rewards (PoWRewardV1) | No token freezing |
| Network fee payment (FeeV1) | No governance coupling |
| Private transfers (Mint/Burn/Transfer) | No multi-token support |
| | No authorization gates |
| | No token registry |
| | No business logic |

Every omission is a security property. No freeze means no freeze-key attack. No governance coupling means no plutocratic takeover of consensus. No multi-token support means no token-ID confusion attacks. No authorization gates means no auth bypass.

The principle: **in consensus-critical code, the feature you don't add is the vulnerability you don't create.** NativeToken is the most frequently called contract in the system. A bug here cascades to every transaction, every block, every miner reward.

### MoneyV3: Minimum Viable Business Logic for DeFi

MoneyV3 carries the business logic that DeFi contracts need to compose — multi-token support, authorization, and cross-contract value verification. It is still minimal by DeFi standards (no AMM, no lending pools, no governance), but it carries more logic than NativeToken because composition demands it:

| What it adds | Why it's needed |
|---|---|
| TokenMintV1 / AuthTokenMintV1 | Permissionless token creation for stablecoins, wrapped assets, LP tokens |
| Multi-token support (token_id) | DEX, lending, yield — all need multiple token types |
| Token registry | Prevents unauthorized minting of unregistered token types |
| BlindOutput_V1 ZK circuit | Proves all output coins are correctly formed (fully private) |
| validate_child_value_commit | Helper for parent contracts to verify child call amounts via commitment comparison |

### Why Not One Contract?

A monolithic token contract that handles both consensus and DeFi creates a single point of failure — a bug in DeFi token logic can break consensus. By separating them:

1. **Failure isolation**: A bug in MoneyV3 cannot break NativeToken. Mining rewards and fees keep flowing regardless.
2. **Different audit postures**: Consensus tokens need maximum security review; DeFi tokens need flexibility. One codebase can't optimize for both.
3. **Independent evolution**: The consensus token can remain frozen while DeFi tokens evolve.
4. **Process safety**: Developers working on DeFi features don't touch consensus-critical code.

```
┌──────────────────────────────────────┐
│           NativeToken                 │
│  Consensus only — block rewards, fees │
│  MINIMAL by design                    │
│  No freezing, no auth, no registry   │
│  Blast radius: ENTIRE NETWORK        │
└──────────────┬───────────────────────┘
               │
┌──────────────┴───────────────────────┐
│           MoneyV3                     │
│  DeFi composition — multi-token, auth │
│  MINIMAL VIABLE for DeFi              │
│  Business logic, cross-contract calls │
│  Blast radius: individual tokens     │
└──────────────┬───────────────────────┘
               │
┌──────────────┴───────────────────────┐
│     DeFi Contracts (DEX, Bridge...)   │
│  Application logic lives here        │
│  Blast radius: individual operations │
└──────────────────────────────────────┘
```

---

## Hardening Lessons: What Can Go Wrong

The following sections describe real vulnerabilities that were identified through security review and their mitigations. Each represents a class of bug that can occur in any contract.

### Lesson 1: Authorization Gaps — The Token Registry

**The vulnerability**: MoneyV3's `MintV1` accepted an `auth_proof` struct containing a nullifier, mint authority public key, and token registry Merkle root. The ZK circuit constrained these values against private witnesses, so the proof verified correctly. But the on-chain contract **never checked that the nullifier was actually spent** — it never verified that `AuthTokenMintV1` had been called first.

Anyone could call `MintV1` with arbitrary `auth_proof` data. As long as the ZK proof verified (which only required knowing a valid auth secret), the mint would succeed. The two-phase authorization model (AuthTokenMintV1 → MintV1) existed on paper but wasn't enforced on-chain.

**The fix**: Three changes close this gap:

1. **Token registry Merkle tree** — `TokenMintV1` stores the `token_id` in an on-chain registry. `MintV1` and `AuthTokenMintV1` both check that the `token_id` exists before proceeding. A token must be registered before it can be minted.

2. **Auth nullifier verification** — `MintV1` performs an SMT lookup on the nullifiers tree to verify that `auth_proof.nullifier` was marked spent by a prior `AuthTokenMintV1` call. The ZK proof alone is not sufficient — on-chain state must corroborate it.

3. **Token registry root tracking** — The registry has its own Merkle tree with historical roots, so `AuthTokenMintV1` can prove token existence against a specific root. This enables light client verification of token authorization.

**The principle**: **ZK proofs constrain witness relationships, not on-chain state.** You must verify that the public inputs to a ZK proof correspond to actual on-chain data. A valid proof of a valid witness does not mean the witness was produced by a valid prior state transition.

### Lesson 2: Cross-Contract Routing — The Opcode Collision

**The vulnerability**: Every parent contract validates child calls by checking `child_call.data[0]` — the function opcode byte. But `0x04` is used by both `MoneyV3::TransferV1` and `Attestation::VerifyClaimV1`. A contract like `labor_market::create_job_v1` checks `data[0] == 0x04` expecting a money transfer, while `labor_market::submit_deliverable_v1` checks `data[0] == 0x04` expecting attestation verification. The contracts never validate `child_call.contract_id`.

If a malicious transaction builder swapped the `contract_id` for a child call, the parent would accept the wrong child function — the opcode matches, but the contract being called is wrong. The WASM runtime dispatches by `contract_id`, so the call goes to the intended contract, but the parent's validation is blind to which contract that is.

**The fix**: Two complementary defenses:

1. **Contract ID validation helper** — `validate_child_contract_id(child_contract_id, expected_contract_id)` provides a standard way for parent contracts to verify the target contract, not just the function code. This should be called after the opcode check.

2. **Value amount validation** — Even with contract_id validation, parent contracts should verify the transfer amount via `validate_child_value_commit` using deterministic blind derivation. The parent computes the expected `value_commit` from its own state and compares it to the child Output's `value_commit` — no plaintext values, no new fields on the shared data model.

**The principle**: **Validate the target, not just the action.** Checking `data[0]` tells you what function will run, but not what contract will run it. Always validate `contract_id` alongside function code, and validate amount/value fields when the child call moves assets.

### Lesson 3: Unproven Outputs — The Blind Output Gap

**The vulnerability**: TransferV1 and OtcSwapV1 outputs had no ZK proof of correct coin formation for fully private outputs. Coins were created client-side and inserted into the transaction without any ZK constraint proving:

- The coin commitment is correctly computed from the attributes
- The value commitment matches the value and blind
- The value is within 64-bit range

The only on-chain check was coin uniqueness — preventing duplicate coin commitments but not proving correct formation. A buggy client could produce malformed coins that would be accepted on-chain.

**The fix**: A new `BlindOutput_V1` ZK circuit (Poseidon-only, no EC) proves correct coin formation for all outputs. The circuit constrains `coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)` and `value_commit = poseidon_hash(value, value_blind)` as public inputs, with a 64-bit range check on value. Every TransferV1 and OtcSwapV1 output uses this single circuit — fully private, no conditional value revelation.

**The principle**: **Every output must have a ZK proof of correct formation.** Client-side construction is not sufficient — the network must be able to verify that every coin commitment and value commitment is correctly computed. Without this, buggy or malicious clients can inject arbitrary coins.

### Lesson 4: Composition Amount Blindness

**The vulnerability**: Before the cross-contract composition refactor, parent contracts called `money_v3::transfer_v1` as a child call but could not verify the transfer amount. The amount was encrypted inside `AeadEncryptedNote` (which the parent can't decrypt), and the `value_commit` was a Poseidon hash (the parent doesn't know the blind). A parent like a bridge or DEX that expects a transfer of 1000 tokens had no way to verify that the child call actually transferred 1000 tokens — only that a TransferV1 call existed.

**The principle**: **A child call's existence is not proof of its correctness.** When a child call moves value, the parent must verify the amount. Relying on the transaction builder to set the right amount is trusting off-chain infrastructure with on-chain correctness.

#### First Attempt: The `public_value` Flakey Pattern

The initial fix added `public_value: Option<u64>` and `public_token_id: Option<pallas::Base>` to the `Output` struct, backed by a `TransferOutput_V1` ZK circuit. Parent contracts read the plaintext `public_value` from the child call data and compared it to the expected amount.

**This was a flakey pattern — it worked but broke the privacy model.** The `Output` is serialized into `ContractCall.data` and stored on-chain. Every composed transfer broadcast its amount in plaintext. The fix solved cross-contract verification by sacrificing the very property the protocol exists to provide.

**Why it passed review**: The `Option<u64>` type made it *look* optional — as if setting it to `None` preserved privacy. But for any composed transfer, it *had* to be `Some(...)`, making privacy conditional and broken for the exact use case cross-contract composition exists to serve. The field was optional in type but mandatory in practice.

#### The Correct Fix: `value_commit` Comparison

The proper fix keeps values fully private by leveraging the cryptographic commitment already present in every `Output`:

1. **The child's `value_commit`** is `poseidon_hash(value, value_blind)` — already part of every Output and already proven correct by the `BlindOutput_V1` ZK proof.

2. **The parent derives `value_blind`** deterministically from its own unique state: `poseidon_hash([expected_value, nullifier])`. No new on-chain fields needed.

3. **The parent recomputes the expected `value_commit`** and checks it equals the child Output's `value_commit`. Equality proves the child coin has the expected value (Poseidon collision resistance).

4. **The transaction builder** derives the same blind and uses it when generating the child's `BlindOutput_V1` proof. No new params. No plaintext values. Fully private.

```rust
// Parent contract computes:
let value_blind = poseidon_hash([
    pallas::Base::from(expected_value),
    nullifier.inner(),
]);
let expected_commit = poseidon_hash([
    pallas::Base::from(expected_value),
    value_blind,
]);

// Checks child output contains a matching value_commit
validate_child_value_commit(&child_call.data, expected_value, value_blind);
```

This eliminates `public_value`, `public_token_id`, and the entire `TransferOutput_V1` circuit. All outputs use the fully-private `BlindOutput_V1` — one circuit, no conditional privacy leakage.

**The meta-lesson**: When you find yourself adding a field that violates a core design constraint to solve a verification problem, the verification itself is the right question — but the answer is almost always to use the cryptographic commitments you already have, not to add plaintext fallbacks.

### Lesson 5: Pubkey as Database Key — Static Identity Queryable On-Chain

**The vulnerability**: Several contracts use raw public keys as database keys for state lookups. This makes identity trivially enumerable on-chain: anyone who knows a pubkey can derive the DB key and enumerate all records for that identity.

Concrete examples from the audit:

| Contract | DB Key Pattern | What It Reveals |
|---|---|---|
| Bridge | `db_set(relayers_db, &serialize(&relayer_pub), ...)` | All relayers and their withdrawal history |
| Identity | `db_set(issuers_db, &serialize(&issuer_id), ...)` | All issuers and their credential types |
| Identity | `issuance_key = serialize(capability_id) + serialize(holder_pub)` | All capabilities issued to a known holder |
| DrainProtection | `vote_key = serialize(&(proposal_id, voter_pubkey))` | Exactly how each voter voted on each proposal |

In the o-cap model, authorization is "what you can prove" not "who you are." Storing a raw pubkey as a DB key inverts this: it makes identity the primary lookup dimension, enabling trivial surveillance of all activity linked to a known public key.

**The fix**: Hash the pubkey through Poseidon before using it as a DB key. For a 32-byte pubkey, split into four u64 chunks to preserve full entropy:

```rust
fn compute_relayer_key(relayer_pub: &[u8; 32]) -> Vec<u8> {
    let mut chunks = [0u64; 4];
    for i in 0..4 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&relayer_pub[i * 8..(i + 1) * 8]);
        chunks[i] = u64::from_le_bytes(bytes);
    }
    let hash = poseidon_hash([
        pallas::Base::from(chunks[0]),
        pallas::Base::from(chunks[1]),
        pallas::Base::from(chunks[2]),
        pallas::Base::from(chunks[3]),
    ]);
    hash.to_repr().to_vec()
}
```

For composite keys (e.g. `(capability_id, holder_pub)`), hash all components together rather than concatenating raw bytes. The resulting key is a Poseidon hash — unlinkable to the original pubkey without knowing the pubkey itself, but still deterministic so the contract can reconstruct it.

**The principle**: **DB keys should be derived via hash, never raw identity material.** The hash preserves look-up capability (anyone who knows the pubkey can recompute the key) but prevents enumeration (without knowing the pubkey first, the key reveals nothing). This is the same principle as address cycling applied to database layout.

### Lesson 6: Signature Key Reuse — Static Identity Link Across Transactions

**The vulnerability**: The `signature_public` field in `Input` structs was documented as a generic "signature public key." Client builders accepted a `Keypair` (the full wallet keypair) or a `signature_secret: SecretKey` without any enforcement that it be ephemeral. If a client reused the wallet-level secret, every transaction from that wallet would share the same `signature_public`, creating a static identity link across all of a user's activity.

This affected two critical token contracts:

| Contract | Field | Client File |
|---|---|---|
| NativeToken (BurnV1) | `Input.signature_public: PublicKey` | `burn_v1.rs` — accepted full `Keypair` |
| NativeToken (FeeV1) | `Input.signature_public: PublicKey` | `fee_v1.rs` — accepted `signature_secret: SecretKey` |
| MoneyV3 (TransferV1) | `signature_public: pallas::Base` | `transfer_v1.rs` — accepted `signature_secret: pallas::Base` |
| MoneyV3 (BurnV1) | `signature_public: pallas::Base` | `burn_v1.rs` — accepted `signature_secret: pallas::Base` |

The `signature_public` is exposed as a public input to the ZK proof and stored on-chain. If it's the wallet's persistent key, every Input from that wallet is trivially linkable.

**The fix**: Three changes that enforce ephemeral derivation at the type and naming level:

1. **Rename the field** to `ephemeral_signature_secret` — the name itself communicates the invariant.
2. **Document the requirement** in the struct definition: "MUST be fresh per transaction, never the wallet secret."
3. **Remove full Keypair from builders** — accept only the individual secrets needed, preventing accidental wallet-key reuse.

```rust
// BEFORE — wallet keypair accepted, nothing prevents reuse
pub struct BurnCallInput {
    pub keypair: Keypair,  // wallet-level identity
    // ...
}

// AFTER — separate secrets, ephemeral enforcement by name
pub struct BurnCallInput {
    /// MUST be fresh per burn — never the wallet secret
    pub ephemeral_signature_secret: SecretKey,
    pub secret: SecretKey,  // coin ownership secret
    // ...
}
```

**The principle**: **Every signature must use an ephemeral key.** The o-cap model requires that each capability consumption (nullifier) be unlinkable to every other. A reused signature public key is a static identity that links all of a user's transactions. The type system and naming conventions should make the invariant impossible to miss.

### Lesson 7: User Data Encoding Identity — Smuggling Public Keys in Opaque Fields

**The vulnerability**: The `user_data` field on coins is designed as opaque private data committed into the coin hash — a place for application-specific metadata that stays behind the commitment. But the stablecoin contract encoded identity material into this field:

```rust
// BEFORE — identity smuggled into opaque field
let sender_pub = poseidon_hash([owner_secret]);
let user_data = poseidon_hash([
    pallas::Base::from(mint_amount),
    stablecoin_token_id,
    sender_pub,  // ← identity derived from owner_secret
]);
```

The `user_data` is committed into the coin hash and passed as a public input to `MoneyV3::MintV1`. While Poseidon-hashed, `poseidon_hash([owner_secret])` is a deterministic function of the owner's secret — it's effectively a public key fingerprint embedded in every mint operation. Anyone who knows (or guesses) the owner_secret can identify all coins minted by that owner.

**The fix**: Use a constant (zero) in place of the identity-derived value:

```rust
// AFTER — no identity material
let user_data = poseidon_hash([
    pallas::Base::from(mint_amount),
    stablecoin_token_id,
    pallas::Base::zero(),  // no identity
]);
```

Authorization is handled by the nullifier (which consumes the position capability), not by embedding identity in auxiliary data. The nullifier already proves "someone who knows the owner_secret authorized this" — encoding the same secret's hash in `user_data` adds no security and undermines privacy.

**The principle**: **Opaque fields must not carry identity material.** If a field is committed into a coin hash or passed as a ZK public input, it is visible on-chain (either directly or through the commitment). Any identity-derived data in these fields creates a linkable fingerprint. Authorization belongs in nullifiers, not in auxiliary data fields.

### Lesson 8: Token ID Carrying Identity Fragments

**The vulnerability**: When creating a new token type in MoneyV3, the stablecoin contract derived `token_auth_parent` — one of the inputs to the token ID computation — from the authority's public key:

```rust
// BEFORE — token ID embeds authority identity fragment
let auth_bytes: [u8; 8] = token_authority_pub[0..8].try_into().unwrap();
let auth_u64 = u64::from_le_bytes(auth_bytes);
let token_auth_parent = pallas::Base::from(auth_u64);
let token_id = poseidon_hash([token_auth_parent, token_user_data, token_blind]);
```

The resulting `token_id` embeds the first 8 bytes of the token authority's public key. Anyone who knows (or suspects) which authority created a token can check: extract the first 8 bytes, recompute the token ID, and see if it matches. Every coin holding this token carries a fingerprint of its creator.

**The fix**: Use a random `token_auth_parent`:

```rust
// AFTER — random, unlinkable to authority
let token_auth_parent = BaseBlind::random(&mut OsRng).inner();
let token_id = poseidon_hash([token_auth_parent, token_user_data, token_blind]);
```

The authority's ability to mint is proven through the AuthTokenMintV1 flow (nullifier + Merkle proof against the token registry), not through the token ID itself. The token ID needs to be unique, not identity-bearing.

**The principle**: **Token IDs must be unlinkable to their authority.** A token's existence and ownership should reveal nothing about who created it. The authority relationship is a capability (proven via nullifier + ZK proof), not an identity (embedded in the token ID). Randomizing all derivation inputs preserves both uniqueness and privacy.

### Lesson 9: Full Keypair in Client Builders — Wallet Secret Leakage

**The vulnerability**: The `PoWRewardCallBuilder` carried the full wallet `Keypair` and serialized the wallet secret into the coin's encrypted note memo:

```rust
// BEFORE — wallet keypair in builder, secret in memo
pub struct PoWRewardCallBuilder {
    pub signature_keypair: Keypair,  // full wallet identity
    // ...
}

// In build():
let note = NativeNote {
    // ...
    memo: serialize(&self.signature_keypair.secret),  // wallet secret bytes
};
```

Two problems: (1) the full wallet keypair in the builder struct invites reuse of the wallet identity for signing (see Lesson 6); (2) the wallet secret is serialized into the note memo — AEAD-encrypted and only decryptable by the recipient, but unnecessary exposure of the wallet's root secret. If the recipient's key is ever compromised, the sender's wallet secret is revealed.

**The fix**: Separate the coin-ownership secret from the ephemeral signature secret, and remove the wallet secret from the memo:

```rust
// AFTER — no wallet keypair, no secret in memo
pub struct PoWRewardCallBuilder {
    pub secret: SecretKey,                       // coin ownership
    pub ephemeral_signature_secret: SecretKey,   // MUST be fresh per reward claim
    // ...
}

// In build():
let note = NativeNote {
    // ...
    memo: vec![],  // no secret leakage
};
```

**The principle**: **Client builders should never carry full wallet keypairs.** Accept only the individual secrets needed for the specific operation. Never serialize wallet secrets into note memos — the note already carries the coin blind and value, which are sufficient for the recipient to spend the coin. The wallet secret should never leave the wallet.



---

## Flakey Patterns: Recognition and Prevention

A **flakey pattern** is a solution that passes functional tests but violates a core architectural invariant. It looks correct in isolation — the code compiles, the tests pass, the immediate problem is solved — but it undermines the very property the system exists to provide. These are the most dangerous bugs because they survive code review and automated testing.

### Anatomy of a Flakey Pattern

Every flakey pattern shares three characteristics:

1. **Solves the immediate problem** — the functional requirement is met. The parent *can* verify the child amount.
2. **Breaks a core invariant** — a non-negotiable design constraint is sacrificed. Privacy is the invariant; plaintext values break it.
3. **Disguises the breakage** — the violation is hidden behind optional types, configurable defaults, or conditional logic that makes it look safe. `Option<u64>` *looks* like privacy is preserved.

### The Warning Signs

When reviewing code, these signals indicate a potential flakey pattern:

| Signal | Example | Why It's Dangerous |
|---|---|---|
| **Plaintext data in privacy structs** | `public_value: Option<u64>` on `Output` | The struct is on-chain; all fields are visible regardless of type wrapping |
| **Optional fields that are mandatory for correctness** | `public_value` must be `Some(...)` for any composed transfer | The "optional" is a lie — the field is required for the primary use case |
| **New ZK circuits that reveal what old ones hid** | `TransferOutput_V1` vs `BlindOutput_V1` | Proves the same thing but with extra public inputs that leak data |
| **Fields added to satisfy one caller's needs** | Bridge needed amount verification → `Output` got `public_value` | One contract's requirement leaked into the shared data model |
| **Type-level safety without invariant enforcement** | `Option<u64>` is type-safe but doesn't enforce privacy | Rust's type system can't check protocol-level invariants |
| **"Backed by a ZK proof" without on-chain verification** | `auth_proof` fields in `MintV1` only ZK-verified | ZK proofs constrain witnesses, not on-chain state (Lesson 1) |
| **Opcode checks without contract ID checks** | `data[0] == 0x04` without validating `contract_id` | Same opcode used by multiple contracts (Lesson 2) |
| **Raw pubkeys as database keys** | `db_set(relayers_db, &serialize(&relayer_pub), ...)` | Enables enumeration of all records for a known identity (Lesson 5) |
| **Shared signature secrets across transactions** | `signature_secret` reused from wallet key | All Inputs from the same wallet share a static identity link (Lesson 6) |
| **Identity material in opaque data fields** | `user_data = poseidon_hash([..., sender_pub])` | Smuggles public key fingerprints through fields meant for private app data (Lesson 7) |
| **Identity fragments in token derivation** | `token_auth_parent = authority_pub[..8]` | Token ID carries a fingerprint of its creator, making all holders linkable (Lesson 8) |
| **Full wallet keypair in client builders** | `signature_keypair: Keypair` on builder structs | Invites wallet secret reuse; may leak secret into serialized data (Lesson 9) |

### The Fix Pattern

Flakey patterns are almost always fixed by the same approach: **use the cryptographic commitments you already have, rather than adding plaintext fallbacks.**

```
FLAKEY:  Add plaintext field + new ZK circuit to prove plaintext matches hidden value
PROPER:  Compare existing commitments using deterministic derivation both sides compute
```

The `value_commit` approach (Lesson 4) exemplifies this: instead of adding `public_value` plus a `TransferOutput_V1` circuit, we use the existing `value_commit` plus deterministic blind derivation. Fewer lines of code, fewer circuits, stronger privacy.

### Audit Heuristic

When auditing for flakey patterns, ask of every field on every on-chain struct:

1. **Is this field visible on-chain?** If yes, what information does it reveal?
2. **Is there a cryptographic commitment already present** that could serve the same purpose without revealing the value?
3. **Is this field "optional" but actually required** for the contract's primary use case?
4. **Was this field added to satisfy a single caller's requirement** rather than the general model?
5. **Does a new ZK circuit reveal more public inputs** than the circuit it replaces or supplements?
6. **Is a raw public key used as a database key?** If yes, replace with `poseidon_hash(pubkey_chunks)` — hash-preserved lookup without identity leakage.
7. **Could a signature public key be reused across transactions?** If the field is named `signature_secret` or `signature_public` without "ephemeral," the naming itself may invite reuse. Rename to `ephemeral_signature_secret`.
8. **Does `user_data` or any opaque field encode identity material?** Grep for `poseidon_hash([owner_secret])` or `sender_pub` in `user_data` derivations. Authorization belongs in nullifiers.
9. **Does a token ID derivation use identity-linked inputs?** Check `token_auth_parent` and `token_id = poseidon_hash(...)` for pubkey fragments. Use random blinds instead.
10. **Does a client builder carry a full `Keypair`?** If yes, replace with individual secrets. The wallet root keypair should never appear in contract client code.

If the answer to any of (3)-(5) is yes, and the answer to (2) is "yes, but we need to know the blind," consider deterministic blind derivation before adding a plaintext field.

If the answer to any of (6)-(10) is yes, the code has an o-cap privacy deviation. Apply the fix pattern from the corresponding lesson.



### Consensus-Critical Contracts

If your contract handles block rewards, fee payment, or any function that the network cannot function without:

- [ ] No governance coupling — no one can vote to change its behavior
- [ ] No authorization gates — no freeze, no ACL, no permissioned minting
- [ ] No multi-token support — single asset, no token-ID confusion possible
- [ ] Minimum functions — if a feature can live in a separate contract, it should
- [ ] Every output has a ZK proof — no client-side-only coin construction
- [ ] Poseidon-only circuits — no EC operations in internal ZK circuits

### DeFi / Application Contracts

If your contract composes with other contracts and handles user funds:

- [ ] Validate `contract_id` on child calls, not just `data[0]`
- [ ] Validate child transfer amounts via `validate_child_value_commit` with deterministic blind derivation
- [ ] Every authorization model has on-chain state backing — ZK proofs alone are not enough
- [ ] Registries exist for any resource that must be "registered before use" (tokens, members, etc.)
- [ ] Nullifier-based replay prevention for all authorization operations
- [ ] Merkle proofs for all existence checks against growing datasets
- [ ] Child call validation happens in the `instruction` phase, before state mutation
- [ ] All database trees are initialized in `init_contract`

### Intentional Transparency vs. Privacy Leaks

Not every plaintext field on a params struct is a flakey pattern. Some amounts MUST be known on-chain for correct contract operation. The distinction:

**Intentional transparency** (keep the plaintext field):
- Bridge withdrawal amounts — cross-chain visibility is inherent; both chains know the amount
- Stablecoin pool totals — global collateral/debt tracking requires known amounts for ratio checks
- DEX order-book prices — market-visible by design; hidden prices would prevent matching
- Fee amounts — network fees are public by protocol design

**Privacy leak** (remove the plaintext field, use commitments):
- Individual transfer amounts on Output structs — use `value_commit` comparison (see Lesson 4)
- Spend amounts on Input structs — use client-side witnesses (see Lesson 3 / RC3)
- Token IDs when already committed — use token_commit comparison

**Heuristic**: If the value updates a global state aggregate that other users depend on (pool totals, market prices, cross-chain proofs), the amount is legitimately public. If the value is only needed by the two counterparties to a transfer, it should stay behind a commitment.

### ZK Circuit Development

- [ ] Is EC required? If this is an internal DarkWow circuit, use Poseidon-only
- [ ] Every output coin has a BlindOutput_V1 ZK proof of correct formation — no conditional privacy leakage
- [ ] Public inputs to the circuit are verified against on-chain state in the entrypoint
- [ ] Range checks on all value fields (64-bit for coin values)
- [ ] Nullifier uniqueness is checked both in the circuit AND in the on-chain nullifiers tree

---

## References

- [NativeToken](./native_token.md) — Consensus token with zero business logic
- [MoneyV3](./money_v3.md) — DeFi token with minimum viable composition logic
- [Standards](./standards.md) — ZK circuit, token, and testing standards
- [Composability](../../contract/composability.md) — Cross-contract child call patterns
- [MoneyV3 Migration](../../contract/money_v3_migration.md) — Architecture rationale for the hard fork that separated NativeToken and MoneyV3
