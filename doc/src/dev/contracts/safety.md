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

## Design Exemplar: NativeToken vs PromissoryNote

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

### PromissoryNote: Minimum Viable Business Logic for DeFi

PromissoryNote carries the business logic that DeFi contracts need to compose — multi-token support, authorization, and cross-contract value verification. It is still minimal by DeFi standards (no AMM, no lending pools, no governance), but it carries more logic than NativeToken because composition demands it:

| What it adds | Why it's needed |
|---|---|
| TokenMintV1 | Permissionless token creation for stablecoins, wrapped assets, LP tokens |
| Multi-token support (token_id) | DEX, lending, yield — all need multiple token types |
| Token registry | Prevents unauthorized minting of unregistered token types |
| BlindOutput_V1 ZK circuit | Proves all output coins are correctly formed (fully private) |
| validate_child_value_commit | Helper for parent contracts to verify child call amounts via commitment comparison |

### Why Not One Contract?

A monolithic token contract that handles both consensus and DeFi creates a single point of failure — a bug in DeFi token logic can break consensus. By separating them:

1. **Failure isolation**: A bug in PromissoryNote cannot break NativeToken. Mining rewards and fees keep flowing regardless.
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
│           PromissoryNote                     │
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

### Lesson 1: Authorization Gaps — The Two-Step Auth Anti-Pattern

**The vulnerability (historical)**: PromissoryNote originally used a two-step auth model (`AuthTokenMintV1` → `MintV1`). `MintV1` accepted an `auth_proof` struct containing a nullifier, but the on-chain contract **never checked that the nullifier was actually spent**. The ZK proof verified correctly, but the prior authorization step wasn't enforced on-chain. Anyone could call `MintV1` with arbitrary `auth_proof` data and mint tokens without ever calling `AuthTokenMintV1`.

**The fix**: The two-step model was removed entirely (May 2026). `AuthTokenMintV1` and `RotateMintAuthorityV1` were deleted. `MintV1` now proves knowledge of the backing secret directly against the stored `token_auth_parent` (backing capability commitment) in a single step. The token registry stores the commitment; `MintV1` proves the prover knows the corresponding secret.

1. **Token registry Merkle tree** — `TokenMintV1` stores the `token_id` and `token_auth_parent` in an on-chain registry. `MintV1` verifies the token exists via Merkle proof against the registry root.

2. **Single-step backing proof** — `MintV1` proves knowledge of `mint_secret` where `token_auth_parent = poseidon_hash(mint_secret)`. No separate auth step, no nullifier to consume. The proof IS the authorization.

3. **No authority to rotate** — Key rotation is the issuer contract's concern, not the token primitive's. PromissoryNote provides mint/burn/transfer; the issuer handles supply caps, timelocks, and key rotation.

**The principle**: **Two-step authorization is an anti-pattern in o-cap systems.** If step 2 requires step 1 to have occurred, step 1's on-chain artifact must be verified in step 2 — creating a fragile chain of state dependencies. The correct pattern is a single-step proof: prove knowledge of the capability secret directly. The proof IS the authorization; there is no prior step to forget to check.

**The deeper pattern**: This is a specific case of a general o-cap failure mode — **authorization by presence rather than authorization by proof-of-knowledge**. A multi-step ACL (register → authorize → execute) creates multiple points of failure where on-chain verification can be skipped. The o-cap model collapses this to a single step: prove you know the secret, and that proof is your authority. This pattern recurs across many contract types — whenever a function requires a prior action, ask whether the prior action can be eliminated by proving knowledge of the capability directly.

### Lesson 2: Cross-Contract Routing — The Opcode Collision

**The vulnerability**: Every parent contract validates child calls by checking `child_call.data[0]` — the function opcode byte. But `0x04` is used by both `PromissoryNote::TransferV1` and `Attestation::VerifyClaimV1`. A contract like `labor_market::create_job_v1` checks `data[0] == 0x04` expecting a money transfer, while `labor_market::submit_deliverable_v1` checks `data[0] == 0x04` expecting attestation verification. The contracts never validate `child_call.contract_id`.

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

**The vulnerability**: Before the cross-contract composition refactor, parent contracts called `promissory_note::transfer_v1` as a child call but could not verify the transfer amount. The amount was encrypted inside `AeadEncryptedNote` (which the parent can't decrypt), and the `value_commit` was a Poseidon hash (the parent doesn't know the blind). A parent like a bridge or DEX that expects a transfer of 1000 tokens had no way to verify that the child call actually transferred 1000 tokens — only that a TransferV1 call existed.

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
| PromissoryNote (TransferV1) | `signature_public: pallas::Base` | `transfer_v1.rs` — accepted `signature_secret: pallas::Base` |
| PromissoryNote (BurnV1) | `signature_public: pallas::Base` | `burn_v1.rs` — accepted `signature_secret: pallas::Base` |

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

The `user_data` is committed into the coin hash and passed as a public input to `PromissoryNote::MintV1`. While Poseidon-hashed, `poseidon_hash([owner_secret])` is a deterministic function of the owner's secret — it's effectively a public key fingerprint embedded in every mint operation. Anyone who knows (or guesses) the owner_secret can identify all coins minted by that owner.

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

**The vulnerability**: When creating a new token type in PromissoryNote, the stablecoin contract derived `token_auth_parent` — one of the inputs to the token ID computation — from the authority's public key:

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

The authority's ability to mint is proven through the MintV1 flow (backing secret proof against the token registry), not through the token ID itself. The token ID needs to be unique, not identity-bearing.

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

### Lesson 10: Capability Descriptors as Living Specification

**The vulnerability**: Capability descriptors — the `descriptor()` functions that declare each contract's actions, required capabilities, and state transitions — were out of sync with the actual contract code. The review found every descriptor had at least one error:

| Contract | Error | Impact |
|---|---|---|
| darkbet_exchange | `Any` expression nested `All` sub-expressions instead of taking `CapabilityId` directly | Descriptor wouldn't compile |
| game_room | `capability_id:` field name instead of `id:` on `CapabilityOutput` | Descriptor wouldn't compile |
| subscription | Same field name error; `SubscribeV1` had wrong function_id | Wrong function mapped |
| darkbet_exchange | Missing `ClaimWinnings` action entirely | Incomplete interface |

These are not just cosmetic — capability descriptors serve as the machine-readable interface specification. The host runtime uses them to verify that transactions only call declared functions with the correct capabilities. A descriptor that compiles but has wrong function_ids silently allows calls the contract doesn't handle, or rejects calls it should accept. A descriptor missing produce/consume transitions means the capability state machine drifts from reality — capabilities that should be consumed persist, and capabilities that should be produced never appear.

**The fix**: Every descriptor was corrected to match the actual entrypoint dispatch table: function_ids verified, `consume`/`produces` transitions mirroring actual state changes, expression types matching the SDK API. The gold standard reference is `darktoshi_dice/src/capability.rs` — it is the only descriptor that was complete and correct from initial implementation.

**The principle**: **Capability descriptors are code, not documentation.** They must be treated with the same rigor as the entrypoint dispatch table. A function added to the entrypoint without a corresponding descriptor action is invisible to the capability system — it can be called without capability checks. A function in the descriptor that doesn't exist in the entrypoint is a dead path that wastes verification cycles. The descriptor and the dispatch table must be kept in lockstep. When you add a function to a contract, the capability descriptor update is not optional — it is part of the function's implementation.

### Lesson 11: Spend Hook Callback Safety — Trust but Verify

**The vulnerability**: The spend_hook callback mechanism delivers a `BurnSpendHookPayload`
to the target contract's `__spend_hook` export. The payload includes `caller_contract_id`
— the PN contract that initiated the burn. If the receiver trusts this field without
verification, a malicious contract could forge callbacks by calling `emit_spend_hook`
directly (if it has access to the host function) or by deploying a fake PN contract.

**The fix**: Three mandatory checks in every spend_hook receiver:

1. **Verify the caller**: Check `payload.caller_contract_id == expected_pn_contract_id`.
   Store the expected PN contract ID during `init_contract` and retrieve it in the handler.
2. **Track nullifiers**: Store every processed nullifier in a dedicated DB tree. Check for
   duplicates before processing. A burn can be replayed if nullifiers aren't tracked.
3. **Keep handlers deterministic**: The callback runs in the same overlay as the burn.
   Don't depend on oracle prices, cross-chain state, or any external data that could
   change between proof generation and callback execution.

```rust
fn process_spend_hook(contract_id: ContractId, instruction_data: &[u8]) -> ContractResult {
    let payload: BurnSpendHookPayload = deserialize(instruction_data)?;

    // 1. Verify the caller
    let expected_pn = get_stored_pn_contract_id()?;
    if payload.caller_contract_id != expected_pn {
        return Err(ContractError::InvalidCaller);
    }

    // 2. Replay protection
    for nullifier in &payload.nullifiers {
        if nullifier_already_processed(nullifier)? {
            return Err(ContractError::ReplayDetected);
        }
    }

    // 3. Build update (deterministic — no external data)
    let update = SpendHookCallbackUpdateV1 { /* ... */ };
    set_return_data(&serialize(&update))
}
```

**The principle**: **Spend_hook callbacks are capability exercise, not trusted messages.**
The callback proves that coins were burned — it does not prove who initiated the burn
or that the payload is authentic. Always verify `caller_contract_id` against a stored
expected value. Always track nullifiers for replay protection. The handler must be
deterministic: same input always produces same output, with no dependency on state
that could change between overlay creation and callback execution.

### Lesson 12: Compiler-Synthesizer Drift — When the Circuit Compiler Outpaces the VM

**The vulnerability**: The zkas compiler (`bin/zkas/`) and the ZK circuit synthesizer
(`src/zk/vm.rs`) are two halves of a single pipeline — the compiler produces `.zk.bin`
artifacts, the synthesizer consumes them. When the compiler gains a new feature but the
synthesizer isn't updated to match, every circuit recompiled with the new compiler
breaks at keygen time.

This happened when the zkas compiler added `Base` as a new constant type (commit
`652c2b779`). Circuits recompiled with the new compiler included `Base <name>`
constants in their binaries. The synthesizer only handled four magic constant names
(`VALUE_COMMIT_VALUE`, `VALUE_COMMIT_RANDOM`, `VALUE_COMMIT_RANDOM_BASE`,
`NULLIFIER_K`). Any circuit with a `Base` constant that didn't match those four names
crashed at `ProvingKey::build` with `Err(Synthesis)`.

The symptom was generic — a `Synthesis` error during keygen, one of 16+ possible
failure sites in the synthesizer. The root cause was upstream: a compiler change
that wasn't mirrored in the VM. The bridge's `deposit_v1.zk` declared `Base
commitment` as a constant — a documentation-only declaration with no functional
purpose (the actual public input binding was already handled by
`constrain_instance`). Removing it fixed the immediate issue, but the systemic
problem remains: the compiler and VM can drift.

**Diagnostic procedure** — when a `Synthesis` error hits at `proof.rs:113`
(`ProvingKey::build` → `keygen_vk`), follow these steps in order:

1. **Identify which circuit fails.** Add `eprintln!` before each `ProvingKey::build`
   call in the harness `spawn()`. The first one that doesn't print "OK" is the failing
   circuit. This narrows the problem from "the contract" to "this specific .zk.bin."

2. **Inspect the `.zk.bin` constants.** Decode the binary in test code with
   `ZkBinary::decode(&bytes, false)` and print `zkbin.constants` — a
   `Vec<(VarType, String)>`. If any entry has a name not in the set
   `{VALUE_COMMIT_VALUE, VALUE_COMMIT_RANDOM, VALUE_COMMIT_RANDOM_BASE, NULLIFIER_K}`,
   the VM synthesizer cannot handle it.

3. **Check if the constant is actually used.** Grep the `.zk` source for the constant
   name. If no opcode references it, it's a documentation-only declaration — remove it
   from the constant block and recompile the `.zk.bin`. `constrain_instance` already
   binds public inputs; the `Base` declaration was redundant.

4. **If the constant IS used by an opcode**, a VM synthesizer change is needed. The
   synthesizer must learn to handle the new constant type. This requires explicit
   permission — the VM is security-critical infrastructure (see `vm-off-limits` rule).

5. **Cross-check: was the circuit recently recompiled?** Run `git log --oneline -1 --
   <circuit>.zk.bin`. If the recompile coincides with a zkas compiler change (grep
   `git log` for "zkas" or "compiler"), the binary format likely changed. Compare
   the old and new `.zk.bin` constants to confirm what was added.

**Preventive check**: After any zkas compiler change, recompile a known-good
circuit (e.g. `burn_v1.zk`) and verify `ProvingKey::build` succeeds. This catches
compiler-VM drift before it reaches production circuits.

**The principle**: **The compiler and synthesizer are a matched pair — updating one
requires verifying the other.** Every new zkas feature must be accompanied by
synthesizer support before any circuit uses it in production. The `.zk.bin` format
is the contract between them; when the format changes, both sides must change
together. When the gap is found after the fact, work backwards from the binary
to the source — decode the constants, trace the names, check which ones the VM
can't handle. The answer is in what the compiler emitted, not in what the VM
should accept.

### Lesson 13: Hash Function Impedance Mismatch — Circuit Merkle vs SDK Merkle

**The vulnerability**: ZK circuits and the SDK use different hash functions for Merkle
tree operations. The circuit's `merkle_root` opcode uses the Orchard `MerkleChip`,
which hashes with **Sinsemilla** (via `OrchardHashDomains::MerkleCrh`). The SDK's
`MerkleNode::combine` uses **Poseidon**. For the same leaf and authentication path,
these produce different roots.

This creates a hard barrier to testing: you cannot generate a valid Merkle proof
for a Sinsemilla-based circuit using SDK utilities. A test that builds a Poseidon
Merkle tree and passes the root to a Sinsemilla circuit will always fail — the
circuit computes a different root and `constrain_equal_base` rejects it. Valid
test data for circuits that use `merkle_root` requires either:
(a) external chain integration (the bridge needs Merkle proofs from Ethereum,
Monero, etc.), or
(b) a Sinsemilla-compatible Merkle tree in the SDK that matches the circuit's
hash function.

The SDK's `MerkleNode` and the circuit's `merkle_root` opcode implement the same
*algorithm* (binary Merkle tree with position-dependent hashing) but different
*hash functions*. They are algorithmically compatible but cryptographically
incompatible — the same inputs produce different outputs.

**The detection heuristic**: If a test provides Merkle data computed with the SDK's
MerkleTree and the circuit rejects it with a constraint failure, check which hash
function each side uses. The ZK opcode documentation (`doc/src/arch/zk/opcodes.md`)
lists the hash function for each opcode.

**The principle**: **Merkle trees used in ZK circuits must have a matching off-circuit
implementation.** For every hash function used in a circuit opcode, there must be a
corresponding utility in the SDK that produces the same output for the same input.
When the SDK and circuit diverge (Sinsemilla vs Poseidon), the gap becomes a hard
dependency on external infrastructure before the circuit can be meaningfully tested.

### Lesson 14: Input Reuse Attacks — Bind Nullifiers to Operation Context

**The vulnerability**: A nullifier proves a coin is spent — the holder knows the
secret and the coin hasn't been double-spent. But if the nullifier isn't bound
to the specific *operation* or *context* in which it's used, the same coin can
be "spent" across multiple independent operations.

The DAO proposal input reuse exploit (upstream commit `1814306ed`) is the
canonical example. A DAO member submits a proposal backed by coin inputs to
satisfy the proposer threshold. The nullifier proves the coins are valid, but
without context binding, the same coins can be submitted again for a different
proposal — bypassing the threshold because the nullifiers aren't linked to
any specific proposal.

**The fix**: Bind the input nullifier to the operation's unique identifier:

```
input_nullifier = poseidon_hash(coin_nullifier, operation_bulla)
```

Each (coin, operation) pair produces a unique on-chain artifact. The entrypoint
checks that `input_nullifier` hasn't been seen before. A reused coin with a
different operation bulla produces a different `input_nullifier`, which passes
the uniqueness check — but the RAW `coin_nullifier` is spent in the first
proposal and can't be respent. The ZK circuit constrains that both the
`input_nullifier` and the `operation_bulla` are correctly derived and reveals
them as public instances.

**Detection**: For every contract function that accepts coin inputs via
nullifiers, ask: is the nullifier unique to this operation, or could the
same nullifier be submitted in a different operation context? If the
nullifier isn't bound to the operation, the same economic stake can be
reused across multiple independent actions.

**The principle**: **Every input nullifier must be bound to the operation it
authorizes.** A nullifier that proves "I spent coin X" without saying "for
purpose Y" allows coin X to be spent for purposes Y, Z, and W simultaneously.
The fix is one line in the ZK circuit — `poseidon_hash(nullifier, context_bulla)`
— but the absence of that line is a protocol-level vulnerability.

**Audit heuristic**: Grep for `constrain_instance` calls in `.zk` circuits
that handle coin inputs. If a nullifier is constrained as an instance but no
operation-specific identifier is also constrained, the nullifier isn't context-bound.

**DarkWow audit result (2026-06-03):** All 27 contracts checked. Every
nullifier-bearing circuit already binds to an operation-specific context
(proposal ID, swap ID, job ID, auction ID, position commitment, or coin
identity). No fixes required.

### Lesson 15: Parent Call Validation — Validate Contract ID + Function Code

**The vulnerability**: A contract function designed to be called ONLY as a child
of a specific parent call validates the parent's *opcode* (`data[0]`) but not
the parent's *contract ID*. Since opcodes are not globally unique (`0x04` is
used by multiple contracts), an attacker can swap the target contract while
keeping the opcode the same.

The DAO `auth_xfer` exploit (upstream commit `3b73ab4e1`) is the canonical
example. `auth_xfer` was designed to run only as a child of `dao::exec()`.
It checked the parent call's opcode but never validated the parent's
`contract_id`. An attacker could invoke `auth_xfer` outside the DAO
execution context — the opcode check passed, but the contract ID was wrong.

This is a concrete instance of safety.md Lesson 2 — "Validate the target,
not just the action." Checking `data[0]` tells you what function will run,
but not what contract will run it.

**The fix**: Two mandatory checks on every cross-contract parent call:

```rust
if exec_callnode.data.contract_id != *DAO_CONTRACT_ID {
    return Err(DaoError::AuthXferParentWrongContractId.into())
}
if exec_callnode.data.data[0] != DaoFunction::Exec as u8 {
    return Err(DaoError::AuthXferParentWrongFunctionCode.into())
}
```

**Detection**: For every contract function that validates a parent call,
check whether BOTH `contract_id` AND `function_code` are validated. If
only `data[0]` is checked, the validation is incomplete.

**The principle**: **Every parent call validation must check both contract_id
and function code.** Opcodes are namespaced per contract — the same byte
means different things in different contracts. Without contract_id
validation, the check is blind to which contract is being called. This
extends safety.md Lesson 2 with the specific two-field check pattern.

**DarkWow audit result (2026-06-03):** All 9 cross-contract-dependent contracts
checked. 7 were already safe (auction, dex, bridge, stablecoin, darkbet_exchange,
relayer_endowment; tender has no child calls yet). 2 required fixes:
`dao_escrow::verify_member_capability_v1` (missing identity contract_id check)
and `labor_market` (3 functions: dispute_v1, initiate_dispute_v1,
accept_job_with_capability_v1 missing dao_escrow/identity contract_id checks).
Both fixed.

### Lesson 16: Unconstrained ZK Witnesses — The Mint Authorization Bypass

**The vulnerability**: PromissoryNote's `Mint_V1` ZK circuit declared `mint_public` as a witness and exposed it via `constrain_instance`, but had NO constraint proving `mint_public = poseidon_hash(backing_secret)`. The `backing_secret` witness didn't exist at all in the circuit. The comment on the witness block said "Backing capability proof (mint_public = poseidon_hash(backing_secret))" but this was aspirational text, not a circuit constraint.

The entrypoint checked `params.mint_public != stored_auth` at line 530, where `stored_auth` is publicly readable from the on-chain token registry. Since `mint_public` was completely unconstrained in the circuit, any prover could:
1. Read `stored_auth` from the token registry (public data)
2. Set `mint_public = stored_auth` as the witness value
3. Generate a valid ZK proof
4. Bypass the entrypoint's authorization check and mint tokens of ANY registered token type

**The fix (2026-06-05)**: Added `Base backing_secret` to the witness block and constrained `derived_mint_public = poseidon_hash(backing_secret); constrain_equal_base(derived_mint_public, mint_public)`. The client now passes `mint_secret` as a witness instead of the pre-computed `mint_public`, letting the circuit derive `mint_public` from the secret.

**The principle**: **Every witness that serves as an authorization check must have its derivation constrained in the circuit.** If `mint_public` is compared against on-chain state to authorize minting, the circuit must prove that `mint_public` is derived from a secret the prover knows — not just accept it as a free variable. An aspirational comment is not a constraint. The ZK circuit is the only enforcement mechanism; if it's not in the circuit, it doesn't exist.

**Detection heuristic**: For every `constrain_instance` of a value that is checked against on-chain state in the entrypoint, verify the circuit constrains how that value is derived. Grep for the value name in the `.zk` file — if it only appears as `constrain_instance(value)` without any prior derivation constraint, it's a free witness.

### Lesson 17: Off-Circuit Value Conservation — The Fee Inflation Vector

**The vulnerability**: NativeToken's `FeeV1` ZK circuit had zero constraint linking `input_value` and `output_value`. The fee subtraction (`output_value = input_value - fee`) was computed off-circuit in the Rust client. The circuit used `input_value` solely in the input coin hash and `output_value` solely in the output coin hash — they were independent witnesses with no relationship enforced.

Since the entrypoint has no way to detect value inflation (values are hidden in Pedersen commitments with different blinds), a prover could set `output_value = input_value + 1,000,000` and generate a valid ZK proof. The 1-in-1-out structure provided zero actual conservation.

**The fix (2026-06-05)**: Added `Base fee` witness, `constrain_instance(fee)`, and the constraint `computed_sum = base_add(output_value, fee); constrain_equal_base(computed_sum, input_value)` in the circuit. The fee is now a ZK public input, verified against the transaction's declared fee in the entrypoint. Range checks added on all three values.

**The principle**: **Structural conservation (1-in-1-out) is not cryptographic conservation.** If the ZK circuit doesn't enforce the relationship between values, the relationship doesn't exist. Every value transformation (fee subtraction, interest accrual, exchange rate conversion) that happens off-circuit must be constrained in-circuit. The Rust client is a convenience, not a security boundary.

**Related finding**: NativeToken's `TransferV1` also lacked cross-proof value conservation. Unlike PromissoryNote's `verify_value_conservation()` (which sums Pedersen commitments per token_commit), NativeToken had no check that `sum(input value_commits) == sum(output value_commits)`. Fixed (2026-06-05) by adding the same Pedersen homomorphic sum check across inputs and outputs.

### Lesson 18: Independent Witness Separation — The Coin-Owner/Transaction-Signer Split

**The vulnerability**: Both NativeToken and PromissoryNote burn circuits had separate `coin_secret` (for nullifier derivation) and `signature_secret` (for transaction signing) witnesses with no cross-constraint. A prover could use `secret_A` for coin ownership proof and `secret_B` for transaction signing — the coin owner and the transaction signer could be different entities.

This broke the fundamental assumption that the person signing the burn transaction is the coin owner. The nullifier proves knowledge of `coin_secret` (since `nullifier = poseidon_hash(coin_secret, coin)`), but the transaction signature proves knowledge of a different `signature_secret`. No constraint linked them.

**The fix (2026-06-05)**: Derive `signature_secret` in-circuit from `coin_secret` and `nullifier`:
```
derived_signature_secret = poseidon_hash(coin_secret, nullifier);
constrain_equal_base(derived_signature_secret, signature_secret);
```
The `signature_secret` is cryptographically bound to `coin_secret` (can't use an independent secret), but since `nullifier` is unique per coin, each burn produces a different `signature_secret` — and therefore a different `signature_public`, preserving unlinkability across burns. The transaction signer IS the coin owner by construction, but each burn has a unique on-chain identity.

**The principle**: **When a ZK proof proves ownership of a secret, derive per-operation signing keys from that secret — don't reuse the static key directly.** Adding a second independent secret for signing creates a separation that can be exploited. But exposing the raw static key (`pub = ec_mul_base(coin_secret, K)`) as a public input links all of a user's burns on-chain. The correct pattern derives a per-burn signing key using a unique operation identifier (the nullifier): `signature_secret = hash(coin_secret, nullifier)`. This binds the signer to the coin owner cryptographically while ensuring each burn has a distinct, unlinkable signature public key.

**First-attempt pitfall**: The initial fix removed `signature_secret` entirely and exposed `derived_pub_x/y` (from `pub = ec_mul_base(coin_secret, K)`) directly as public inputs. This fixed the separation attack but created a privacy regression — every burn from the same coin owner revealed the same static public key, making all burns trivially linkable. The per-burn derivation pattern fixes both problems simultaneously.

### Lesson 19: Isolated Execution Overlays — The Same-Block Double-Spend

**The vulnerability**: In `bin/dwowd/src/execution.rs`, every contract call receives `base_overlay.clone()` — an independent copy of the pre-block state. No call sees any other call's state changes during execution. Diffs are merged post-hoc with `main_overlay.add_diff(diff)` which silently overwrites duplicate keys.

Two transactions spending the same coin in the same block both pass their exec-phase nullifier checks (base state shows nullifier unspent). Both writes land in the merge. The merge silently overwrites, so both transactions appear to succeed.

The mempool only deduplicates by exact transaction hash — two different transactions spending the same nullifier are not detected as conflicting.

**Status (2026-06-05)**: Documented with a TODO for key-conflict detection in the merge phase. The full fix requires either: (a) semantic nullifier deduplication in the mempool, (b) key-conflict detection in the merge phase (reject blocks with conflicting diffs), or (c) a shared-overlay execution model. This is deferred pending sled_overlay API access for key iteration in diffs.

**The principle**: **Isolated execution overlays are correct only when combined with conflict detection at merge time.** If every call sees an independent pre-block state, two calls can both "succeed" while making conflicting state changes. The merge phase must detect and reject these conflicts — silent overwrite is not safe for value-bearing state.

**Mitigation in the meantime**: The miner's block construction logic should reject transactions with conflicting nullifiers before block assembly. The mempool should track a set of spent nullifiers alongside transactions.

---

## Flakey Patterns: Recognition and Prevention

A **flakey pattern** is a solution that passes functional tests but violates a core architectural invariant. It looks correct in isolation — the code compiles, the tests pass, the immediate problem is solved — but it undermines the very property the system exists to provide. These are the most dangerous bugs because they survive code review and automated testing.

Flakey patterns are also the primary way that **blast radius expands without anyone noticing**. In an o-cap architecture, each capability is meant to be a self-contained authorization token — lose one, lose access to exactly one action. A flakey pattern that allows silent authorization bypass, capability reuse, or cross-instance identity linking doesn't break one action; it erodes the isolation that the o-cap model depends on. One flakey signature check in a shared validation path can turn a single-capability compromise into a cross-contract exploit. The o-cap model's blast-radius guarantee is only as strong as the weakest verification in the capability chain.

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
| **Shared raw pubkeys across contract instances** | Same wallet pubkey used for `owner_pubkey`, `member_pub`, `staker_pub` across multiple instances of the same contract | Cross-instance identity linking — an observer enumerates all contracts a user interacts with by matching the pubkey. Fix: `SecretKey::derive_instance` |
| **Silent authorization bypass** | `verify_capability_for_action` returns `Ok(())` when governance is inactive instead of `Err(GovernanceNotActive)` | Caller proceeds as if authorized — the check exists in code but the failure branch is a no-op. The function name says "verify" but the implementation says "succeed." |
| **Placeholder signatures in production params** | `signature: pallas::Base::zero()` instead of `signature: schnorr::Signature` | Type system prevents accidental misuse — `pallas::Base::zero()` compiles everywhere, `schnorr::Signature` requires actual signing. A scalar zero is not a signature. |
| **Safety features disabled by default** | `DrainConfig { circuit_breaker: None, exit_queue: None }` as `Default` | Every deploy starts insecure. Operators must opt-in to safety. Defaults should be the secure configuration; opt-out for exceptions. |
| **Missing temporal validation** | Slash attestation accepts `block_height` from any block, including future blocks | A relayer can pre-register a slash for block N+1000, blocking real slash attestations at that height via idempotency check. Temporal order matters — validate that events happened in the past and within a recency window. |
| **Capability descriptors out of sync with dispatch** | Descriptor says `function_id: 0x01` for `Subscribe` but dispatch maps `0x00` | The host capability engine enforces rules on the wrong functions. A function call passes capability checks for an action it doesn't perform. The descriptor is security infrastructure, not documentation (Lesson 10). |
| **ZK circuit / client public input ordering mismatch** | Circuit `constrain_instance` order: `[x, y, id, bid]`. Client `to_vec()`: `[id, bid, x, y]` | Proofs verify against the circuit's instance column order. Mismatched order means instance column 0 constrains `x` but receives `id` — the proof verifies garbage. The circuit, the entrypoint, and the client builder must agree on public input order. |

### The Fix Pattern

Flakey patterns are almost always fixed by one of two approaches: **use the cryptographic commitments you already have**, or **derive per-instance keys deterministically**.

```
FLAKEY:  Add plaintext field + new ZK circuit to prove plaintext matches hidden value
PROPER:  Compare existing commitments using deterministic derivation both sides compute

FLAKEY:  Use raw wallet pubkey across multiple contract instances
PROPER:  Derive per-instance key via SecretKey::derive_instance(&contract_id, &instance_seed)

FLAKEY:  Return Ok(()) when authorization check finds nothing (silent pass)
PROPER:  Return Err(GovernanceNotActive) — deny by default, enumerate only the success conditions

FLAKEY:  Accept pallas::Base or [u8; 32] as a signature type
PROPER:  Use schnorr::Signature — let the type system enforce that actual signing occurred

FLAKEY:  Safety features are None by default, requiring operator opt-in
PROPER:  Safety features are enabled by default — Default::default() is the secure configuration

FLAKEY:  Accept block_height without bounds checking
PROPER:  Validate temporal parameters: block_height <= current_block && current_block - block_height <= MAX_AGE

FLAKEY:  Capability descriptor drifts from entrypoint dispatch table
PROPER:  Descriptor and dispatch are a matched pair — updating one without the other is a half-implemented change
```

The `value_commit` approach (Lesson 4) exemplifies the first: instead of adding `public_value` plus a `TransferOutput_V1` circuit, we use the existing `value_commit` plus deterministic blind derivation. Fewer lines of code, fewer circuits, stronger privacy.

The `derive_instance` approach (Per-Capability Keys) exemplifies the second: instead of reusing the wallet pubkey across all escrows, stakes, and pools, each instance gets a unique derived key. Same wallet, different pubkey per instance — cross-instance linking becomes impossible.

The deny-by-default approach (Lesson 10, DAO governance) exemplifies a third principle: authorization checks must enumerate what grants access and reject everything else. A function named `verify_X` that returns `Ok(())` when X doesn't exist is not verifying — it's a no-op with a misleading name.

### The Root Causes

Looking across every vulnerability identified in the review, the root causes fall into just five categories:

1. **Insufficient on-chain verification** — ZK proof accepted as sufficient without checking on-chain state (Lesson 1). Signature field exists but verification is missing or uses a placeholder type (ESC-001, INS-001, DAO-002).

2. **Authorization by presence, not proof** — A parameter field or function name implies authorization, but nothing checks it. The parameter exists, the code compiles, but the guard is decorative (MV-001, DAO-001).

3. **Default insecurity** — The path of least resistance (default config, easiest client API, simplest builder) produces an insecure configuration (DRAIN-001, Lesson 9).

4. **Drift between specification and implementation** — Capability descriptors don't match dispatch tables. Client `to_vec()` order doesn't match circuit `constrain_instance` order. The system has two sources of truth and they disagree (Lesson 10, TENDER-002).

5. **Missing temporal or lifecycle constraints** — Functions accept parameters without validating when an event occurred, whether a lock period has elapsed, or whether state was persisted before the next phase (ATTEST-001, BET-001, POOL-001).

Every fix in the review maps to one of these five root causes. When auditing a contract, these are the five questions to ask — they catch the majority of vulnerabilities before they reach production.

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
11. **Does the same raw wallet pubkey appear across multiple contract instances?** If yes, derive per-instance keys with `SecretKey::derive_instance(&contract_id, &instance_seed)`. Store a random `instance_seed: [u8; 32]` on-chain so the wallet can reconstruct the derived key without a circular dependency. The same wallet creates a different pubkey for every contract instance — cryptographically unlinkable.

If the answer to any of (3)-(5) is yes, and the answer to (2) is "yes, but we need to know the blind," consider deterministic blind derivation before adding a plaintext field.

If the answer to any of (6)-(11) is yes, the code has an o-cap privacy deviation. Apply the fix pattern from the corresponding lesson.

12. **Does an authorization function return `Ok(())` when the thing it's checking is absent?** If `verify_X` returns success when X doesn't exist, it's not verifying — it's rubber-stamping. Every verification function must have a deny-by-default posture: enumerate the conditions that permit access, and reject everything else.
13. **Is a signature field typed as `pallas::Base` or `[u8; 32]` instead of `schnorr::Signature`?** The type system is a security tool. `schnorr::Signature` communicates intent and prevents zero-value placeholders. Raw scalar types invite `::zero()` and `::dummy()`.
14. **Are safety features opt-in?** Check `Default` impls on config structs. If `circuit_breaker` defaults to `None` and `exit_queue` defaults to `None`, the contract deploys with safety off. Flip the defaults — enable protection by default, let operators explicitly disable.
15. **Does a function accept a `block_height` argument without validating it's in the past?** Temporal parameters must be bounded: `block_height <= current_block` and `current_block - block_height <= MAX_AGE`. Without this, future blocks can be pre-registered and stale events replayed.
16. **Are function_ids in the capability descriptor verified against the entrypoint dispatch table?** For every `Action` in the descriptor, grep the entrypoint for the corresponding function enum variant. Mismatched IDs mean the capability engine authorizes the wrong actions.
17. **Does the `to_vec()` order in the client match the `constrain_instance` order in the circuit?** Write a comment above both listing the expected order. They must be identical, position for position. The entrypoint's `zk_public_inputs` must also match.
18. **Does the contract receive spend_hook callbacks?** If yes, verify: (a) `caller_contract_id` is validated against the expected PN contract, (b) nullifiers are tracked for replay protection, (c) the handler is fallible (callback failure reverts the burn), (d) `define_contract_with_spend_hook!` is used instead of `define_contract!`, (e) the spend_hook handler does not make external assumptions (oracle prices, cross-chain state) — the callback runs in the same overlay as the burn and must be deterministic.
19. **Does the `.zk` source declare any `Base` constants with non-magic names?** If the constant name isn't `VALUE_COMMIT_VALUE`, `VALUE_COMMIT_RANDOM`, `VALUE_COMMIT_RANDOM_BASE`, or `NULLIFIER_K`, verify the VM synthesizer handles it. `Base` constants that exist only as documentation should be removed — `constrain_instance` already binds the public input.
20. **Do the circuit and the SDK use the same Merkle hash function?** Grep the circuit's `.zk` source for `merkle_root` — it uses Sinsemilla via `OrchardHashDomains::MerkleCrh`. Grep the SDK for `MerkleNode::combine` — it uses Poseidon. If they differ, valid Merkle proofs cannot be generated from SDK utilities. Either add a Sinsemilla-compatible Merkle tree to the SDK or document the external chain dependency.
21. **Are input nullifiers bound to their operation context?** For every `constrain_instance` of a nullifier in a `.zk` circuit, check whether an operation-specific identifier (bulla, proposal ID, swap ID) is also constrained and bound to the nullifier. A nullifier that proves "I spent coin X" without saying "for purpose Y" can be reused across different purposes — each use spends the same coin for a different operation. Fix: `input_nullifier = poseidon_hash(coin_nullifier, operation_bulla)`.
22. **Does every parent call validation check BOTH `contract_id` AND `function_code`?** Grep for `data[0]` checks in child-call validation paths. If the code checks the opcode byte but not the `contract_id`, it's vulnerable to contract-swapping. The same opcode means different things in different contracts. Always validate both fields.

### The O-Cap / ZK-Proof Symbiosis

Object-capability security and zero-knowledge proofs are not two independent design choices — they are complementary. Each addresses a weakness in the other:

| O-Cap provides | ZK-Proof provides |
|---|---|
| Fine-grained per-action authorization | Hiding *who* holds the capability |
| State-machine transitions (produce/consume) | Hiding *which* capability is being exercised |
| Blast-radius containment (one cap = one action) | Hiding the relationship between capabilities |
| Auditable on-chain state (who can do what) | Unlinkability across transactions |
| Revocability (consume the cap) | Privacy of the revocation event |

**The o-cap model without ZK proofs** is a permission system with full surveillance: every capability exercise is visible, every holder is linkable, every state transition is public. The system is secure but not private.

**ZK proofs without the o-cap model** are a privacy layer on a monolithic authorization scheme: you can hide who authorized an action, but if the underlying auth is a single god-mode ACL, a single compromised key controls everything. The system is private but not secure.

**Together**, they create a system where each action requires a specific, unlinkable capability proof:
- The o-cap model ensures that compromising one capability (e.g., a specific escrow's cancel right) doesn't grant access to any other capability (blast radius = 1).
- The ZK proof ensures that exercising that capability reveals nothing about which capability was used, who holds it, or what other capabilities that holder possesses.

This is why DarkWow contracts separate **capability derivation** (on-chain, per-instance, auditable) from **capability exercise** (ZK-proven, off-chain, unlinkable). The `CapabilityId` is a Poseidon hash of `(contract_id, capability_type, instance_seed)` — deterministic, unique per action per instance, and only meaningful to someone who already knows all three inputs. The ZK proof constrains that the prover knows a valid capability secret without revealing which one.

#### The Capability Descriptor as Security Boundary

The capability descriptor is the contract's security interface — it declares:
- Which actions exist (function IDs)
- What capabilities are required to call each action (`requires`)
- What capabilities are consumed by each action (`consumes`)
- What capabilities are produced by each action (`produces`)

The host runtime enforces these declarations. An action not in the descriptor cannot be called through the capability system. A capability not declared as `produces` cannot be minted. A capability not declared as `consumes` cannot be revoked.

This means the descriptor is a **compile-time security audit** written in Rust types. A missing `consumes` entry means a capability persists when it should be destroyed — a privilege that should be one-shot becomes reusable. A missing `produces` entry means a state transition has no artifact — the system can't track who entered what state. A wrong `requires` expression means the wrong capability gates access.

The discipline: every time you add, remove, or rename a contract function, update the capability descriptor. The descriptor and the dispatch table must match, or the capability system is enforcing rules on a phantom contract.



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
- [ ] If receiving spend_hook callbacks: verify `caller_contract_id`, track nullifiers for replay, use `define_contract_with_spend_hook!`
- [ ] If issuing tokens: set `spend_hook` on minted coins to your contract ID so burns route through your callback
- [ ] If receiving coins from child calls: verify `output.spend_hook` matches expectations (all 5 ZK circuits expose it as a public input)

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
- [ ] After recompiling a circuit with a new zkas version, verify `ProvingKey::build` succeeds
- [ ] Every `Base` constant in the `.zk` source has a corresponding handler in the VM synthesizer
- [ ] Merkle hash functions match between circuit (`merkle_root` opcode) and SDK (`MerkleNode::combine`) — Sinsemilla vs Poseidon divergence blocks testability
- [ ] `to_vec()` instance count matches the circuit's `constrain_instance` count — mismatched counts cause silent proving failures

---

## Design Principles: Hardening by Construction

The following principles emerged from a systematic review of hardening gaps across all contracts. Each was identified as a deferred concern, fixed across the codebase, and distilled into a rule that prevents recurrence. They are not contract-specific bugs — they are patterns that should be built into every new contract from the start.

### Principle 1: Version Every State Struct

**The gap**: With one exception (identity contract), no state struct in any contract carried a version field. State structs are serialized to binary and stored in Merkle trees. If a future upgrade changes a struct's serialization layout, existing on-chain data becomes unreadable with no migration path.

**The fix**: Added `pub version: u8` as the first field of every state struct (~60 structs across 22 contracts), defaulting to `0`. The entrypoint reads the version byte, deserializes the old format, and migrates to the new format on write.

**The principle**: **Version your state before you need to.** The cost is one byte per record. The alternative — a hard fork to fix unreadable on-chain data — is catastrophic. This is not speculative future-proofing; it is insurance against a failure mode that has hit every long-lived blockchain. Every state struct gets `pub version: u8` at creation time, not when the first breaking change forces it.

### Principle 2: Secure Defaults Are the Only Defaults

**The gap**: Client builders defaulted `nonce` and `secret_nonce` fields to `pallas::Base::zero()`. A zero nonce means every call from the same caller with the same parameters produces an identical commitment — trivially linkable. The setter API allowed callers to override, but the default was the least-private possible value. A developer who calls `Builder::new()` and forgets to set a nonce gets the worst outcome.

**The fix**: Replaced all `pallas::Base::zero()` nonce defaults with `pallas::Base::random(&mut OsRng)` across 8 builders in betting_stake, slot, and game_room contracts. Callers can still override via setters, but the default is private.

**The principle**: **The path of least resistance must be the secure path.** A developer who calls `Builder::new()` without reading every optional setter should get a secure configuration by default. Defaults that are "safe if you remember to change them" are insecure — someone will forget. This applies to nonces, blinds, safety feature flags, and every other configurable parameter. `Default::default()` should produce a secure instance; opt out for exceptions, never opt in for safety.

### Principle 3: Creation Requires a Deactivation Path

**The gap**: State structs across multiple contracts carried `active: bool` or `is_active: bool` flags, but no contract function ever set them to `false`. Underwriters could not resign, markets could not close, risk types could not be retired, governance could not be paused, capability requirements could not be revoked, relayer endowments could not be deactivated. Every record, once created, was permanently active — a state leak with no recovery.

**The fix**: Added 7 deactivation functions across 4 contracts:

| Contract | Function | What it deactivates |
|---|---|---|
| insurance_market | `DeactivateUnderwriterV1` | Underwriter record |
| insurance_market | `CloseMarketV1` | Insurance market |
| insurance_market | `RetireRiskTypeV1` | Risk type definition |
| oracle | `SetOracleActiveV1` | Oracle feed |
| dao_escrow | `SetGovernanceActiveV1` | Governance config |
| dao_escrow | `DeactivateCapabilityRequirementV1` | Capability requirement |
| relayer_endowment | `DeactivateEndowmentV1` | Endowment account |

Each follows the full-stack contract pattern: function enum variant → model structs (ParamsV1 + UpdateV1) → entrypoint dispatch → client builder → capability descriptor. Every function verifies caller authorization (owner, capability holder, or governance proof) before mutating state.

**The principle**: **Every `active` flag needs a function that sets it to `false`.** Creation without deletion is a state leak. For every "create" or "register" function in a contract, there must be a corresponding "deactivate" function. This should be part of the contract scaffolding template — not retrofitted after review catches the gap.

### Principle 4: Bound All User-Supplied Iteration

**The gap**: Several functions accepted user-supplied `Vec` parameters and iterated over them without length checks. The WASM runtime has execution limits, so this is not a DoS vector today, but hitting the runtime ceiling produces an opaque WASM trap — functional but not debuggable. As gas metering evolves, unbounded iteration incurs proportional and unpredictable costs.

**The fix**: Added per-call limit constants with explicit assertions before iteration loops:

| Contract | Constant | Value |
|---|---|---|
| darkbet_exchange | `DARKBET_EXCHANGE_MAX_SETTLE_MATCHES` | 100 |
| pool_stake | `POOL_STAKE_MAX_REBALANCE_MEMBERS` | 100 |
| relayer_endowment | `RELAYER_ENDOWMENT_MAX_ALLOCATIONS` | 100 |
| identity | `IDENTITY_CONTRACT_MAX_DAG_CREDENTIALS` | 100 |
| roulette | `ROULETTE_CONTRACT_MAX_SETTLE_BETS` | 100 |

**The principle**: **User-supplied collections must have explicit upper bounds.** An assertion with a clear error message ("Too many match IDs for settle") is debuggable; a WASM trap at the runtime execution limit is not. Choose bounds generous enough for legitimate use and small enough to keep execution cost predictable. Functions that need to process more items can be called multiple times with different slices.

### Architectural Concerns

Two patterns identified in the review require network-level infrastructure not yet built. They remain noted but are not actionable at the contract level:

**Oracle centralization.** Contracts use single-oracle models (darkbet_exchange checks a single `oracle_id`, insurance_market uses a single `oracle_pubkey`). Mitigation — threshold oracles, M-of-N attestations, oracle rotation — requires the oracle network to support those primitives first. When that infrastructure exists, dependent contracts should accept M-of-N signatures rather than a single key.

**Rate limiting.** Only `drain_protection` implements rate limiting. Per-block or per-epoch limits on state-creating functions are defense-in-depth once fee markets and gas accounting exist. Until then, transaction fees are the rate limit.

---

## References

- [NativeToken](./native_token.md) — Consensus token with zero business logic
- [Standards](./standards.md) — ZK circuit, token, and testing standards
- [Composability](../../contract/composability.md) — Cross-contract child call patterns
- [PromissoryNote](../../contract/promissory_note.md) — Privacy-preserving bearer instrument contract for DeFi tokens
- [Python Contract Simulations](../testing/python-simulations.md) — Smoke test layer for catching state machine bugs before reaching the testnet
