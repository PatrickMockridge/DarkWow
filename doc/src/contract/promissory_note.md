# Promissory Note: Private Bearer Instruments

## "Good Money Drives Out Bad" — Gresham's Law Reversed

In the late 18th century, Britain's Industrial Revolution had a problem.
The Royal Mint had effectively stopped minting copper coinage — the small
change that factories needed to pay workers and merchants needed to make
change. The state had failed to provide money.

Private enterprise filled the gap. Merchants, factories, and mines began
issuing their own tokens — **Conder tokens**, named after the numismatist
James Conder who catalogued them. These were bearer instruments: whoever
physically held the token could present it to the issuer for redemption
in gold, silver, or Bank of England notes. At their peak, millions of
privately-issued tokens circulated alongside (and often in preference to)
official coinage. They were typically better-made, harder to counterfeit,
and — critically — **actually redeemable**.

In *Good Money*, economic historian George Selgin documents how these
Birmingham button makers, steam engine manufacturers, and copper mines
built a private monetary system more reliable than the state's. The
tokens worked because they embodied a promise: **the issuer's commitment
to redeem on demand, carried by the token itself**. The token WAS the
proof of the promise. Whoever held it held the capability to redeem.

DarkWow's Promissory Note contract is the cryptographic realization of
this same principle — returning to the very roots of tokenization in a
radically new and futuristic way. A coin *is* a promissory note. Holding
it *is* the capability to redeem. The issuer's commitment propagates
through every transfer until redemption closes the loop.

## The Problem: ERC-20 Privacy Was Intractable

Privacy-preserving blockchain systems face a fundamental tension: how do
you replicate ERC-20 multi-token functionality — minting, transferring,
redeeming — without the transparent state that makes ERC-20 possible?

In a transparent system, the answers are trivial. The contract stores
`balances[address]`, an `onlyOwner` modifier guards `mint()`, and every
observer can verify conservation. In a private system, balances are
hidden, owners are hidden, values are hidden, and token types are hidden.

Three problems were previously considered intractable:

1. **Authorization without access control.** In Solidity, `onlyOwner`
   prevents unauthorized minting. In a private system, how do you prove
   you're authorized without revealing your identity? The answer cannot
   be an ACL — that would destroy privacy by construction.

2. **Value conservation without visible values.** An observer must be
   able to verify that total input value equals total output value per
   token type, without knowing any of the values or which token type is
   being transferred.

3. **Lifecycle closure without linkability.** When a coin is redeemed
   (e.g., a stablecoin for collateral, a wrapped token for the native
   asset), the system must record that redemption happened — but without
   linking the redemption to the original mint, which would deanonymize
   the entire chain of custody.

Promissory Note solves all three. The key that makes it possible is
**redemption capability** — and the `is_notequal` boolean logic gate
available on DarkWow but not upstream.

## The Breakthrough: Capability, Not Access Control

The fundamental difference from upstream smart contract platforms (Ethereum,
Solana, etc.) is philosophical and mathematical:

| | Upstream (Solidity/ERC-20) | DarkWow Promissory Note |
|---|---|---|
| Mint authorization | `onlyOwner` modifier on `mint()` | ZK proof of backing secret possession |
| Transfer authorization | `msg.sender == from` check | Nullifier proof (possession of coin secret) |
| Value conservation | Visible arithmetic on `uint256` | Pedersen additive homomorphism per token_commit group |
| Redemption proof | `emit Redeem(address, amount)` event | Zero-value receipt coin (ZK-constrained) |
| Identity model | Address-based (Ethereum account) | Capability-based (nullifier secret) |

Minting a promissory note is NOT authorized by access control. It is a
**proven property of the issuer**. The backing capability proof —
`token_auth_parent = poseidon_hash(mint_secret)` — is embedded in the
`token_id` at creation time. Every IssueV1 proves knowledge of the same
`mint_secret` against this stored commitment. The proof survives in the
chain of custody through every transfer until RedeemV1 closes the loop.

This inverts the ERC-20 model. Instead of "I am the owner, therefore I
can mint," it says: "I can produce this proof, therefore I am the issuer."
Ownership is not a database entry. It is a cryptographic capability.

## What Promissory Note Does and Does Not Do

### What PN Does

Promissory Note is **currency plumbing**. It provides the bearer instrument
primitives — mint, burn, transfer, and redeem — with full ZK privacy:

| Primitive | Opcode | What It Does |
|---|---|---|
| RegisterTypeV1 | `0x00` | Create a new token type with a backing capability commitment |
| RedeemV1 | `0x01` | Close the lifecycle: burn a coin, issue a zero-value receipt |
| IssueV1 | `0x02` | Mint coins of a token type — proves knowledge of the backing secret |
| RevokeV1 | `0x03` | Destroy coins, publish nullifiers |
| TransferV1 | `0x04` | Atomic burn + blind output — private transfer between parties |
| OtcSwapV1 | `0x05` | Atomic peer-to-peer exchange |

Every primitive is ZK-proven. Value conservation via Pedersen homomorphism.
Nullifiers break linkability. AEAD notes for recipient discovery. Spend hooks
for cross-contract composition.

### What PN Does Not Do

PN deliberately omits logic that belongs in the **issuer contract** — the
contract that controls a specific token type via its mint secret:

- **No supply cap.** IssueV1 has no `max_supply`. The mint secret is the only
  gate. If it leaks, unlimited minting occurs. The issuer contract must protect
  the secret and enforce its own supply invariants via spend_hook.
- **No collateralization enforcement.** PN cannot verify whether an issuer
  holds reserves. A token with zero backing is indistinguishable on-chain
  from a fully-backed one. Collateralization is the issuer contract's job.
- **No price oracle.** Token prices are market-driven via OTC swaps. PN has
  no concept of market value or exchange rates.
- **No mandatory redemption.** RedeemV1 exists and works, but PN does not
  require any token type to implement a redemption path. The issuer contract
  decides if and how redemption works.
- **No on-chain supply tracking.** There is no per-token `total_supply`
  counter. Outstanding circulation must be computed off-chain or maintained
  in the issuer contract's own database.

Caveat emptor means: understand where PN's responsibility ends and the issuer
contract's begins. PN gives you the primitives. The guarantees — collateral,
supply caps, redemption — are YOUR responsibility.

### Issuer Contract Responsibility

DarkWow ships with two issuer contracts that demonstrate this separation
correctly, covering both ends of the trust-model spectrum:

**Bridge: Cryptographic Self-Custody**

The [bridge](bridge.md) wraps external chain assets (ETH, XMR, ZEC, DAI, LTC)
as promissory notes. The user holds the secret that authorizes redemption —
the bridge cannot block or forge a withdrawal.

| PN Limitation | How the Bridge Handles It |
|---|---|
| Supply cap | 1:1 backing by external chain deposits |
| Redemption | User self-signs ZK withdrawal proof — cryptographic self-custody |
| Trust | Relayer is optional — user can run their own |
| Failure modes | Limited — user owns redemption keys; worst case is delay, not loss |
| Governance | `GovernanceReportV1` proves `total_deposited >= total_withdrawn` |

**Stablecoin: On-Chain Collateralization**

The [stablecoin](stablecoin.md) issues USDx with full on-chain enforcement —
the contract IS the issuer.

| PN Limitation | How the Stablecoin Handles It |
|---|---|
| Collateralization | On-chain CDP over-collateralization with automatic liquidation |
| Supply cap | Bounded by collateral deposits |
| Redemption | Calls `RedeemV1`, closing the full bearer-instrument lifecycle |
| Governance | On-chain audit: `total_collateral >= outstanding` |

If you're building an issuer contract, study these two as reference
implementations. Set `spend_hook` on every mint so burns route through your
contract. Track your supply on-chain. Make redemption a real code path.

For a full adversarial analysis of how these properties interact with Bearer
Bond coverage reports and composability risk, see
[Caveat Emptor: Pricing, Coverage & Adversarial Analysis](../arch/economics-caveat-emptor.md).


## The Lifecycle

```
RegisterTypeV1 → IssueV1 → TransferV1 (xN) → RedeemV1 → receipt
   0x00         0x02        0x04             0x01
```

A promissory note is a **promise from the issuer** to redeem on demand.
The coin IS both the proof of the promise AND the capability to redeem.
The lifecycle has three phases:

**Opening (0x00):** RegisterTypeV1 creates a new token type. The issuer
commits to their backing capability — `token_auth_parent = H(mint_secret)`.
A promise is made, recorded immutably in the token registry.

**Circulation (0x02–0x05):** IssueV1 creates coins of the token type (with
backing proof). TransferV1 moves them between parties (atomic burn+mint
with nullifier link-breaking). RevokeV1 destroys coins. OtcSwapV1 enables
atomic peer-to-peer exchange. The token_id — and therefore the issuer's
original commitment — propagates through every transfer.

**Closure (0x01):** RedeemV1 destroys the monetary value and creates a
zero-value receipt coin. The promise is honored. The receipt is permanent,
verifiable, and non-transferable — cryptographic proof that redemption
occurred with the issuer.

## Bearer Instrument Model

A promissory note is a written promise to pay a specified sum to a
specified person or to bearer. In this contract:

- **The coin** is the note. `Coin = H(owner_pub, value, token_id, spend_hook, user_data, blind)`
- **The nullifier secret** is possession. `Nullifier = H(secret, coin)`
- **To transfer**: burn the old note (exercising the capability to destroy it)
  and issue a new note to the bearer (the recipient's public key).
- **To redeem**: present the coin to the issuer contract via RedeemV1. The
  monetary value is destroyed, and a receipt coin — proof of redemption —
  is issued in its place.

The contract tracks what it needs to prevent double-spending (nullifiers) and
prove existence (Merkle roots). It does NOT track who owns what, what the values
are, or which tokens are being moved. Those are private by construction.

## Contract Functions

| Function | Opcode | Lifecycle Phase |
|----------|--------|-----------------|
| RegisterTypeV1 | `0x00` | A promise is made |
| RedeemV1 | `0x01` | The promise is honored |
| IssueV1 | `0x02` | The promise is exercised (coins minted) |
| RevokeV1 | `0x03` | Coins destroyed |
| TransferV1 | `0x04` | Coins circulate |
| OtcSwapV1 | `0x05` | Coins exchanged |

### RegisterTypeV1 (`0x00`)

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
    spend_hook: pallas::Base,     // Contract ID for cross-contract callback
}
```

**Entrypoint checks:**
- Coin is unique (not already in coins tree)

**State update:** Adds coin to coins tree and Merkle tree. Stores
`token_id → token_auth_parent` in token registry. Updates token registry
Merkle tree.

### RedeemV1 (`0x01`)

Closes the lifecycle: burns the input coin (destroying monetary value) and
creates a **zero-value receipt coin** — cryptographic proof that redemption
occurred with the issuer.

RedeemV1 is what makes the bearer instrument model complete. Without it,
IssueV1 opens a promise that can never be formally closed. A RevokeV1 (0x03)
destroys coins and publishes nullifiers, but a nullifier is a weak receipt —
it proves *some* coin was spent, not that it was spent *as a redemption with
the issuer.* A bridge operator cannot distinguish "coins the user transferred
elsewhere" from "coins the user redeemed with us." The nullifier carries no
semantics.

The receipt coin is:
- **Verifiable**: anyone can check the receipt exists in the coins tree
- **Semantic**: the receipt says "this specific token type was redeemed"
- **Non-transferable**: `spend_hook = issuer contract` prevents any TransferV1
  on the receipt — it's a permanent, non-circulating record
- **Issuer-visible**: the issuer scans for receipts of their `token_id` to
  compute their balance sheet entirely from on-chain data

```
Liabilities   = Σ IssueV1 outputs  (token_id = mine)
Redemptions   = Σ RedeemV1 inputs (token_id = mine)
Outstanding   = Liabilities - Redemptions
```

The receipt coin gives the redeemer a verifiable record. The nullifier in
the RedeemV1 input gives the issuer a verifiable record. Both sides of the
double-entry are on-chain.

**The is_notequal gate.** DarkWow's zkas circuit language includes
`is_notequal` — a boolean logic gate that returns 0 when two field elements
are equal and 1 when they differ. This gate is not available upstream.
RedeemV1's circuit uses it to prove that `coin_value == 0` — the receipt
has no monetary value — without revealing `coin_value` itself:

```
# is_notequal(value, 0) returns 0 when value == 0, 1 otherwise
# The entrypoint verifies zero_check == 0 → receipt is valid
zero_check = is_notequal(coin_value, 0);
constrain_instance(zero_check);
```

Without `is_notequal`, proving value = 0 would require either revealing the
value (destroying privacy) or a circuit-level constraint that leaks the
value to the verifier. `is_notequal` makes the proof fully private — the
verifier learns only that value IS zero, not what value was tested.

**Parameters:**
```rust
struct RedeemParamsV1 {
    input: Input,    // Coin being redeemed (standard RevokeV1 input)
    output: Output,  // Receipt coin (value=0, spend_hook=issuer)
}
```

**Entrypoint checks:**
- Merkle root exists in coin_roots tree (coin existed)
- Nullifier not already spent (no double-spend)
- Output coin is unique (receipt is new)
- **No value conservation** — RedeemV1 deliberately breaks the conservation
  invariant. Redemption IS value destruction from the system. The issuer
  fulfills the promise by releasing the underlying asset off-chain; the
  on-chain monetary value is destroyed.

### IssueV1 (`0x02`)

Mints new tokens of an existing type. Proves knowledge of the backing secret
against the stored `token_auth_parent`.

**Parameters:**
```rust
struct MintParamsV1 {
    coin: Coin,                      // The newly minted coin
    value_commit: pallas::Point,     // Pedersen value commitment
    token_id: pallas::Base,          // Token being minted
    token_registry_root: MerkleNode, // Proves token exists in registry
    mint_public: pallas::Base,       // H(mint_secret) — backing proof
    spend_hook: pallas::Base,        // Contract ID for cross-contract callback
}
```

**Entrypoint checks:**
- Coin is unique
- Token is registered (`token_id` exists in token registry)
- `mint_public == stored token_auth_parent` (backing capability match)
- `token_registry_root` matches current on-chain registry root

### RevokeV1 (`0x03`)

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
    spend_hook: pallas::Base,      // Verified by circuit as public input
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
the RevokeV1 and TransferV1 proofs verify independently — they each only
prove their own coin is well-formed. The entrypoint must bridge them.

The check uses Pedersen's additive homomorphism:

```
For each token_commit group:
    sum(input value_commits) == sum(output value_commits)
```

Where `sum` is EC point addition. Since `C(v1,b1) + C(v2,b2) = C(v1+v2, b1+b2)`,
the equality holds iff the total input value equals the total output value
for that token type.

This is enforced for TransferV1 and OtcSwapV1. **RedeemV1 deliberately
breaks it** — redemption IS value destruction. The entrypoint does not
enforce value conservation for RedeemV1; it verifies instead that the
receipt coin has value = 0 (ZK-constrained).

For OtcSwapV1, the token_commit pairing is:
- `inputs[0].token_commit` must equal `outputs[1].token_commit` (Alice→Bob)
- `inputs[1].token_commit` must equal `outputs[0].token_commit` (Bob→Alice)

## Token Model

### Coin

```
Coin = poseidon_hash(owner_pub, value, token_id, spend_hook, user_data, blind)
```

Where `owner_pub = poseidon_hash(DOMAIN_SIGNATURE_SECRET, secret)` — a
field element public key derived via Poseidon hash. This keeps the ZK
circuits on the base field without requiring EC multiplication for key
derivation.

### Value Commitment (Pedersen)

```
value_commit = pedersen_commit(value, value_blind)
             = value * G_value + value_blind * G_rand
```

Pedersen commitments are additively homomorphic:
`C(v1, b1) + C(v2, b2) = C(v1+v2, b1+b2)`.

This property lets the entrypoint enforce value conservation — sum of input
value_commits equals sum of output value_commits — without knowing any
plaintext values.

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
commitment — stored in the token registry when the token is created. IssueV1
proves knowledge of `mint_secret` against this stored value.

Token IDs are hidden from observers. The `token_commit = poseidon_hash(token_id, token_blind)`
in RevokeV1 and TransferV1 binds coins to a token type without revealing it.
The entrypoint groups inputs and outputs by `token_commit` to enforce per-token
value conservation — still without knowing the underlying token_id.

## ZK Circuits

Promissory Note uses 5 ZK circuits:

| Circuit | Namespace | Public Inputs | Purpose |
|---------|-----------|---------------|---------|
| `register_type_v1.zk` | `RegisterType_V1` | `token_id`, `token_auth_parent`, `coin`, `vc_x`, `vc_y`, `spend_hook` | Create token type |
| `redeem_v1.zk` | `Redeem_V1` | `coin`, `vc_x`, `vc_y`, `token_commit`, `coin_value`, `spend_hook` | Redeem receipt (value=0) |
| `issue_v1.zk` | `Issue_V1` | `token_root`, `mint_public`, `coin`, `vc_x`, `vc_y`, `token_id`, `spend_hook` | Mint with backing proof |
| `revoke_v1.zk` | `Revoke_V1` | `nullifier`, `vc_x`, `vc_y`, `token_commit`, `merkle_root`, `user_data_enc`, `spend_hook`, `signature_public` | Spend coins |
| `transfer_v1.zk` | `Transfer_V1` | `coin`, `vc_x`, `vc_y`, `token_commit`, `spend_hook` | Create output coins |

**Design principles:**
- Value commitments use **Pedersen** (not Poseidon). Pedersen is additively
  homomorphic — the property that makes cross-proof value conservation possible.
- Coin commitments use **Poseidon** — the standard ZK-friendly hash.
- Public keys are **field elements** (`H(secret)`), not EC points. This avoids
  EC multiplication in circuits where it isn't needed.
- Authorization uses **Poseidon-based public key derivation** where
  `signature_public = poseidon_hash(DOMAIN_SIGNATURE_SECRET, ephemeral_secret)`.
  The ephemeral secret MUST be fresh per transaction (never reuse the wallet
  secret — doing so links all spends to the same on-chain signature_public).
- `token_commit` is ZK-constrained in both RevokeV1 and TransferV1, enabling
  the entrypoint to group by token type for value conservation.
- **Redeem_V1 constrains `coin_value` as a public input** — the entrypoint
  verifies it is zero, proving the receipt has no monetary value. This is
  the boolean constraint pattern (`is_notequal`) available on DarkWow's zkas
  but not upstream.

### RevokeV1 Public Input Layout

```
[nullifier, value_commit_x, value_commit_y, token_commit,
 merkle_root, user_data_enc, spend_hook, signature_public]
```

### TransferV1 Public Input Layout

```
[coin, value_commit_x, value_commit_y, token_commit, spend_hook]
```

### RedeemV1 Public Input Layout

```
[coin, value_commit_x, value_commit_y, token_commit, coin_value, spend_hook]
```

`coin_value` is constrained to `pallas::Base::zero()` — the entrypoint
verifies this independently. This is functionally equivalent to the
`is_notequal` gate: the verifier learns that value IS zero, without
learning what value was tested.

The receipt coin's `spend_hook` is exposed as a public input, enabling
parent contracts to verify it is set to the issuer contract (preventing
transfer of receipt coins).

### IssueV1 Public Input Layout

```
[token_registry_root, mint_public, coin, value_commit_x, value_commit_y, token_id, spend_hook]
```

### RegisterTypeV1 Public Input Layout

```
[token_id, token_auth_parent, coin, value_commit_x, value_commit_y, spend_hook]
```

## Capability Lifecycle

Promissory Note follows the [object capability](../arch/ocap.md) model:

```
COMMIT: RegisterTypeV1
  token_auth_parent = H(mint_secret)
  → Stored in registry: token_id → token_auth_parent
  → "The backing secret exists, I control it"

EXERCISE (issuance): IssueV1
  mint_public = H(mint_secret)
  → Entrypoint checks mint_public == stored token_auth_parent
  → Mints new coins of this token type

EXERCISE (transfer): TransferV1
  nullifier = H(coin_secret, old_coin)
  → RevokeV1 proves: "I know secret for a coin in the tree"
  → TransferV1 proves: "New coin is well-formed"
  → "I consumed my capability and issued a new one to the recipient"

EXERCISE (redemption): RedeemV1
  nullifier = H(coin_secret, old_coin)
  → RevokeV1 proves: "I know secret for a coin in the tree"
  → Redeem_V1 proves: "Receipt coin is well-formed with value=0"
  → "I presented the note for redemption; the promise is honored"
  → Receipt is permanent, non-transferable, verifiable on-chain
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
   with the recipient's secret key
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

The `spend_hook` field is a `pallas::Base` embedded in every coin commitment.
When set to a non-zero value (typically a `ContractId`), burning that coin
triggers a **callback** to the target contract. When zero, no callback fires —
the burn is a plain destruction.

**Callback mechanism:**

1. `revoke_v1()` checks that all inputs share the same `spend_hook`. If they
   differ, it returns `SpendHookMismatch`.
2. If `spend_hook != 0`, PN builds a `RevokeSpendHookPayload` containing the
   caller's ContractId, all nullifiers, token commits, value commits, and
   encrypted user data.
3. PN calls `emit_spend_hook(target_cid, payload)`, which writes the callback
   request to the runtime environment.
4. After `exec()` returns, the blockchain pipeline loads the target contract's
   WASM, creates a Runtime in the same overlay, and calls `__spend_hook`
   followed by `apply()`.
5. If any step fails, the entire overlay is reverted — burn and callback are
   **atomic**.

**RevokeSpendHookPayload:**
```rust
struct RevokeSpendHookPayload {
    caller_contract_id: ContractId,    // PN contract that initiated the burn
    nullifiers: Vec<pallas::Base>,     // nullifiers being published
    token_commits: Vec<pallas::Base>,  // per-input token commitments
    value_commits: Vec<pallas::Point>, // per-input Pedersen value commitments
    user_data_encs: Vec<pallas::Base>, // per-input encrypted user data
}
```

Receiving contracts use `define_contract_with_spend_hook!` to export a
`__spend_hook` WASM function. See [Spend Hooks](../arch/zk/spend_hook.md) for
the full reference.

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
output's `value_commit` fields.

## Best Practices

### For Issuing Contracts (Stablecoin, Bridge, Wrapped Tokens)

Issuing contracts create tokens via RegisterTypeV1 and mint coins via IssueV1.
They are the **sole authority** for their token type and should enforce that
all burns route through them.

**Set spend_hook at mint time.** When calling `IssueV1` or `RegisterTypeV1`, set
`spend_hook` to your contract's `ContractId`. Every coin of your token type
will carry this hook, and every RevokeV1 will trigger a callback to you.

```
IssueV1 coin.spend_hook = my_contract_id
  → TransferV1 (preserves spend_hook in coin hash)
    → RevokeV1 (spend_hook matches → callback to my_contract_id)
```

**Implement the spend_hook receiver.** Switch from `define_contract!` to
`define_contract_with_spend_hook!` and implement `process_spend_hook`:

1. Verify `payload.caller_contract_id` is the expected PN contract
2. Check nullifiers for replay (store in DB, reject duplicates)
3. Build update data for the `apply` phase

**Track redemption state.** Record nullifiers from spend_hook callbacks to
compute outstanding supply: `Outstanding = Minted - Redeemed`.

**Verify spend_hook on incoming coins.** When your contract receives coins
(via TransferV1 child calls), verify `output.spend_hook` matches your
contract ID. Since all 5 ZK circuits now expose `spend_hook` as a public
input, this value is available in the proof metadata.

### For Intermediary Contracts (DEX, Game Room, Insurance Market)

Intermediary contracts move tokens between participants but do not issue
them. They receive coins from users and forward them to other users or
contracts.

**Verify spend_hook on received coins.** If your contract expects coins
with a specific spend_hook (e.g., only accepting coins bound to a particular
issuer), verify it in the proof metadata before processing.

**Don't set spend_hook unless you're an issuer.** Intermediaries should
typically set `spend_hook = pallas::Base::zero()` on outputs they create.
Setting a non-zero spend_hook would route burns through your contract —
only do this if you have a specific reason to intercept burns.

**Validate child call amounts.** Use `validate_child_value_commit` to
verify that child TransferV1 calls move the expected amounts. The
deterministic blind derivation pattern keeps values private while
enabling cross-contract verification.

### Safe Patterns

| Pattern | Do This | Avoid |
|---------|---------|-------|
| Single spend_hook per burn | All inputs must share the same spend_hook | Mixing inputs with different spend_hook values |
| Zero spend_hook for unrestricted coins | `spend_hook = pallas::Base::zero()` | Setting spend_hook without a receiver contract |
| Verify caller in receiver | Check `caller_contract_id` in process_spend_hook | Trusting the payload without caller verification |
| Track nullifiers | Store processed nullifiers in DB, check for duplicates | Processing callbacks without replay protection |
| Atomicity awareness | Callback failure reverts the burn — keep handlers fallible | Making external assumptions (oracle prices, cross-chain state) in spend_hook handlers |
| Test with zero spend_hook first | Verify burns work without callbacks before adding hook | Deploying spend_hook receiver without testing plain burns |

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
│       ├── register_type_v1.rs # RegisterTypeCallBuilder
│       ├── redeem_v1.rs     # RedeemCallBuilder
│       ├── issue_v1.rs       # IssueCallBuilder
│       ├── revoke_v1.rs       # RevokeCallBuilder + create_revoke_proof()
│       └── transfer_v1.rs   # TransferCallBuilder
├── proof/
│   ├── register_type_v1.zk     # RegisterType_V1 circuit
│   ├── redeem_v1.zk          # Redeem_V1 circuit (receipt coin, value=0)
│   ├── issue_v1.zk           # Issue_V1 circuit
│   ├── revoke_v1.zk           # Revoke_V1 circuit
│   ├── transfer_v1.zk   # Transfer_V1 circuit
│   └── *.zk.bin             # Compiled ZK binaries
├── tests/
│   └── integration.rs       # Integration tests
└── README.md
```

## Appendix: Why Separate from NativeToken?

### Relation to Upstream

Upstream DarkFi uses a single `money` contract that handles both consensus
tokens AND DeFi tokens. DarkWow splits this into two contracts:

- **NativeToken** — consensus-critical subset (fees, coinbase rewards)
- **Promissory Note** — ERC-20 style DeFi primitive (tokens, transfers, swaps)

This split follows a **CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD** philosophy:

| Priority | NativeToken | Promissory Note |
|----------|-------------|-----------------|
| 1. Consensus | PoWRewardV1 — block rewards | N/A |
| 2. Network Fees | FeeV1 — deterministic fee payment | N/A |
| 3. Privacy | N/A | Full privacy DeFi |

| Aspect | NativeToken | Promissory Note |
|--------|------------|-----------------|
| Purpose | Consensus (PoW rewards, fees) | DeFi tokens |
| Tokens | Single (DARK), hardcoded | Multiple, registered via RegisterType |
| Authorization | None (permissionless) | Backing capability proof |
| Deployment | Native (consensus-critical) | WASM genesis (ecosystem infrastructure) |
| Attack surface | Minimal | Broader (multi-token, composability) |

NativeToken is deliberately rock-dumb — no multi-token, no auth, no freezing —
to minimize consensus attack surface. If the consensus-critical native token
contract has a bug, the chain halts.

Promissory Note is a **WASM contract** deployed at genesis, not via Deployooor.
Despite being genesis-deployed, it is NOT consensus-critical — bugs in PN
cannot halt consensus, they only affect DeFi tokens. It carries the minimum
viable business logic for DeFi: token creation, minting with backing capability
proofs, private transfers, OTC swaps, and redemption.

### Genesis Deployment

PN is deployed at genesis alongside NativeToken and Deployooor. Its ContractId
is hardcoded as `PROMISSORY_NOTE_CONTRACT_ID` (poseidon hash of the prefix,
zero, and the constant 3). This provides a canonical well-known ID for every
DeFi contract that depends on it — bridge, stablecoin, DEX, escrow, bearer
bond, and lottery all store PN's contract ID for cross-contract routing.

Without a canonical PN, anyone could deploy replicas, fragmenting capability
resolution and breaking wallet discovery. Genesis deployment prevents this
while remaining entirely opt-in: the ecosystem is free to deploy alternative
token contracts via Deployooor. PN's genesis status is ecosystem infrastructure
— the same principle as ERC-20 pre-deploys on Ethereum testnets or the bank
module in Cosmos SDK.

See [NativeToken](native_token.md) for the native-side rationale.

## Related
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
 Contracts

- **[Stablecoin](stablecoin.md)** — Issues USDx tokens via RegisterTypeV1, manages
  CDP positions via IssueV1/RevokeV1 with spend_hook enforcement
- **[Bridge](bridge.md)** — Wraps external chain assets as promissory notes
- **[DEX](dex.md)** — Atomic swaps via OtcSwapV1
- **[NativeToken](native_token.md)** — Consensus token (DARK), the other half
  of the two-contract architecture

## References

- George Selgin, *Good Money: Birmingham Button Makers, the Royal Mint, and
  the Beginnings of Modern Coinage* (University of Michigan Press, 2008)
- [Object Capability Architecture](../arch/ocap.md)
- [Safety Patterns](../dev/contracts/safety.md) — Flakey pattern catalog and hardening principles
- [Composability](composability.md) — Cross-contract call patterns
- [Standards](../dev/contracts/standards.md) — Security standards reference
- [Intermediary Contract Audit](promissory_note_intermediaries.md) — Full audit of all
  22 PN-interacting contracts: spend_hook enforcement, redemption readiness, validation gaps
