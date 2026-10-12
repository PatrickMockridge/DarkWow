# Anonymous DEX Architecture

> **Contract specification.** For developer integration details, see [DEX Dev Guide](../dev/contracts/dex.md).

*Privacy-preserving decentralized exchange using incremental transparency.*

## The Core Problem: SPV De-anonymization

Bitcoin's SPV was supposed to provide privacy by only requesting relevant transactions. But bloom filters leak which addresses you care about - revealing what you're looking for IS a privacy failure.

```
SPV Problem:
1. Alice queries for transactions involving her addresses
2. Bloom filter reveals which addresses she cares about
3. Node can link Alice's addresses together
4. Privacy = broken
```

## Our Solution: Incremental Transparency

Instead of building a transparent system and trying to add privacy, we start completely dark and carefully reveal transparency as needed.

### Privacy Gradient

| Level | Description | Privacy Leakage |
|-------|-------------|-----------------|
| **Dark** | Complete darkness | None |
| **Aggregate** | Price ranges, volume bands | Minimal |
| **Anonymized** | Unlinkable trades | Low |
| **Full** | Opt-in transparency | None (opt-in) |

## Level 0 MVP: Atomic Swap DAO

The MVP is a **DAO that coordinates bilateral atomic swaps** - not a full order book:

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize DEX with its fee, swap timeout and transparency config |
| `CreateSwapV1` | `0x01` | Proposer locks funds, creates swap proposal |
| `AcceptSwapV1` | `0x02` | Acceptor locks matching funds |
| `ExecuteSwapV1` | `0x03` | Execute atomic swap (partial fill support) |
| `CancelSwapV1` | `0x04` | Cancel swap, refund proposer |
| `UpdateConfigV1` | `0x05` | Update DEX configuration parameters |
| `SetTransparencyLevelV1` | `0x06` | Set transparency level per deployment |
| `ExecuteSwapFeeV1` | `0x07` | Execute swap with fee deduction (exact integer division) |
| `ExecuteSwapSlippageV1` | `0x08` | Execute swap with slippage tolerance (cross-multiplication) |

Neither circuit uses `base_div`. Both were annotated `(BaseDiv)` until 2026-10-12, and the
annotation had been false for two revisions — `execute_swap_slippage.zk` says so in its own header
("RC4: already uses cross-multiplication (no base_div)") and `execute_swap_fee.zk` records the line
that claimed otherwise being deleted. The fee path reinstates an **exact integer division**; the
slippage path compares `base_mul(fill_amount, BPS)` against `base_mul(slippage_sub, alice_amount)`.

*Corrected 2026-10-11 (`F-520`).* The `InitializeV1` row said "Initialize DEX with
config and governance key". Measured: `InitializeParams` has three fields — `timeout`,
`fee` and `transparency_config` — and **no governance key**, so none is initialized;
and `DEX_CONTRACT_GOVERNANCE_PUBKEY_KEY`, which the old wording leaned on, was
declared and never written or read anywhere in the tree, so it has been removed
rather than left as an explanation. Governance authority in this contract is proven
per call, not registered at init: `update_config` and
`set_transparency_level` each verify a ZK proof from the authority the caller holds,
which is the same shape the rest of the tree uses.

```
Alice wants: 100 DRKW ↔ 1 ETH (Bob's offer)
Bob wants: 1 ETH ↔ 100 DRKW (Alice's offer)

1. Proposal: Alice locks 100 DRKW, creates swap proposal
2. Acceptance: Bob locks 1 ETH, accepts swap
3. Execution: Atomic swap executes
4. Timeout: Auto-refund if no acceptance
```

### Why This Works as Level 0

- **No order book**: No one knows what swaps are possible
- **No price discovery**: Parties agree bilaterally
- **No information leakage**: Swap either happens or refunds
- **Trustless**: No trusted third party

## Modular Transparency Architecture

Different DEX deployments can choose different transparency levels at deployment:

```
┌─────────────────────────────────────────────────────────────────────┐
│  DEX Deployment: InitializeParams.transparency_config               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Dark (Default)                                                     │
│  └── swap_id only in events - maximum privacy                       │
│                                                                     │
│  Aggregate                                                          │
│  └── price bands, volume buckets - market data for LPs              │
│                                                                     │
│  Anonymized                                                         │
│  └── unlinkable trades - compliance-friendly                        │
│                                                                     │
│  Full                                                               │
│  └── everything revealed - full auditability                         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Per-Level Circuit Availability

| Circuit | Dark | Aggregate | Anonymized | Full |
|---------|------|-----------|------------|------|
| `execute_swap_slippage.zk` | ❌ | ✅ | ✅ | ✅ |
| `execute_swap_fee.zk` | ❌ | ✅ | ✅ | ✅ |

Higher transparency levels enable more sophisticated market-making circuits.

## Promissory Note Lifecycle Integration

The DEX is a **token mover** in the Promissory Note ecosystem. It facilitates
token exchange between participants but does not mint, burn, or redeem tokens.

### PN Opcodes Used

| Operation | PN Opcode Called | Purpose |
|-----------|-----------------|---------|
| ExecuteSwapV1 | TransferV1 (0x04) | Alice's token to Bob, one direction |
| ExecuteSwapFeeV1 | TransferV1 (0x04) | The same move, with a fee deducted |
| ExecuteSwapSlippageV1 | TransferV1 (0x04) | The same move, with a slippage bound |
| CancelSwapV1 | TransferV1 (0x04) | Refund proposer on cancel |

Every swap entrypoint requires **two** child calls, each a `TransferV1`: one
moving Alice's token to Bob and one moving Bob's token to Alice. The swap's
atomicity is the *transaction's* — both transfers are children of the same
transaction, so the chain applies them together or not at all — and each
entrypoint's circuit binds both children's `FuncRef`s, so neither can be
substituted. Cancel operations use `TransferV1` to refund locked tokens.

*Corrected 2026-10-11 (`F-512`).* This table said `otc_swap_v1 (0x05)` for the
three swap operations, and the paragraph here said the DEX "delegates swap
execution to the PN contract's OTC swap functionality rather than implementing
its own atomic swap logic". Neither is what the code does: all three entrypoints
reject a child whose selector is not `0x04`, and their error messages name
`transfer_v1`. The child count settles which side was wrong — `otc_swap_v1` is
itself a *bilateral* swap, so two of them would be two swaps, while two
`TransferV1`s compose the single swap the DEX is making. `CancelSwapV1` was
already documented as `TransferV1` and matches the code.

### Role in the PN Lifecycle

The DEX is purely a transfer facilitator. It does not participate in the
mint/redeem lifecycle. No spend_hook policy is needed — DEX output commitments
carry no protocol-level restrictions.

### Cross-Contract Validation

Child calls validate `contract_id` to prevent routing attacks. Value commit
validation is handled by the otc_swap contract for 0x05 calls and is not
performed for TransferV1 refunds.

## ZK Circuits

Eight ZK circuits power the DEX:

| Circuit | Purpose |
|---------|---------|
| `create_swap.zk` | Proves proposer locked valid funds |
| `accept_swap.zk` | Proves acceptor locked matching funds |
| `execute_swap.zk` | Proves both secrets known, partial fill |
| `cancel_swap.zk` | Proves ownership for cancellation |
| `update_config.zk` | Proves authority to update DEX configuration |
| `set_transparency_level.zk` | Proves authority to set transparency level |
| `execute_swap_slippage.zk` | Proves slippage tolerance (cross-multiplication) |
| `execute_swap_fee.zk` | Proves the fee as an exact integer quotient — and nothing binds it (OBL-Z13) |

### Data Structures

```
Lock Commitment = H(secret, token, amount)
Swap ID = H(proposer_lock, request_token, request_amount)
Nullifier = H(secret)
```

## Signature Verification

The DEX binds the signer **in the circuit**. There is no host half:

```
Circuit constrains: ec_mul_base(sig_secret, K) == signature_public
                    (both coordinates exposed as public inputs)
```

*Corrected 2026-10-11 (`F-515`).* This section described a "split verification (host
+ circuit)" whose host half was `schnorr_verify(public, signature)`, and closed by
saying it "exists because no `schnorr_verify` opcode exists yet" — which contradicts
itself, since the function it names is the thing it concedes is missing. Measured:
**`schnorr_verify` appears nowhere in the tree**, and the host's import table carries
no signature function of any kind. The circuit half exists and is the *whole*
binding: `create_swap.zk` derives the public key from a witnessed `signature_secret`
with `ec_mul_base`, constrains its two coordinates equal to the ones the call
publishes, and `constrain_instance`s both — so a prover can produce the proof only
for a key it holds. It is the same derive-and-expose shape `deployooor`'s lock uses
(`F-511`).

The signer is therefore bound without a signature scheme, which is why this
section's premise — that a host check is needed — is gone rather than deferred.

## Capabilities Enabled by Opcodes

### Ratio arithmetic — the quotient-remainder, not `base_div`

This section used to sketch both features with `base_div`, which computes `a * b^(p-2)`: a number
near `p`, not the small integer either feature is defined by. The circuits were moved to the
quotient-remainder form in the RC4 pass; this prose was not, until 2026-09-20.

**Slippage tolerance**:
```zk
slippage_ok = less_than_or_equal(slippage_bps, BPS);   # a tolerance above 100% is not a tolerance
right = base_mul(slippage_sub, alice_amount);          # slippage_sub = BPS - slippage_bps
left  = base_mul(fill_amount, BPS);
satisfied = less_than_or_equal(right, left);
constrain_equal_base(satisfied, ONE);
```

**Fee calculation** — an exact integer quotient:
```zk
fee_times_bps      = base_mul(fee, BPS);
fill_times_fee_bps = base_mul(fill_amount, fee_bps);
fee_times_bps <= fill_times_fee_bps < (fee + 1) * BPS;
constrain_instance(fee);
```

Note what the fee sketch does **not** say: where the fee goes. Nothing in dex reads `params.fee`,
the params carry no `fill_amount` for the host to recompute it from, and the children are validated
for contract id only — no dex entrypoint validates a child value commitment, while promissory note
exports `validate_child_value_commit` for exactly that. The fee is proven and unbound; OBL-Z13 in
`doc/src/arch/verification-hazop.md` records it.

### LessThanOrEqual (0x55)

Verified sound comparison enables:

**Partial fill**:
```zk
is_lte = less_than_or_equal(fill_amount, alice_amount);
constrain_equal_base(is_lte, ONE);
```

**Predicate-based matching**:
```zk
is_match = less_than_or_equal(my_price, market_price);
```

## Avoiding De-anonymization

### Attack Vectors

| Attack | Mitigation |
|--------|------------|
| Timing analysis | Minimum time delays between order and match |
| Dust attacks | Minimum order size thresholds |
| Volume fingerprinting | Batched trades + differential privacy |
| Key linkage | Different key per order |

### Key Insight

The key is **not revealing what you're looking for**:

| SPV Problem | DarkWow Solution |
|-------------|-----------------|
| Bloom filters reveal addresses | Order commitments hidden until match |
| Address clustering | Different key per order |
| Trade size = epoch volume | Privacy noise in aggregates |
| Timing analysis | Batched trades with random delays |

## Future: Order Book with Privacy

After MVP, expanding to order book requires:

```
Problem: Where do prices come from?
- Bilateral agreement: parties agree directly
- Order book: need price discovery
- But revealing order prices = deanonymization

Solutions:
- Encrypted order book ( commitments, ZK matching )
- Differential privacy ( noise in aggregates )
- Time-locked disclosure ( reveal after settlement )
```

### The SPV-Style Leak Problem

When solver queries for matching orders, the SMT query reveals what they're looking for:

```
Naive: Query SMT for "orders where price >= X"
      → Reveals you're looking for price X

DarkWow: Solver computes in encrypted domain
        → Only submits ZK proof of valid match
        → No one learns what orders were queried
```

## Multi-Chain Token Support

The DEX supports bridged tokens from the universal bridge:

| Chain | Token | Notes |
|-------|-------|-------|
| Ethereum | ETH | Native gas token |
| Monero | XMR | Privacy-native |
| Zcash | ZEC | Shielded |
| Aztec | ETH/DAI | Private rollup |
| Litecoin | LTC | Trade pair |

## Comparison

| Feature | UniSwap | Curve | DarkWow DEX |
|---------|---------|-------|------------|
| Order visibility | Full | Full | Level-dependent |
| Amount privacy | None | None | ZK commitments |
| Identity privacy | Pseudonym | Pseudonym | Hidden |
| Price discovery | AMM formula | AMM formula | Bilateral |
| Front-running | Vulnerable | Vulnerable | Resistant |
| Liquidity model | AMM | AMM | Bilateral locks |

## References

- [DarkWow DEX Contract](../../../src/contract/dex/)
- [PromissoryNote Contract](promissory_note.md)
- [DarkWow Bridge Contract](./bridge.md)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)
- [Differential Privacy](https://en.wikipedia.org/wiki/differential_privacy)

## See Also

- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis
