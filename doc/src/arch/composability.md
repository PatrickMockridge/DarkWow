# Composability & General Primitives

*This document describes common patterns and primitives that appear across DarkFi smart contracts, enabling composition and interoperability.*

## The Problem: Contract-Specific Reasoning

Most smart contract designs reason about each contract in isolation:

```
Contract A: "handles token transfers"
Contract B: "handles identity"
Contract C: "handles DEX swaps"
```

**This approach fails** when contracts need to compose with each other. A DAO might need to verify identity credentials before allowing governance participation. A DEX might need to verify token balances from the Money contract. A bridge might need to interact with multiple token standards.

Without a common framework for reasoning about composability, each contract reinvents the same patterns.

## The Solution: General Primitive Categories

DarkFi contracts share three categories of general primitives:

| Category | Purpose | Appears In |
|----------|---------|------------|
| **State Primitives** | How contracts represent and store data | All contracts |
| **Authorization Primitives** | How contracts verify authority | All contracts |
| **Interaction Primitives** | How contracts communicate and compose | Cross-contract calls |

### State Primitives

All DarkFi contracts must represent state. The common patterns are:

```
┌─────────────────────────────────────────────────────────────────┐
│                    State Primitive Patterns                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. BINARY ACCUMULATOR                                           │
│     Represents: "element exists in set"                           │
│     Used in: Merkle trees, Bloom filters, RSA accumulators       │
│                                                                   │
│     ┌─────────┐         ┌─────────────┐                          │
│     │ Element │ ──────→ │   Merkle    │                          │
│     └─────────┘         │     Root     │                          │
│                         └─────────────┘                          │
│     Proof: "element is in set" without revealing element         │
│                                                                   │
│  2. INTERVAL TREE                                                │
│     Represents: "value exists in range"                           │
│     Used in: Balance ranges, time windows, credential expiration │
│                                                                   │
│     ┌─────────┐         ┌─────────────┐                          │
│     │  Value  │ ──────→ │ Interval    │                          │
│     └─────────┘         │    Tree     │                          │
│                         └─────────────┘                          │
│     Proof: "value is within bounds" without revealing value      │
│                                                                   │
│  3. HASH CHAIN                                                   │
│     Represents: "sequence of events in order"                    │
│     Used in: Transaction history, credential issuance order       │
│                                                                   │
│     ┌─────┐ → ┌─────┐ → ┌─────┐ → ┌─────┐                       │
│     │Event│   │Event│   │Event│   │Event│                       │
│     └─────┘   └─────┘   └─────┘   └─────┘                       │
│       │         │         │         │                            │
│       ▼         ▼         ▼         ▼                            │
│     H(0)      H(1)      H(2)      H(3)                           │
│                                                                   │
│     Proof: "event i happened before event j"                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Authorization Primitives

As detailed in [Private Authorization Layer](privauth.md), all DarkFi contracts share the same authorization pattern:

```
┌─────────────────────────────────────────────────────────────────┐
│              Shared Authorization Pattern                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  COMMITMENT          NULLIFIER           PROOF                   │
│  H(secret, params)  H(secret, ...)      ZK(prover knows secret) │
│       │                  │                    │                   │
│       │                  │                    │                   │
│       ▼                  ▼                    ▼                   │
│  ┌─────────────────────────────────────────────────────┐         │
│  │              Authorization Check                       │         │
│  │  • Commitment exists and is valid                     │         │
│  │  • Nullifier has not been spent                       │         │
│  │  • Proof verifies predicate without revealing secret   │         │
│  └─────────────────────────────────────────────────────┘         │
│                                                                   │
│  KEY INSIGHT: Every contract uses the same pattern.              │
│               Only the predicate changes.                         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## The Attestation Primitive

The [Attestation Contract](./attestation.md) provides a **generalized claims and attestation system** that enables cross-contract composition through a common pattern:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Attestation Pattern                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ATTESTOR → ATTESTATION → CLAIMANT → CLAIM → VALIDATION          │
│                                                                   │
│  ┌─────────────┐    ┌──────────────┐    ┌─────────────┐         │
│  │ Attestor    │───→│ Attestation  │───→│ Claimant    │         │
│  │ (issuer)    │    │ (commitment) │    │ (holder)    │         │
│  └─────────────┘    └──────────────┘    └──────┬──────┘         │
│                                                 │                 │
│                                                 ▼                 │
│                                           ┌─────────────┐         │
│                                           │    Claim    │         │
│                                           │ (assertion) │         │
│                                           └──────┬──────┘         │
│                                                  │                 │
│                                                  ▼                 │
│                                           ┌─────────────┐         │
│                                           │   Verify    │         │
│                                           │ (predicate) │         │
│                                           └──────┬──────┘         │
│                                                  │                 │
│                                                  ▼                 │
│                                           ┌─────────────┐         │
│                                           │   Consume   │         │
│                                           │ (nullifier) │         │
│                                           └─────────────┘         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Attestation vs Identity

| Aspect | Identity Contract | Attestation Contract |
|--------|------------------|---------------------|
| **Purpose** | ZK credential proofs using competency DAGs | Generalized attestation and claims |
| **Pattern** | Issuer issues credentials, holder proves | Attestor commits to data, claimant creates claim |
| **Predicate** | Custom ZK circuits | Standard predicates: Matches, GreaterOrEqual, LessOrEqual, Contains |
| **Replay Prevention** | Nullifier per claim | Nullifier per claim consumption |
| **Use Cases** | Competency verification, age checks | Deliverable verification, price feeds, oracle data |

The Attestation contract generalizes the claims pattern that appeared in Identity, Labor Market, and Tender.

### Predicates

The Attestation contract supports standard predicates:

| Predicate | Description |
|-----------|-------------|
| `Matches` | `evidence_commitment == claim_data[0]` |
| `GreaterOrEqual` | `value >= threshold` |
| `LessOrEqual` | `value <= threshold` |
| `Contains` | `data contains pattern` |
| `Custom` | Custom predicate via ZK circuit |

### State Machines

**Attestation State:**
```
Active ──[Revoke]──> Revoked
    │
    └──[Expire]──> Expired
```

**Claim State:**
```
Pending ──[Verify:valid]──> Verified ──[Consume]──> Consumed
    │
    └──[Verify:invalid]──> Rejected
```

### Contracts Using Attestation

| Contract | Attestation Usage |
|----------|-------------------|
| **Labor Market** | Employer attests to deliverable hash; worker claims completion |
| **Tender** | Requester attests to competency requirements; bidders claim competency |
| **Oracle** | Oracle attests to external data values; consumers claim and verify |

### Oracle: Push Model for External Data

The [Oracle Contract](./oracle.md) demonstrates the push model using Attestation:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Oracle + Attestation Flow                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Oracle Operator                                                             │
│     │                                                                       │
│     │  RegisterOracle(name, data_type)                                      │
│     ▼                                                                       │
│  Oracle(Active)                                                             │
│     │                                                                       │
│     │  PushValue(value)                                                    │
│     │                                                                       │
│     │  AttestValue(predicate, threshold)                                    │
│     ▼                                                                       │
│  Attestation(claim_data=[value]) ─────────────────────────────────────────►│
│                                                                              │
│                                              Consumer Contract               │
│                                                 │                            │
│                                                 │ CreateClaim(evidence)      │
│                                                 ▼                            │
│                                              Claim(Verified)                 │
│                                                 │                            │
│                                                 │ ConsumeClaim()             │
│                                                 ▼                            │
│                                              Contract Logic                  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Interaction Primitives

Cross-contract composition follows specific patterns:

 ```
┌─────────────────────────────────────────────────────────────────┐
│              Cross-Contract Interaction Patterns                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. CALL-CHAIN                                                   │
│     Contract A → Contract B → Contract C                          │
│                                                                   │
│     Problem: How does C verify A's authorization?                │
│                                                                   │
│     Solution: Pass proof along call chain                         │
│     A produces proof, B validates and forwards, C trusts B      │
│                                                                   │
│  2. STATE DEPENDENCY                                             │
│     Contract A reads state from Contract B                        │
│                                                                   │
│     Problem: How does A know B's state is valid?                  │
│                                                                   │
│     Solution: Merkle proofs + consensus verification              │
│     A verifies B's state hash is in consensus                    │
│                                                                   │
│  3. TOKEN TRANSFER                                               │
│     Contract A sends tokens to Contract B                        │
│                                                                   │
│     Problem: How do we prevent double-spending?                   │
│                                                                   │
│     Solution: Atomic transactions with dependent operations      │
│     Both operations in same transaction, all or nothing          │
│                                                                   │
│  4. ATTESTATION REFERENCE                                       │
│     Contract A uses attestation from Contract B                  │
│                                                                   │
│     Problem: How does A verify B's attestation without direct?   │
│                                                                   │
│     Solution: Store attestation_id, verify claim via Attestation │
│     A reads attestation_id, calls Attestation.verify_claim()    │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Cross-Contract Composability Matrix

| Caller → | Money | DAO | Bridge | DEX | Attestation | Oracle | Labor Market | Tender |
|----------|-------|-----|--------|-----|--------------|--------|-------------|--------|
| **Money** | - | Token transfers | Token escrow | Swap settlement | - | - | Job payment escrow | Bid deposit escrow |
| **DAO** | Treasury management | - | Governance of bridge | Governance of DEX | Attestation governance | - | Job approval governance | Tender authorization |
| **Bridge** | Cross-chain transfers | Relayer rewards | - | Liquidity provision | - | - | External job funding | External tender integration |
| **DEX** | Swap execution | Fee distribution | - | - | - | - | - | - |
| **Attestation** | - | - | - | - | - | Oracle data attestation | Deliverable verification | Competency verification |
| **Oracle** | Collateral pricing | - | - | Liquidity pricing | Creates attestations | - | - | - |
| **Labor Market** | Job payment settlement | Job DAO governance | External payment integration | - | Uses for delivery | - | - | Job creation from tender |
| **Tender** | Bid deposit management | - | External tender integration | - | Uses for competency | - | Winner job creation | - |

## General Primitive Composition Patterns

### Pattern 1: Token-Gated Access

```
┌─────────────────────────────────────────────────────────────────┐
│                 Token-Gated Access                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: User holds >= N tokens                            │
│                                                                   │
│  Implementation:                                                 │
│  1. User creates commitment: H(balance_secret, token, amount)  │
│  2. User generates ZK proof: "I know secret such that            │
│     commitment = H(secret, token, amount) AND amount >= N"     │
│  3. Contract verifies: commitment exists, proof valid            │
│                                                                   │
│  Privacy: Only reveals "amount >= N", not actual balance         │
│                                                                   │
│  Used in: DAO voting, premium features, liquidity pools           │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern 2: Attestation-Based Claims

```
┌─────────────────────────────────────────────────────────────────┐
│               Attestation-Based Claims                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: Attestor has attested to a claim                   │
│                                                                   │
│  Implementation:                                                 │
│  1. Attestor creates: attestation = H(data, attestor_key)     │
│  2. Claimant creates: claim = ZK proof of attestation access    │
│  3. Contract verifies: attestation exists, claim valid,           │
│     predicate satisfied                                           │
│                                                                   │
│  Privacy: Only reveals "valid claim", not underlying data        │
│                                                                   │
│  Used in: Deliverable verification, competency claims,            │
│           oracle data consumption, event attestation               │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern 3: Time-Locked Actions

```
┌─────────────────────────────────────────────────────────────────┐
│                  Time-Locked Actions                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: Action can only happen after timestamp T           │
│                                                                   │
│  Implementation:                                                  │
│  1. State includes: time_lock = H(T, action_description)        │
│  2. Consensus ensures: block.timestamp >= T                      │
│  3. Contract verifies: current time >= lock time                │
│                                                                   │
│  Privacy: Locked action description hidden until unlock          │
│                                                                   │
│  Used in: Vesting schedules, delayed withdrawals, expiration      │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern 4: Multi-Signature Authorization

```
┌─────────────────────────────────────────────────────────────────┐
│              Multi-Signature Authorization                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: N of M parties must sign                           │
│                                                                   │
│  Implementation:                                                 │
│  1. Each party creates: partial_sig_i = sign(secret_i, msg)     │
│  2. Aggregator combines: full_sig = combine(partial_sigs)        │
│  3. Contract verifies: threshold met, all signers authorized    │
│                                                                   │
│  Privacy: Individual signers revealed only if needed             │
│                                                                   │
│  Used in: DAO proposals, bridge admin keys, upgrade gates        │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Case Studies

### Case Study: Subscription + DAO-Escrow + Atomic Swap

The Subscription contract demonstrates DarkFi's full composability stack: DAO-Escrow membership verification via Merkle proofs, block-based time locks, and cross-chain atomic swap payments.

```
┌─────────────────────────────────────────────────────────────────────────┐
│           Subscription + DAO-Escrow + Atomic Swap Composability              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────────┐         ┌──────────────────────┐              │
│  │     DAO-Escrow       │         │    Subscription      │              │
│  │                      │         │                       │              │
│  │  ┌────────────────┐  │         │  ┌────────────────┐  │              │
│  │  │ pay_premium()  │──┼───┐     │  │ subscribe()    │  │              │
│  │  └────────────────┘  │   │     │  └───────┬────────┘  │              │
│  │                      │   │     │          │            │              │
│  │  State: Merklized    │   │     │  Verifies via:      │              │
│  │  Membership tree     │   │     │  ┌────────▼────────┐ │              │
│  │                      │   │     │  │ Merkle proof   │ │              │
│  │                      │   │     │  │ + expiry check │ │              │
│  │                      │   │     │  │ + pubkey link  │ │              │
│  └──────────────────────┘   │     │  └────────────────┘  │              │
│                             │     │                       │              │
│         ┌───────────────────┘     └───────────────────────┘              │
│         │                           │                                      │
│         │    Cross-Contract         │                                      │
│         │    ZK Verification        │                                      │
│         ▼                           ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐        │
│  │                   Composability                               │        │
│  │                                                             │        │
│  │  No direct state sharing!                                   │        │
│  │  Pure Merkle proof verification.                             │        │
│  │  Nullifiers prevent double-spending.                         │        │
│  └─────────────────────────────────────────────────────────────┘        │
│                                                                          │
│  ┌──────────────────────┐         ┌──────────────────────┐              │
│  │    Atomic Swap       │         │    Subscription      │              │
│  │                      │         │                       │              │
│  │  ┌────────────────┐  │         │  ┌────────────────┐  │              │
│  │  │ CreateSwap()   │──┼─────────┼──│ SubscribeV1()  │  │              │
│  │  │ + HTLC        │  │         │  │ + hash link   │  │              │
│  │  └────────────────┘  │         │  └───────┬────────┘  │              │
│  │                      │         │          │            │              │
│  │  External chain      │         │  Cross-chain            │              │
│  │  funding flow        │         │  payment settlement     │              │
│  └──────────────────────┘         └───────────────────────┘              │
└─────────────────────────────────────────────────────────────────────────┘
```

#### Three-Mode DAO-Escrow

DAO-Escrow operates in three modes, configurable at deployment:

| Mode | Constant | Description | Subscription Use |
|------|----------|-------------|------------------|
| Escrow | `MODE_ESCROW = 0` | Pure insurance pool | Deposits held as insurance |
| Treasury | `MODE_TREASURY = 1` | Same as DarkFi DAO | Subscription fees fund governance |
| Treasury+Endowment | `MODE_TREASURY_ENDOWMENT = 2` | Combined treasury + endowment | Fees split: treasury + endowment share |

The Subscription contract splits each payment:

```rust
struct FeeConfig {
    treasury_share: u64,      // Goes to DAO treasury
    endowment_share: u64,    // Goes to endowment fund
}
```

#### Block-Based Time Locks

Unlike timestamp-based locks (which require oracles), DarkFi uses deterministic block numbers:

```rust
// DAO-Escrow: membership expiry
lock_until_block = current_block + duration_blocks;
less_than_strict(current_block, expiry);  // Verify not expired

// Subscription: subscription period
lock_until_block = current_block + plan.duration_blocks;
less_than_strict(current_block, lock_until_block);  // Still active
```

**Advantage**: Miners cannot manipulate block numbers the way they can timestamps.

#### Cross-Chain Atomic Swap Flow

```
Ethereum                          DarkFi
    │                                 │
    │  1. User locks ETH in HTLC      │
    │     hash = SHA256(secret)        │
    │ ───────────────────────────────► │
    │                                 │  2. Verify hash matches
    │                                 │  3. SubscribeV1 executes
    │                                 │     (DAO-Escrow membership
    │                                 │      verified via Merkle)
    │                                 │
    │  4. Reveal secret              │  5. Subscription activated
    │ ───────────────────────────────► │
```

The atomic swap's HTLC pattern ensures:
- **Atomicity**: Either both chains complete, or neither
- **Hashlock**: Only secret holder can claim
- **Timelock**: Refund guaranteed after expiration

#### Composability Principles Applied

| Principle | How It's Applied |
|-----------|------------------|
| **Merkle State** | DAO-Escrow stores memberships in Merkle tree; Subscription verifies via Merkle proof |
| **ZK Verification** | All three contracts use ZK proofs for authorization without revealing secrets |
| **Nullifier Namespace** | Each contract has its own nullifier namespace; Subscription nullifier unique per subscriber |

This pattern enables:
- Tiered services (DAO-Escrow members get subscription benefits)
- Insurance-backed subscriptions (endowment fund covers failures)
- Cross-chain membership (atomic swap funds subscription from external chains)
- Privacy-preserving access control (Merkle proofs don't reveal membership details)

#### Contract Integration Points

| From | To | Integration |
|------|----|-------------|
| Subscription | DAO-Escrow | `SubscribeV1` verifies DAO-Escrow membership via Merkle proof |
| Atomic Swap | Subscription | Swap proceeds fund subscription payment; hash links swap to subscription |
| DAO-Escrow | Subscription | Membership note proves insurance eligibility |

See [Subscription Contract](subscription.md), [DAO-Escrow Contract](dao_escrow.md), and [Atomic Swap Contract](atomic_swap.md) for full details.

### Case Study: How DAO-Escrow + Subscription Fixes Real-World DAO Failures

The failure modes of historical DAOs — particularly treasury management failures and governance paralysis — reveal why naive DAO designs fail in practice. The DAO-Escrow + Subscription architecture addresses these directly.

#### The Problem: Transparency and Accountability in Pure Treasury DAOs

AssangeDAO raised approximately $55 million in a 2022 token sale, with DarkFi founders Amir Taaki and Rachel Rose O'Leary among its founding members. The DAO's stated purpose was to bid on Julian Assange's NFT (a controversial strategy even at the time), but the auction ultimately failed — not because of lack of funds, but because no other bidders participated, knowing the DAO's treasury was large enough to outbid anyone.

The failure modes were predictable:

| Failure Mode | What Happened | Root Cause |
|-------------|---------------|------------|
| **Auction manipulation** | No other bidders participated because the transparent treasury revealed the DAO's maximum bid | Treasury transparency created information asymmetry |
| **Governance paralysis** | Taaki and O'Leary cited legal liability concerns as reasons members abstained from governance votes | No clear accountability for outcomes vs. decisions |
| **Treasury disappearance** | The $55M vanished with no clear accounting or service delivery documented | No escrow mechanism, pure trust model |
| **Free rider problem** | External actors exploited knowledge of treasury size to extract value in downstream negotiations | Public treasury balance invited extraction |
| **Insurance gap** | No protection against service failure or mismanagement | Pure treasury model has no consumer protection |

The founders' explanation — that the failed auction and legal liability fears caused governance paralysis — explains some failures. But it does not explain where the $55 million went or how it was ultimately spent. A DAO with proper escrow, service delivery, and insurance mechanisms would have had far stronger accountability for treasury management.

#### DarkFi Founders' Lessons Applied

The involvement of Amir Taaki and Rachel Rose O'Leary in both DarkFi and the earlier cypherpunk movements informs the architecture directly. The problems they encountered in practice shaped the design decisions:

**Transparency as a liability**: In a pure treasury DAO, public treasury balances become a negotiating liability. DAO-Escrow stores state in Merkle trees — membership can be verified without revealing total treasury holdings.

**Outcome-based governance**: Legal liability fears paralyzed voting because members felt responsible for every decision. DAO-Escrow separates governance (voting on parameters) from service delivery (enforced via escrow and endowment).

**Consumer protection via endowment**: A pure treasury DAO has no mechanism to refund members if services aren't delivered. DAO-Escrow's endowment fund accumulates a portion of each subscription as an insurance reserve, governed by DAO vote for drawdown.

#### How DAO-Escrow + Subscription Solves Each Problem

**1. Treasury Privacy via Merkle State**

A pure treasury DAO reveals all balances publicly. DAO-Escrow stores treasury state in a Merkle tree:

```zk
# DAO-Escrow membership tree stores:
# - Member stake
# - Voting weight
# - Treasury share entitlement

dao_root = merkle_root(leaf_position, membership_path, membership_note);
```

Outside observers cannot see the total treasury balance — only that a given membership is valid. This prevents the auction manipulation problem: bidders cannot know the DAO's max bid because the treasury balance isn't publicly visible.

**2. Service Delivery via Subscription**

Instead of donating to a treasury with no strings attached, members pay for defined services:

```rust
struct SubscriptionPlan {
    plan_id: u64,
    price: TokenAmount,
    duration_blocks: u64,
    service_description: "Weekly magazine + podcast access",
}

struct Subscription {
    subscriber: PublicKey,
    plan_id: u64,
    lock_until_block: u64,
    endowment_share: u64,  // Insurance reserve
}
```

The endowment_share of each payment accumulates in an insurance reserve. If the service fails to deliver, the DAO can authorize refunds from the endowment fund via governance.

**3. Block-Based Time Locks (No Oracle Manipulation)**

Traditional DAOs use timestamps which miners can manipulate. DarkFi uses deterministic block numbers:

```rust
// Subscription: lock_until is a block height, not a timestamp
lock_until_block = current_block + plan.duration_blocks;
less_than_strict(current_block, lock_until_block);  // Still active?

// DAO-Escrow: membership expiry is a block height
less_than_strict(current_block, membership_expiry);
```

**Advantage**: A block number N means "the Nth block in the chain" — not an approximation of time. Miners cannot manipulate block numbers the way they can timestamps.

**4. Triple-Mode DAO-Escrow for Risk Distribution**

DAO-Escrow supports three operating modes:

| Mode | Description | Use Case |
|------|-------------|----------|
| `MODE_ESCROW` | Pure insurance pool, no treasury | Consumer protection, service escrow |
| `MODE_TREASURY` | Same as DarkFi DAO, governance funding | Traditional DAO treasury |
| `MODE_TREASURY_ENDOWMENT` | Treasury + endowment split | Service + insurance combined |

A news organization using this architecture might configure:
- **Treasury share** (e.g., 70%): Funds operations, governance
- **Endowment share** (e.g., 30%): Insurance reserve for refunds if publication fails to deliver

**5. Nullifier Namespace Prevents Double-Spending**

Each contract has its own nullifier namespace:

```rust
// Subscription nullifier
subscription_nullifier = poseidon_hash(subscriber, subscription_id);

// DAO-Escrow membership nullifier
membership_nullifier = poseidon_hash(member_pubkey, membership_note);

// Different namespaces — no collision, no double-spend possible
```

**6. Accountability via On-Chain Record**

Every state transition is recorded:

| Action | What Gets Recorded |
|--------|-------------------|
| Subscribe | Commitment (private), plan_id, duration, endowment_share |
| Cancel | Nullifier (spent), refund_amount at lock_until_block |
| Service deliver | No record needed — service is off-chain |
| Refund from endowment | DAO governance vote recorded, payout executed |
| Membership expiry | Automatic via block height check |

#### Comparison: Naive DAO vs DAO-Escrow + Subscription

| Dimension | Naive Treasury DAO | DAO-Escrow + Subscription |
|-----------|---------------------|--------------------------|
| Treasury visibility | Fully transparent | Merkle-protected |
| Member accountability | Token ownership only | Subscription + service delivery |
| Consumer protection | None | Endowment fund escrow |
| Governance paralysis risk | High (legal fear) | Lower (outcome-based, not decision-based) |
| Treasury extraction | Easy (public balance) | Harder (obscured by Merkle state) |
| Service failure recourse | None | DAO can trigger endowment refund |
| Cross-chain funding | Difficult | Atomic swap integration |

#### Real-World Application: News Organization DAO

Imagine a privacy-preserving news organization funded via DAO-Escrow + Subscription:

1. **Reporter** pays subscription in DRK via atomic swap from Ethereum
2. **Subscription** activates with endowment_share flowing to insurance reserve
3. **Publication** delivers weekly magazine, podcast, blog — off-chain, but membership verified on-chain
4. **If publication fails**: DAO governance votes to trigger endowment refunds
5. **If publication succeeds**: Treasury funds operations, endowment grows as insurance buffer

The $55M problem doesn't happen because:
- The money wasn't sitting in a transparent treasury waiting to be extracted
- Members paid for services, not donated to a black box
- The endowment provided insurance against failure
- Block-based time locks prevented timestamp manipulation

This architecture would have solved the accountability problems that plagued organizations like AssangeDAO — where the inability to vote on governance (due to legal liability fears) and the transparent treasury (which invited extraction) led to failure.

### Case Study: Tender + Labor Market + Attestation

The Tender and Labor Market contracts demonstrate DarkFi's integration of sealed-bid procurement with attestation-based competency verification and job execution, forming a complete project lifecycle from tender to delivery.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│              Tender + Labor Market + Attestation Composability                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────────────┐         ┌──────────────────────┐                  │
│  │    Attestation       │         │       Tender         │                  │
│  │                      │         │                       │                  │
│  │  ┌────────────────┐  │         │  ┌────────────────┐  │                  │
│  │  │ CreateAttest() │──┼─────────┼──│ SubmitBidV1()   │  │                  │
│  │  │ + claim_data   │  │         │  │ + claim_id     │  │                  │
│  │  └────────────────┘  │         │  └───────┬────────┘  │                  │
│  │                       │         │          │            │                  │
│  │  Attestation          │         │  Sealed Bid          │                  │
│  │  Reference            │         │  + Claim ID         │                  │
│  └───────────────────────┘         └───────────────────────┘                  │
│                                    │                                           │
│                                    │    Winner Selected                        │
│                                    ▼                                           │
│  ┌────────────────────────────────────────────────────────────────┐           │
│  │                    Labor Market                                  │           │
│  │                                                                 │           │
│  │  ┌──────────────────────────────────────────────────────────┐  │           │
│  │  │ CreateJobV1()                                             │  │           │
│  │  │ - Creates job from tender winner                          │  │           │
│  │  │ - Sets attestation_id from tender specification            │  │           │
│  │  │ - Sets payment_amount from winning bid                     │  │           │
│  │  │ - Sets deadline from tender delivery_deadline              │  │           │
│  │  └──────────────────────────────────────────────────────────┘  │           │
│  │                                                                 │           │
│  │  ┌──────────────────────────────────────────────────────────┐  │           │
│  │  │ SubmitDeliverableV1()                                     │  │           │
│  │  │ - Worker submits claim_id from attestation                 │  │           │
│  │  │ - Labor market verifies claim via Attestation contract    │  │           │
│  │  └──────────────────────────────────────────────────────────┘  │           │
│  └────────────────────────────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Tender State Machine

The tender contract implements a sealed-bid workflow:

```
Created ──[SubmitBid]──> Bidding ──[Close]──> Revealed ──[Select]──> Awarded
                                               │
                                               └──[Cancel]──> Cancelled
```

#### Bid State Machine

Bids transition through sealed and revealed states:

```
Sealed ──[Reveal]──> Revealed ──[Accept]──> Accepted
  │                        │
  └──[Timeout]──> Expired  └──[Reject]──> Rejected
```

#### Integration with Attestation

Workers prove competency via the Attestation contract:

```rust
// Requester creates attestation for competency requirements
let attestation_id = attestation::create_attestation(
    attestor: requester_pubkey,
    claim_type: Predicate::Matches,
    claim_data: vec![requirement_commitment],
)?;

// Worker creates claim proving they meet requirements
let claim_id = attestation::create_claim(
    attestation_id,
    claimant: worker_pubkey,
    predicate: Predicate::Matches,
    evidence_commitment: worker_competency_commitment,
)?;

// Worker submits bid with claim_id
struct SubmitBidParamsV1 {
    claim_id: pallas::Base,  // From Attestation.create_claim
    // ...
}
```

#### Integration with Labor Market

Winner selection automatically creates a job:

```rust
// SelectWinnerV1 creates labor job with:
struct SelectWinnerParamsV1 {
    winner_pubkey: PublicKey,         // From winning bid
    winning_amount: u64,              // From revealed bid
    // Tender's attestation_id, delivery_deadline used for job
}

// Labor Market job creation:
// - attestation_id = tender.attestation_id
// - payment_amount = winning_amount
// - deadline_block = tender.delivery_deadline
// - employer_pubkey = tender.requester_pubkey
// - worker_pubkey = winner_pubkey
```

#### Attestation Flow for Deliverables

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                Labor Market + Attestation Deliverable Flow                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Employer                                                                   │
│     │                                                                       │
│     │ CreateAttestation(deliverable_hash)                                   │
│     ▼                                                                       │
│  Attestation(Active)                                                        │
│     │                                                                       │
│     │ attestation_id                                                        │
│     │                                                                       │
│     │◄──────────────────────────────── CreateJob(job, attestation_id)      │
│     │                                                                       │
│     │                              Worker                                   │
│     │                                 │                                     │
│     │                                 │ CreateClaim(evidence_commitment)   │
│     │                                 ▼                                     │
│     │                              Claim(Verified)                          │
│     │                                 │                                     │
│     │                                 │ claim_id                            │
│     │                                 │                                     │
│     │◄── SubmitDeliverable(job_id, claim_id)                              │
│     │                                                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Composability Principles Applied

| Principle | How It's Applied |
|-----------|------------------|
| **Sealed Bids** | `poseidon_hash(amount, nonce)` hides bid until reveal deadline |
| **Attestation Verification** | Attestation contract verifies competency/deliverable claims |
| **State Machine** | Tender/Bid states prevent invalid transitions |
| **Nullifier Namespace** | Bid submission and reveal use separate nullifiers |
| **Escrow Integration** | Bid deposits held in escrow; refundable if outbid or cancelled |

#### Contract Integration Points

| From | To | Integration |
|------|----|-------------|
| Attestation | Tender | `SubmitBid` verifies claim_id from Attestation |
| Attestation | Labor Market | `SubmitDeliverable` verifies claim_id for deliverable |
| Tender | Labor Market | `SelectWinner` creates job with tender's specification and winning bid details |
| Labor Market | Attestation | Worker creates claim on attestation for deliverable verification |

#### Tau Task Tracking Integration

Jobs created from tender winners can be tracked via Tau:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Tender → Labor Market → Tau Flow                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Requester creates Tender with specifications                 │
│     ↓                                                             │
│  2. Workers submit sealed bids with attestation claims           │
│     ↓                                                             │
│  3. Winner selected → Job created in Labor Market                │
│     ↓                                                             │
│  4. Job tracked in Tau for delivery monitoring                   │
│     ↓                                                             │
│  5. Worker submits deliverables → Attestation verifies claim       │
│     ↓                                                             │
│  6. Payment released via escrow                                   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

See [Tender Contract](tender.md), [Labor Market Contract](labor_market.md), [Attestation Contract](attestation.md), [Oracle Contract](oracle.md), and [Tau](../../misc/tau.md) for full details.

### Case Study: Oracle + Attestation + DeFi

The Oracle contract demonstrates using Attestation for external data integration:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Oracle + Attestation + DeFi Flow                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Price Oracle                                                               │
│     │                                                                       │
│     │  RegisterOracle("BTC/USD", "price")                                  │
│     ▼                                                                       │
│  Oracle(Active)                                                             │
│     │                                                                       │
│     │  PushValue(50000)  // BTC/USD price                                  │
│     │                                                                       │
│     │  AttestValue(GreaterOrEqual, 45000)  // Liquidation threshold        │
│     ▼                                                                       │
│  Attestation(claim_data=[50000])                                            │
│     │                                                                       │
│     │ attestation_id                                                        │
│     │                                                                       │
│     │◄─────────────────────────────── Stablecoin Contract                 │
│     │                                                                       │
│     │                              Worker/Bot                               │
│     │                                 │                                     │
│     │                                 │ CreateClaim(poseidon_hash(50000))  │
│     │                                 ▼                                     │
│     │                              Claim(Verified)                          │
│     │                                 │                                     │
│     │                                 │ if price < threshold:               │
│     │                                 │    liquidate_position()            │
│     │                                 ▼                                     │
│     │                              Payout                                  │
│     │                                                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Oracle Use Cases

| Use Case | Oracle Pushes | Attestation Predicate | Consumer Action |
|----------|---------------|----------------------|-----------------|
| DeFi Liquidation | Token price | LessOrEqual threshold | Liquidate CDP |
| Sports Betting | Game outcome | Matches team | Settle bets |
| Gaming Randomness | Random value | Matches committed | Mint NFT |
| Weather Insurance | Weather data | LessOrEqual threshold | Trigger payout |

See [Oracle Contract](oracle.md) and [Attestation Contract](attestation.md) for full details.

## SDK Primitives

The DarkFi SDK provides reusable primitives for contracts:

### Generic Intent Primitives (`src/sdk/src/crypto/intent.rs`)

The `PrivateIntent` struct provides a reusable authorization pattern:

```rust
use darkfi_sdk::crypto::{PrivateIntent, IntentCommitment, IntentNullifier};

// Create an intent
let intent = PrivateIntent::new(
    owner_pubkey,
    namespace,        // Scopes to identity/bridge/DEX/etc.
    payload_hash,   // H(application-specific data)
    expiry,         // Block height expiration
    nonce,          // Prevents replay
    blind,          // Additional blinding
);

// Get commitment for on-chain storage
let commitment = intent.commitment();  // IntentCommitment

// Derive nullifier when consuming
let nullifier = intent.derive_nullifier(owner_secret)?;  // IntentNullifier
```

### Intent-Set State Machine (`src/sdk/src/crypto/intent_set.rs`)

The `IntentSetIndexV1` provides a generic state machine:

```rust
use darkfi_sdk::crypto::{IntentSetIndexV1, IntentPostTransitionV1, IntentConsumeTransitionV1};

let mut index = IntentSetIndexV1::new();

// Post new intent
let post = IntentPostTransitionV1 { ... };
index.validate_post(&post)?;
index.apply_post(&post)?;

// Consume intent (fill/cancel)
let consume = IntentConsumeTransitionV1 { ... };
index.validate_consume(&consume)?;
index.apply_consume(&consume)?;
```

### Contract Function Macro (`src/sdk/src/primitives.rs`)

Use `define_contract_function!` to define contract functions:

```rust
use darkfi_sdk::define_contract_function;

define_contract_function!(MyContract {
    InitializeV1 = 0x00,
    DoActionV1 = 0x01,
});
```

### Commitment/Nullifier Helpers (`src/sdk/src/primitives.rs`)

Low-level commitment and nullifier computation:

```rust
use darkfi_sdk::primitives::{compute_commitment, compute_nullifier};

let commitment = compute_commitment::<2>([secret, param1]);
let nullifier = compute_nullifier(secret, commitment);
```

### Transition Payload Encoding (`src/sdk/src/crypto/transition_payload.rs`)

Helper functions for encoding/decoding transition payloads:

```rust
use darkfi_sdk::crypto::{encode_intent_set_post_v1, decode_intent_set_post_v1};

let payload = encode_intent_set_post_v1(&transition)?;
let decoded = decode_intent_set_post_v1(&payload)?;
```

### Tree Name Helper (`src/sdk/src/primitives.rs`)

Generate consistent tree names:

```rust
use darkfi_sdk::primitives::tree_name;

pub const MY_STATE_TREE: &str = tree_name!("mycontract", "state");
// Results in: "mycontract_state"
```

## Relationship to Existing DarkFi Work

This framework builds on and integrates with existing DarkFi patterns:

### Existing Groundwork in DarkFi

DarkFi `master` already contains relevant foundational work:

| Component | Location | Purpose |
|-----------|----------|---------|
| Anonymous bridge draft | `doc/src/arch/bridge.md` | Early design for cross-chain privacy |
| DEX direction | `doc/src/arch/dex.md` | Uses `spend_hook` and `auth_otc` |
| OTC swap settlement | `src/contract/money/src/entrypoint/swap_v1.rs` | Real swap plumbing in money contract |

### Broader Framing: Private Authorization Layer

The intent primitives in this SDK are **not** specific to AMM or DEX. They implement a **private authorization layer** that appears across all DarkFi privacy-heavy contracts:

```
┌─────────────────────────────────────────────────────────────────┐
│        The Shared Pattern Across All Privacy Contracts                │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Bridge:                                                          │
│    DepositParams.commitment → WithdrawParams.nullifier            │
│    "I know the secret for this deposit"                          │
│                                                                   │
│  Attestation:                                                     │
│    Attestation.id → Claim.id → Claim.nullifier                   │
│    "I have a valid claim on this attestation"                   │
│                                                                   │
│  DEX:                                                             │
│    CreateSwapParams.lock_commitment → AcceptSwapParams.lock_commitment │
│    "I have locked funds for this swap"                           │
│                                                                   │
│  Stablecoin:                                                      │
│    OpenPositionParams.commitment → LiquidateParams.nullifier       │
│    "I own this CDP"                                              │
│                                                                   │
│  KEY INSIGHT: Same lifecycle, different predicates.                │
│               The machinery is reusable; the proof is not.       │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

This suggests DarkFi's privacy architecture benefits from **reusable private authorization / claim machinery** rather than ad-hoc commitment/nullifier patterns per contract.

### Namespace Scoping

Each application domain uses a namespace constant to scope intents:

| Contract | Namespace | Purpose |
|----------|-----------|---------|
| Identity | `0x0001` | Credential claims and verifications |
| Attestation | `0x0002` | Generalized attestation and claims |
| Bridge | `0x0003` | Cross-chain deposit/withdrawal |
| DEX | `0x0004` | Atomic swaps and exchange |
| Stablecoin | `0x0005` | CDP positions and liquidation |

Namespace separation allows the same `PrivateIntent` primitives to work across all privacy-preserving contracts without collision.

### Predicate Expressiveness and the Opcode Layer

The authorization primitives above define the **structure** of how contracts authorize actions.
The **expressiveness** of what predicates can be verified is determined by the zkVM opcode layer.

Current ZK circuits can verify:
- `amount > 0` (via `BoolCheck` and `RangeCheck`)
- Commitment validity (via `ConstrainEqualBase`)
- Merkle membership (via `MerkleRoot`)

But predicates like `attribute >= threshold` or `collateral >= 2 * debt` require comparison
opcodes that return values — currently missing from the zkVM.

See [zkVM Primitive Layer](zkvm_primitives.md) for the full analysis of:
- Why `LessThanOrEqual` and `IsEqualBase` are systematically needed
- How they compose with existing opcodes
- What each opcode unlocks across identity, stablecoin, DEX, and AMM use cases

This is **not blocking** current contracts from functioning — they use placeholder constraints
that always pass. But unlocking full predicate expressiveness requires implementing these
opcodes in the zkVM.

### Relationship to AMM/DEX Work

The DEX contract uses these primitives but is **not limited to AMM-style exchange**:

- The intent-set lifecycle supports: atomic swaps, intent-based matching, order book styles
- The `IntentSetIndexV1` state machine validates: post/consume transitions generically
- Actual AMM semantics (constant-product, TWAP pricing) are **application-specific**, built on top of these primitives

The primitives solve the **authorization and lifecycle problem**; the application layer solves the **pricing and matching problem**.

## Designing New Contracts: General Primitive Checklist

When designing a new DarkFi contract:

```
┌─────────────────────────────────────────────────────────────────┐
│         New Contract Design Checklist                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  State Primitives:                                                │
│  □ What state does this contract hold?                           │
│  □ Can state be expressed as Merkle tree / accumulator?          │
│  □ What are the state transition rules?                           │
│                                                                   │
│  Authorization Primitives:                                         │
│  □ What actions require authorization?                            │
│  □ Can all authorization be expressed as commitment/nullifier?     │
│  □ What predicates must be satisfied?                             │
│  □ Is revocation needed?                                          │
│  □ Can I use Attestation instead of building custom claims?       │
│                                                                   │
│  Interaction Primitives:                                          │
│  □ What other contracts does this call?                           │
│  □ Does Attestation already provide what I need?                  │
│  □ What state does this read from other contracts?                │
│  □ What tokens does this contract manage?                         │
│  □ How are atomic transactions handled?                           │
│                                                                   │
│  Privacy Analysis:                                                │
│  □ What information is revealed?                                  │
│  □ Can we use ZK proofs to hide more?                             │
│  □ What is the minimal disclosure?                                │
│  □ Can different users share the same proof?                      │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## The Incremental Transparency Framework

All DarkFi contracts should support incremental transparency (see [Identity](identity.md)):

| Level | Name | What Is Revealed | Use Case |
|-------|------|-----------------|----------|
| **0** | `zk_only` | Nothing | Maximum privacy |
| **1** | `predicate` | Predicate result only | Basic verification |
| **2** | `attested` | Issuer attestation | Trusted interactions |
| **3** | `public` | Full disclosure | Regulatory compliance |

```
┌─────────────────────────────────────────────────────────────────┐
│            Incremental Transparency Implementation                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Contract State:                                                  │
│  ┌─────────────────────────────────────────────────────┐         │
│  │ visibility_level: u8  // 0, 1, 2, or 3              │         │
│  │ commitment: H(data)  // Always stored               │         │
│  │ nullifier: H(secret)  // For spent state            │         │
│  │ plaintext: Option<data>  // Only if level >= 3      │         │
│  └─────────────────────────────────────────────────────┘         │
│                                                                   │
│  ZK Circuit:                                                      │
│  assert(visibility_level >= required_level)                      │
│  if level >= 1: assert(predicate(data) == true)                   │
│  if level >= 2: assert(issuer_signature.valid)                   │
│  if level >= 3: reveal(data)                                      │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## References

- [Private Authorization Layer](privauth.md)
- [zkVM Primitive Layer](zkvm_primitives.md) — opcode-level reasoning for contract expressiveness
- [Contract MVP Status](mvp_status.md) — blockers for each contract in the contracts folder
- [Attestation Contract](attestation.md) — Generalized attestation and claims
- [Oracle Contract](oracle.md) — Push-model oracle with attestation
- [Identity Contract](identity.md)
- [Bridge Contract](bridge.md)
- [DEX Contract](../dev/contracts/dex.md)
- [Stablecoin Contract](../dev/contracts/stablecoin.md)
- [Subscription Contract](subscription.md)
- [DAO-Escrow Contract](dao_escrow.md)
- [Atomic Swap Contract](atomic_swap.md)
- [Labor Market Contract](labor_market.md)
- [Auction Contract](auction.md)
- [Tender Contract](tender.md)
- [Intent AMM Proposal](https://codeberg.org/rusticml/darkfi-intent-amm-proposal)
- [Response to PatrickM123](https://codeberg.org/rusticml/darkfi-intent-amm-proposal/src/branch/main/docs/response-to-patrickm123.md)
