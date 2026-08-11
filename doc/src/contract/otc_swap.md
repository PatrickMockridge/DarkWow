# OTC Swap Contract

Privacy-preserving peer-to-peer over-the-counter token swap. Two parties atomically
exchange tokens without a centralized exchange — Alice locks her coins first, then
Bob completes the swap by locking his and releasing both.

## The Problem: Trust in P2P Trading

Direct peer-to-peer token swaps require both parties to trust each other:
- **Alice**: "I'll send you 1000 WCKD if you send me 2000 MLDY"
- **Bob**: "I'll send you 2000 MLDY if you send me 1000 WCKD"
- **Problem**: Whoever sends first risks the other party walking away

Centralized exchanges solve this by custodying both parties' funds, but introduce
privacy violations, counterparty risk, and gatekeeper control.

**What if you could swap tokens atomically with full privacy?**

## Our Solution: Two-Phase Commit Swap

```
┌─────────────────────────────────────────────────────────────────────┐
│                    OTC Swap Contract Flow                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   ALICE                                     BOB                       │
│      │                                        │                      │
│      │  1. CreateSwap                         │                      │
│      │     (send_value, recv_value, timeout)   │                      │
│      │     → swap_id shared off-chain          │                      │
│      │────────────────────────────────────────→│                      │
│      │                                        │                      │
│      │  2. FundSwap                           │                      │
│      │     (locks Alice's coins in contract)   │                      │
│      │     → state: Created → Funded           │                      │
│      │                                        │                      │
│      │                    3. ExecuteSwap      │                      │
│      │                    (locks Bob's coins, │                      │
│      │                     releases both)     │                      │
│      │                    → state: Funded →   │                      │
│      │                      Executed           │                      │
│      │←──────────────────────────────────────│                      │
│                                                                       │
│   ALICE (fallback)                          BOB                        │
│      │                                        │                      │
│      │  4. CancelSwap (after timeout)          │                      │
│      │     (refunds Alice's coins)             │                      │
│      │     → state: Funded → Cancelled         │                      │
│      │                                        │                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Trust Model: Two-Phase Commit with Timeout

Alice commits first. Bob completes. Neither can steal from the other:

| Phase | Who Acts | What Happens | Trust Guarantee |
|-------|----------|--------------|-----------------|
| Create | Alice | Proposes swap parameters on-chain | Parameters are binding |
| Fund | Alice | Locks her coins via child transfer | Bob can verify funds are locked |
| Execute | Bob | Locks his coins + both released atomically | Atomic — both or neither |
| Cancel | Alice | Refunds after timeout | Alice can always recover |

**Why Alice locks first?** This prevents Bob from being front-run — if Bob locked
first, Alice could see his coins on-chain and walk away. With Alice locking first,
Bob knows the swap is fully funded before he commits.

## Privacy Properties

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| Swap exists (commitment hash) | Token amounts (Pedersen commitments) |
| Execution or cancellation (nullifier) | Which party executed |
| Timeout (block height) | Actual token types (hidden in commitments) |
| Alice and Bob's pubkeys (derived from secrets) | Real identities |

## State Machine

```
┌─────────────────────────────────────────────────────────────────────┐
│                     OTC Swap State Machine                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   Created ──[Fund]──> Funded ──[Execute]──> Executed                 │
│      │                   │                                           │
│      │                   └──[Cancel]──> Cancelled (timeout only)      │
│      │                                                                │
│      └──[Cancel]──> Cancelled                                        │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

| State | Description | Who Can Transition |
|-------|-------------|-------------------|
| **Created** | Alice proposed the swap | Alice → Fund or Cancel |
| **Funded** | Alice locked her coins | Bob → Execute, Alice → Cancel (after timeout) |
| **Executed** | Both parties' coins exchanged | Terminal state |
| **Cancelled** | Alice cancelled (before fund or after timeout) | Terminal state |

## Contract Functions

| Function | ID | Who | Description |
|----------|-----|-----|-------------|
| InitializeV1 | 0x00 | — | Initialize swap contract state |
| CreateSwapV1 | 0x01 | Alice | Propose swap parameters on-chain |
| FundSwapV1 | 0x02 | Alice | Lock Alice's coins via child transfer |
| ExecuteSwapV1 | 0x03 | Bob | Lock Bob's coin + atomic release of both |
| CancelSwapV1 | 0x04 | Alice | Cancel before execute, or timeout refund |

## Capability Resolution

The contract integrates with the wallet's [capability-based position resolution system](../arch/wallet.md#capability-based-position-resolution).

### Capability Type Discriminants

| Discriminant | Role | State |
|-------------|------|-------|
| 0x00 | Alice | Created |
| 0x01 | Bob | Created |
| 0x02 | Alice | Funded |
| 0x03 | Bob | Funded |

### Resolution Table

| State | Role | Capability | Available Actions |
|-------|------|------------|-------------------|
| Created | Alice | 0x00 Alice+Created | FundSwap (0x02), CancelSwap (0x04) |
| Created | Bob | 0x01 Bob+Created | *(wait for Alice to fund)* |
| Funded | Alice | 0x02 Alice+Funded | CancelSwap (0x04) — after timeout |
| Funded | Bob | 0x03 Bob+Funded | ExecuteSwap (0x03) |
| Executed | either | *(terminal)* | *(none)* |
| Cancelled | either | *(terminal)* | *(none)* |

Alice always retains her Created-state capability even after funding (FundSwap
doesn't consume it), so she can cancel from either state. The contract enforces
the timeout requirement in the Cancel ZK circuit, not in the capability graph.

## ZK Circuits

### create_swap.zk

Proves the swap commitment is correctly formed:
- **Public inputs**: `commitment = H(alice_pub, H(bob_pub), send_value, send_token, recv_value, recv_token, timeout)`, `bob_commitment = H(bob_pub)`
- **Private inputs**: `alice_pub_x, alice_pub_y, bob_pub_x, bob_pub_y, send_value, send_token_id, recv_value, recv_token_id, timeout, alice_secret`
- **Verification**: Alice's pubkey derivation + commitment hash + Bob commitment privacy

### fund_swap.zk

Proves Alice's locked value commitment is valid:
- **Public inputs**: `value_commit.x, value_commit.y, swap_id, merkle_root`
- **Private inputs**: `value, value_blind, merkle_leaf_pos, merkle_path`
- **Verification**: Pedersen commitment `C = value * G + value_blind * H` + Merkle proof

### execute_swap.zk

Proves Bob legitimately completes the swap:
- **Public inputs**: `swap_id, bob_commitment, spent_nullifier`
- **Private inputs**: `bob_secret, bob_pub_x, bob_pub_y, alice_recipient_x, alice_recipient_y, bob_recipient_x, bob_recipient_y`
- **Verification**:
  1. `bob_pub = bob_secret * G`
  2. `bob_commitment = H(bob_pub)` matches stored commitment
  3. `spent_nullifier = H(swap_id, bob_secret)`

### cancel_swap.zk

Proves Alice legitimately cancels:
- **Public inputs**: `swap_id, timeout, current_block, alice_pub_x, alice_pub_y, spent_nullifier`
- **Private inputs**: `alice_secret, recipient_x, recipient_y`
- **Verification**:
  1. `less_than_strict(timeout, current_block)` — timeout passed (for Funded state)
  2. `alice_pub = alice_secret * G` matches stored pubkey
  3. `spent_nullifier = H(swap_id, alice_secret)`

## Opcode Requirements

**No new opcodes needed.** All circuits use existing opcodes:

| Circuit | Opcodes Used | Status |
|---------|-------------|--------|
| `create_swap.zk` | `poseidon_hash`, `ec_mul_base`, `ec_get_x`, `ec_get_y`, `constrain_equal_base` | Existing |
| `fund_swap.zk` | `ec_mul_short`, `ec_mul`, `ec_add`, `ec_get_x`, `ec_get_y`, `merkle_root` | Existing |
| `execute_swap.zk` | `ec_mul_base`, `poseidon_hash`, `constrain_equal_base`, `ec_get_x`, `ec_get_y` | Existing |
| `cancel_swap.zk` | `less_than_strict`, `ec_mul_base`, `ec_get_x`, `ec_get_y`, `poseidon_hash`, `constrain_equal_base` | Existing |

## Use Cases

### Direct Token Swap
```rust
// Alice has WCKD, wants MLDY
// Bob has MLDY, wants WCKD

// Step 1: Alice creates swap
let swap = CreateSwapBuilder::new()
    .send_value(1000)        // 1000 WCKD
    .send_token(WCKD_ID)
    .recv_value(2000)        // 2000 MLDY
    .recv_token(MLDY_ID)
    .timeout(current_block + 1000)
    .build()?;
// Alice shares swap_id with Bob off-chain

// Step 2: Alice funds (locks 1000 WCKD)

// Step 3: Bob executes (locks 2000 MLDY, both released atomically)
// Bob receives 1000 WCKD, Alice receives 2000 MLDY

// OR Step 4: After timeout, Alice cancels → refunds her 1000 WCKD
```

### DarkIRC Private Trade
```rust
// Two users discover each other on DarkIRC
// They negotiate terms in a private message
// The swap contract handles the atomic exchange

// 1. Alice posts swap_id in private message to Bob
// 2. Bob verifies swap parameters on-chain
// 3. Alice funds → Bob executes → trade complete
```

## Architecture

The OTC swap contract source is in `src/contract/otc_swap/`:

```
src/contract/otc_swap/
├── proof/                      # ZK proof circuits (.zk files)
│   ├── create_swap.zk       # Swap proposal commitment
│   ├── fund_swap.zk         # Alice's value commitment
│   ├── execute_swap.zk      # Bob's atomic completion
│   └── cancel_swap.zk       # Alice's timeout refund
├── src/
│   ├── client/                 # Builder structs + proof generation
│   │   ├── create_swap_v1.rs
│   │   ├── fund_swap_v1.rs
│   │   ├── execute_swap_v1.rs
│   │   └── cancel_swap_v1.rs
│   ├── capability.rs           # Wallet capability descriptor
│   ├── entrypoint.rs           # WASM entrypoint (274 lines)
│   ├── error.rs                # Error types (26 variants)
│   ├── lib.rs                  # Contract definitions
│   └── model/mod.rs            # Data structures
└── tests/
    └── integration.rs          # 16 serialization/enum tests
```

## Integration with Money Contract

Like the [escrow contract](escrow.md), OTC Swap manages its own state machine and
uses child calls to `promissory_note::transfer_v1` (0x04) for actual token movement:

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Integration Architecture                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   PromissoryNote Contract                                            │
│   ├── Owns coin ledger (coins, nullifiers, Merkle tree)             │
│   ├── Issues tokens (MintV1/BurnV1)                                  │
│   └── Transfer semantics (0x04 = TransferV1)                          │
│                                                                       │
│   OTC Swap Contract                                                  │
│   ├── Owns swap state machine (Created → Funded → Executed/Cancelled)│
│   ├── Verifies ZK proofs for fund/execute/cancel                     │
│   ├── FundSwap → child call: promissory_note::transfer_v1 (Alice's coins)   │
│   └── ExecuteSwap → child call: promissory_note::transfer_v1 (Bob's coins)  │
│                                                                       │
│   Flow:                                                               │
│   1. Alice creates swap on-chain → state: Created                    │
│   2. Alice funds → child transfer locks her coins → state: Funded    │
│   3. Bob executes → child transfer locks his coins + releases both   │
│      → state: Executed                                               │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

## Security Considerations

### Double-Spend Prevention
The `spent_nullifier = H(swap_id, secret)` ensures:
- Execute and Cancel are mutually exclusive
- Only one of them can succeed
- The first one finalized wins

### Front-Run Prevention
Alice locks first, then Bob executes:
- Bob cannot be front-run — Alice's coins are already locked when he commits
- Alice cannot walk away after Bob locks — the child transfer is atomic
- Timeout protects Alice if Bob never responds

### Timeout Integrity
`less_than_strict(timeout, current_block)` in the Cancel circuit ensures:
- Alice cannot cancel before the timeout (in Funded state)
- Bob has guaranteed time window to execute
- Block proposers cannot manipulate `current_block`

### Value Privacy
Pedersen commitment `C = value * G + blind * H` ensures:
- On-chain values are hidden
- Commitment is binding
- Only the parties (who know the blind) can verify amounts

## Comparison

| Feature | CEX Trade | DEX Pool (Public) | DarkWow OTC Swap |
|---------|-----------|-------------------|-----------------|
| Privacy | None (KYC) | Partial (tx visible) | Full (commitments only) |
| Trust | Exchange custody | Smart contract | Trustless, private |
| Counterparty risk | Exchange hack | Pool manipulation | Zero (atomic) |
| Price discovery | Order book | AMM curve | Direct negotiation |
| Front-running | Possible | MEV | Prevented (two-phase) |

## Implementation Status

**Full MVP** — All 5 functions and 4 ZK circuits implemented, compiled, and tested.

| Circuit | Binary | Status |
|---------|--------|--------|
| `create_swap.zk` | `create_swap.zk.bin` | Compiled — commitment + bob privacy hash |
| `fund_swap.zk` | `fund_swap.zk.bin` | Compiled — Pedersen commitment + Merkle proof |
| `execute_swap.zk` | `execute_swap.zk.bin` | Compiled — Bob secret proof + nullifier + recipients |
| `cancel_swap.zk` | `cancel_swap.zk.bin` | Compiled — timeout check + Alice secret proof + nullifier |

**Tests**: 16/16 integration tests pass (serialization roundtrips, enum validation,
state machine constants, derive_id, compute_nullifier).

**Wallet resolver**: Descriptor defined in `capability.rs`. Wallet-side resolver
in `bin/dww/src/capability.rs` pending — follows the [standard integration pattern](../arch/wallet.md#adding-a-new-contract-resolver).

## References

- [Escrow Contract](escrow.md) — same architecture pattern (child transfer, nullifier, state machine)
- [Wallet Architecture](../arch/wallet.md) — capability-based position resolution
- [Promissory Note Contract](promissory_note.md) — OTC swap function (0x05) for raw input/output swaps
- [Anonymous Assets](../arch/anonymous_assets.md) — coin commitment model
- [Opcodes](../arch/zk/opcodes.md) — ZK circuit opcode reference

## See Also

- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis
