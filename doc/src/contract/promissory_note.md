# Promissory Note: Privacy-Preserving Bearer Instruments

## Overview

The Promissory Note contract is DarkWow's DeFi token layer. It implements
**bearer instruments** — cryptographic promissory notes where possession of a
coin IS the capability to redeem it. Every coin embeds a hidden token type,
a hidden value behind a Pedersen commitment, and an owner-controlled nullifier
secret. Transferring a coin means exercising one capability (burn) and issuing
a new one (blind output) in a single atomic step, with no on-chain link
between the two.

**Key properties:**

- **Bearer instruments**: coins are self-contained capabilities. No ACL, no
  allowlists. If you hold the nullifier secret, you can spend the coin.
- **Hidden token types**: token IDs are Poseidon commitments — no observer can
  determine which token a coin belongs to, making all tokens fully fungible
  at the observer level.
- **Homomorphic value conservation**: value commitments use Pedersen
  (additively homomorphic), enabling the entrypoint to enforce
  sum(inputs) == sum(outputs) per token type without revealing plaintext values.
- **Atomic burn+mint**: transfers consume old coins and create new ones in one
  transaction. Nullifiers break the link.
- **AEAD note delivery**: output attributes are encrypted to the recipient's
  public key. Only the recipient can decrypt and verify their coin.

## Why Separate from NativeToken?

DarkWow splits token functionality across two contracts following a
**CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD** design philosophy:

| Priority | NativeToken | Promissory Note |
|----------|-------------|-----------------|
| 1. Consensus | PoWRewardV1 — block rewards | N/A |
| 2. Network Fees | FeeV1 — deterministic fee payment | N/A |
| 3. Privacy | N/A | Full privacy DeFi |

| Aspect | NativeToken | Promissory Note |
|--------|------------|-----------------|
| Purpose | Consensus (PoW rewards, fees) | DeFi tokens |
| Tokens | Single (DARK), hardcoded | Multiple, registered via TokenMint |
| Authorization | None (permissionless) | Backing capability proof |
| Value commitments | Pedersen | Pedersen (same primitive) |
| Attack surface | Minimal (consensus-critical) | Broader (multi-token, composability) |

NativeToken is deliberately rock-dumb — no multi-token, no auth, no freezing —
to minimize consensus attack surface. If the consensus-critical native token
contract has a bug, the chain halts. NativeToken therefore does one thing:
mint and burn DARK for block rewards and fees, using the simplest possible
circuits with zero composability surface area.

Promissory Note is a **WASM contract** (not native), so bugs in Promissory Note
cannot halt consensus — they only affect DeFi tokens built on it. It carries
the minimum viable business logic for DeFi: token creation, minting with
backing capability proofs, private transfers, and OTC swaps.

See [NativeToken](native_token.md) for the native-side rationale.

## Bearer Instrument Model

A promissory note is a written promise to pay a specified sum to a specified
person or to bearer. In this contract:

- **The coin** is the note. `Coin = H(owner_pub, value, token_id, spend_hook, user_data, blind)`
- **The nullifier secret** is possession. `Nullifier = H(secret, coin)`
- **To transfer**: burn the old note (exercising the capability to destroy it)
  and issue a new note to the bearer (the recipient's public key).
- **To redeem**: the ultimate redemption path (e.g., stablecoin for collateral,
  wrapped token for native asset) is enforced by the **issuing contract** via
  `spend_hook` — not by Promissory Note itself.

The contract tracks what it needs to prevent double-spending (nullifiers) and
prove existence (Merkle roots). It does NOT track who owns what, what the values
are, or which tokens are being moved. Those are private by construction.

## Token Model

### Coin

```
Coin = poseidon_hash(owner_pub, value, token_id, spend_hook, user_data, blind)
```

Where `owner_pub = poseidon_hash(secret)` — a Schnorr-style field element
public key, not an EC point. This keeps the ZK circuits on the base field
without requiring EC multiplication for key derivation.

### Value Commitment (Pedersen)

```
value_commit = pedersen_commit(value, value_blind)
             = value * G_value + value_blind * G_rand
```

Pedersen commitments are additively homomorphic:
`C(v1, b1) + C(v2, b2) = C(v1+v2, b1+b2)`.

This property lets the entrypoint enforce value conservation — sum of input
value_commits equals sum of output value_commits — without knowing any
plaintext values. See [Cross-Proof Value Conservation](#cross-proof-value-conservation).

### Nullifier

```
nullifier = poseidon_hash(secret, coin_inner)
```

Publishing the nullifier proves the spender knows the coin's secret without
revealing which coin was spent. The entrypoint checks the nullifier hasn't
been seen before (SMT-backed nullifier set).

### Token ID

```
token_id = poseidon_hash(token_auth_parent, token_user_data, token_blind)
```

`token_auth_parent = poseidon_hash(mint_secret)` is the backing capability
commitment — stored in the token registry when the token is created. MintV1
proves knowledge of `mint_secret` against this stored value.

Token IDs are hidden from observers. The `token_commit = poseidon_hash(token_id, token_blind)`
in BurnV1 and BlindOutputV1 binds coins to a token type without revealing it.
The entrypoint groups inputs and outputs by `token_commit` to enforce per-token
value conservation — still without knowing the underlying token_id.

## Contract Functions

| Function | Opcode | Purpose |
|----------|--------|---------|
| TokenMintV1 | `0x00` | Create new token type |
| *(hole)* | `0x01` | Removed (was AuthTokenMintV1) |
| MintV1 | `0x02` | Mint tokens of existing type |
| BurnV1 | `0x03` | Burn/destroy tokens |
| TransferV1 | `0x04` | Private transfer (burn + blind output) |
| OtcSwapV1 | `0x05` | Atomic OTC swap (2-in, 2-out) |

### TokenMintV1 (`0x00`)

Creates a new token type. This is how stablecoins, wrapped tokens, and other
DeFi assets are born.

**Parameters:**
```rust
struct TokenMintParamsV1 {
    coin: Coin,                   // Initial coin (first mint)
    value_commit: pallas::Point,  // Pedersen commitment
    token_id: pallas::Base,       // H(auth_parent, user_data, blind)
    token_auth_parent: pallas::Base, // H(mint_secret) — backing capability
    token_commit: pallas::Base,   // H(token_id, token_blind)
}
```

**Entrypoint checks:**
- Coin is unique (not already in coins tree)

**State update:** Adds coin to coins tree and Merkle tree. Stores
`token_id → token_auth_parent` in token registry. Updates token registry
Merkle tree.

### MintV1 (`0x02`)

Mints new tokens of an existing type. Proves knowledge of the backing secret
directly against the stored `token_auth_parent`.

**Parameters:**
```rust
struct MintParamsV1 {
    coin: Coin,                      // The newly minted coin
    value_commit: pallas::Point,     // Pedersen value commitment
    token_id: pallas::Base,          // Token being minted
    token_registry_root: MerkleNode, // Proves token exists in registry
    mint_public: pallas::Base,       // H(mint_secret) — backing proof
}
```

**Entrypoint checks:**
- Coin is unique
- Token is registered (`token_id` exists in token registry)
- `mint_public == stored token_auth_parent` (backing capability match)
- `token_registry_root` matches current on-chain registry root (prevents replay
  with stale root after registry changes)

### BurnV1 (`0x03`)

Destroys coins. Publishes nullifiers to prevent double-spending.

**Parameters:**
```rust
struct BurnParamsV1 {
    inputs: Vec<Input>,
}
```

Where each `Input` carries:
```rust
struct Input {
    value_commit: pallas::Point,     // Pedersen (homomorphic)
    token_commit: pallas::Base,      // H(token_id, token_id_blind)
    nullifier: Nullifier,            // H(secret, coin)
    merkle_root: MerkleNode,         // Proves coin existed at this root
    user_data_enc: pallas::Base,     // H(user_data, user_data_blind)
    spend_hook: pallas::Base,        // Contract ID for cross-contract logic
    signature_public: pallas::Base,  // H(ephemeral_signature_secret)
}
```

**Entrypoint checks:**
- Merkle root exists in coin_roots tree
- Nullifier not already spent (SMT lookup)

### TransferV1 (`0x04`)

Atomic burn + blind output: consumes input coins and creates new output coins
in one transaction. The burn side provides authorization (nullifier proves
ownership); the blind output side creates new capabilities for the recipients.

**Parameters:**
```rust
struct TransferParamsV1 {
    inputs: Vec<Input>,    // Coins being consumed
    outputs: Vec<Output>,  // Coins being created
}
```

Where each `Output` carries:
```rust
struct Output {
    value_commit: pallas::Point,   // Pedersen (ZK-constrained)
    token_commit: pallas::Base,    // H(token_id, token_id_blind) (ZK-constrained)
    coin: Coin,                    // New coin commitment (ZK-constrained)
    note: AeadEncryptedNote,       // Encrypted (value, token_id, blinds, memo)
}
```

**Privacy model:**
- Nullifiers break the link between consumed coins and created coins
- Observers see the burn proofs and blind output proofs, but cannot determine
  which burn corresponds to which output
- Token types and values are hidden behind commitments
- The recipient discovers their coin by trial-decrypting the AEAD notes

**Entrypoint checks:**
- At least 1 input, at least 1 output
- All Merkle roots exist
- All nullifiers are unspent
- All output coins are unique
- **Cross-proof value conservation** (see below)

### OtcSwapV1 (`0x05`)

Atomic OTC swap between two parties. Same proof structure as TransferV1 but
enforces exactly 2 inputs and 2 outputs. Each party burns their coin and
receives the counterparty's coin as a blind output.

```
Alice burns input[0] → Bob receives output[1]
Bob burns input[1]   → Alice receives output[0]
```

Value conservation is enforced per `token_commit` group, so the swap must
conserve value for each token type independently.

## Cross-Proof Value Conservation

A critical security property: **the entrypoint verifies that the sum of
burned values equals the sum of created values**, per token type.

Without this check, a prover with one valid coin of value 1 could create a
TransferV1 that burns it and creates a new coin of value 1,000,000. Both
the BurnV1 and BlindOutputV1 proofs verify independently — they each only
prove their own coin is well-formed. The entrypoint must bridge them.

The check uses Pedersen's additive homomorphism:

```
For each token_commit group:
    sum(input value_commits) == sum(output value_commits)
```

Where `sum` is EC point addition. Since `C(v1,b1) + C(v2,b2) = C(v1+v2, b1+b2)`,
the equality holds iff the total input value equals the total output value
for that token type.

This is enforced for both TransferV1 and OtcSwapV1. For OtcSwapV1, the
token_commit pairing is:
- `inputs[0].token_commit` must equal `outputs[1].token_commit` (Alice→Bob)
- `inputs[1].token_commit` must equal `outputs[0].token_commit` (Bob→Alice)

## ZK Circuits

Promissory Note uses 4 ZK circuits:

| Circuit | Namespace | Public Inputs | Purpose |
|---------|-----------|---------------|---------|
| `token_mint_v1.zk` | `TokenMint_V1` | `token_id`, `token_auth_parent`, `coin`, `vc_x`, `vc_y` | Create token type |
| `mint_v1.zk` | `Mint_V1` | `token_root`, `mint_public`, `coin`, `vc_x`, `vc_y`, `token_id` | Mint with backing proof |
| `burn_v1.zk` | `Burn_V1` | `nullifier`, `vc_x`, `vc_y`, `token_commit`, `merkle_root`, `user_data_enc`, `spend_hook`, `signature_public` | Spend coins |
| `blind_output_v1.zk` | `BlindOutput_V1` | `coin`, `vc_x`, `vc_y`, `token_commit` | Create output coins |

**Design principles:**
- Value commitments use **Pedersen** (not Poseidon). Pedersen is additively
  homomorphic — the property that makes cross-proof value conservation possible.
- Coin commitments use **Poseidon** — the standard ZK-friendly hash.
- Public keys are **field elements** (`H(secret)`), not EC points. This avoids
  EC multiplication in circuits where it isn't needed.
- Signatures use **Schnorr-style** verification where `signature_public = H(ephemeral_secret)`.
  The ephemeral secret MUST be fresh per transaction (never reuse the wallet
  secret — doing so links all spends to the same on-chain signature_public).
- `token_commit` is ZK-constrained in both BurnV1 and BlindOutputV1, enabling
  the entrypoint to group by token type for value conservation.

### BurnV1 Public Input Layout

```
[nullifier, value_commit_x, value_commit_y, token_commit,
 merkle_root, user_data_enc, spend_hook, signature_public]
```

### BlindOutputV1 Public Input Layout

```
[coin, value_commit_x, value_commit_y, token_commit]
```

### MintV1 Public Input Layout

```
[token_registry_root, mint_public, coin, value_commit_x, value_commit_y, token_id]
```

### TokenMintV1 Public Input Layout

```
[token_id, token_auth_parent, coin, value_commit_x, value_commit_y]
```

## Capability Lifecycle

Promissory Note follows the [object capability](../../arch/ocap.md) model:

```
COMMIT: TokenMintV1
  token_auth_parent = H(mint_secret)
  → Stored in registry: token_id → token_auth_parent
  → "The backing secret exists, I control it"

EXERCISE (issuance): MintV1
  mint_public = H(mint_secret)
  → Entrypoint checks mint_public == stored token_auth_parent
  → Mints new coins of this token type

EXERCISE (transfer): TransferV1
  nullifier = H(coin_secret, old_coin)
  → BurnV1 proves: "I know secret for a coin in the tree"
  → BlindOutputV1 proves: "New coin is well-formed"
  → "I consumed my capability and issued a new one to the recipient"
```

The capability IS the coin. Holding a coin means you can produce a valid
nullifier for it. The nullifier proves you held the capability without
revealing which one — it's a cryptographic receipt of exercise.

## Recipient Verification Flow

When a recipient receives coins via TransferV1 or OtcSwapV1, they must
discover and verify their coins:

```
1. Scan transaction outputs
2. For each Output, try AEAD decryption of the note
   with the recipient's view key
3. If decryption succeeds → "this coin is for me"
4. Verify coin commitment:
   expected_coin = H(recipient_address, decrypted_value,
                     decrypted_token_id, decrypted_spend_hook,
                     decrypted_user_data, decrypted_coin_blind)
   assert expected_coin == output.coin
5. Verify value commit:
   expected_vc = pedersen_commit(decrypted_value, decrypted_value_blind)
   assert expected_vc == output.value_commit
```

The `verify_received_coin()` function in the client module ([client/mod.rs](../../../src/contract/promissory_note/src/client/mod.rs))
implements this flow.

## Cross-Contract Composition

### Spend Hook

The `spend_hook` field on `Input` is a contract ID. When set, transfers that
spend this coin must route through that contract — enabling protocol-owned
liquidity, collateral checks, and other DeFi logic. The spend_hook is a public
input to BurnV1, so it's cryptographically bound to the coin commitment.

### User Data

The `user_data` field carries encrypted parameters for the spend hook contract.
It's hidden behind `user_data_enc = H(user_data, user_data_blind)` on-chain.

### Value Commitment Validation

Parent contracts that make child calls to Promissory Note can verify the
transfer amount without seeing plaintext values:

```rust
// In the parent contract's entrypoint:
validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
validate_child_value_commit(&child_call.data, expected_amount, blind_seed)?;
```

`validate_child_value_commit` ([validation.rs](../../../src/contract/promissory_note/src/validation.rs))
recomputes the expected Pedersen commitment from the known plaintext value
and a deterministic blind seed, then checks it matches one of the child
output's `value_commit` fields. The blind seed must be derived
deterministically from parent state (e.g., `H(value, nullifier)`) so the
child transfer can compute the same blind.

This pattern follows [safety.md Lesson 4](safety.md): "use the cryptographic
commitments you already have." No plaintext values are added to the Output
struct. The privacy model is preserved.

## Database Trees

```
PROMISSORY_NOTE_CONTRACT_COINS_TREE                - coin → () (existence set)
PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE            - nullifier → spent (SMT-backed)
PROMISSORY_NOTE_CONTRACT_MERKLE_TREE                - Merkle tree of all coins
PROMISSORY_NOTE_CONTRACT_INFO_TREE                  - contract metadata
PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE            - historical Merkle roots
PROMISSORY_NOTE_CONTRACT_NULLIFIER_ROOTS_TREE       - historical nullifier roots
PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_TREE        - token_id → token_auth_parent
PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_ROOTS_TREE  - historical registry roots
```

## Files

```
src/contract/promissory_note/
├── Cargo.toml              # dwow_promissory_note_contract
├── pipeline.toml            # name = "promissory_note"
├── Makefile                 # WASM + zkas compilation
├── src/
│   ├── lib.rs               # PromissoryNoteFunction enum, constants, module declarations
│   ├── error.rs             # PromissoryNoteError enum
│   ├── model/mod.rs         # Coin, Nullifier, Input, Output, all Params/Update types
│   ├── entrypoint/mod.rs    # WASM entrypoint (init, exec, apply, metadata)
│   ├── validation.rs        # Cross-contract validation helpers
│   └── client/              # Client API (feature = "client")
│       ├── mod.rs           # PromissoryNote struct, verify_received_coin()
│       ├── token_mint_v1.rs # TokenMintCallBuilder
│       ├── mint_v1.rs       # MintCallBuilder
│       ├── burn_v1.rs       # BurnCallBuilder + create_burn_proof()
│       └── transfer_v1.rs   # TransferCallBuilder
├── proof/
│   ├── token_mint_v1.zk     # TokenMint_V1 circuit
│   ├── mint_v1.zk           # Mint_V1 circuit
│   ├── burn_v1.zk           # Burn_V1 circuit
│   ├── blind_output_v1.zk   # BlindOutput_V1 circuit
│   └── *.zk.bin             # Compiled ZK binaries
├── tests/
│   └── integration.rs       # Integration tests
└── README.md
```

## Related Contracts

- **[Stablecoin](stablecoin.md)** — Issues USDx tokens via TokenMintV1, manages
  CDP positions via MintV1/BurnV1 with spend_hook enforcement
- **[Bridge](bridge.md)** — Wraps external chain assets as promissory notes
- **[DEX](dex.md)** — Atomic swaps via OtcSwapV1
- **[NativeToken](native_token.md)** — Consensus token (DARK), the other half
  of the two-contract architecture

## References

- [Object Capability Architecture](../../arch/ocap.md)
- [Safety Patterns](safety.md) — Flakey pattern catalog and hardening principles
- [Composability](../../contract/composability.md) — Cross-contract call patterns
- [Standards](standards.md) — Security standards reference
