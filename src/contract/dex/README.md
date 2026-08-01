# DarkWow DEX Contract

Anonymous decentralized exchange for privacy-preserving token swaps with **configurable transparency levels**.

## Design Philosophy: Modular Transparency

Different DEX deployments serve different users with different privacy/compliance needs. The transparency level is set at **deployment time** via `InitializeParams.transparency_config`:

| Level | Description | Use Case |
|-------|-------------|----------|
| **Dark** (default) | Complete darkness - nothing revealed | Maximum sovereignty |
| **Aggregate** | Price ranges, volume bands only | Market makers, institutional |
| **Anonymized** | Unlinkable trade data | Regulated entities |
| **Full** | Everything revealed | Full compliance, audits |

## Contract Functions

| Function | ID | Description |
|---------|-----|-------------|
| `InitializeV1` | 0x00 | Initialize DEX with timeout, fee, transparency config |
| `CreateSwapV1` | 0x01 | Alice creates swap proposal, locks funds |
| `AcceptSwapV1` | 0x02 | Bob accepts, locks matching funds |
| `ExecuteSwapV1` | 0x03 | Atomic swap execution (ZK verified) |
| `CancelSwapV1` | 0x04 | Either party cancels (triggers refund) |
| `UpdateConfigV1` | 0x05 | Update timeout/fee (governance) |
| `SetTransparencyLevelV1` | 0x06 | Change transparency level (governance) |
| `ExecuteSwapFeeV1` | 0x07 | Atomic swap with fee deduction (ZK verified) |
| `ExecuteSwapSlippageV1` | 0x08 | Atomic swap with slippage tolerance (ZK verified) |

### InitializeV1 (0x00)

```rust
pub struct InitializeParams {
    pub timeout: u32,                      // Swap timeout in blocks
    pub fee: u64,                          // DEX fee (basis points)
    pub trusted_money_merkle_root: [u8; 32],
    pub transparency_config: TransparencyConfig,
}

pub struct TransparencyConfig {
    pub level: TransparencyLevel,           // Dark, Aggregate, Anonymized, Full
    pub price_band_size: u64,               // e.g., 100 = $100 bands
    pub volume_bucket_size: u64,           // e.g., 1000 = token buckets
    pub anonymity_group_size: u64,          // e.g., 10 = groups of 10
}
```

## ZK Circuits

The DEX uses **six ZK circuits**:

| Circuit | Description | Dark | Aggregate | Anonymized | Full |
|---------|-------------|------|-----------|------------|------|
| `create_swap_v1.zk` | Proves proposer locked valid funds | ✅ | ✅ | ✅ | ✅ |
| `accept_swap_v1.zk` | Proves acceptor locked matching funds | ✅ | ✅ | ✅ | ✅ |
| `execute_swap_v1.zk` | Proves both secrets known, partial fill | ✅ | ✅ | ✅ | ✅ |
| `cancel_swap_v1.zk` | Proves ownership for cancellation | ✅ | ✅ | ✅ | ✅ |
| `execute_swap_slippage_v1.zk` | Proves slippage tolerance | ❌ | ✅ | ✅ | ✅ |
| `execute_swap_fee_v1.zk` | Proves fee deduction | ❌ | ✅ | ✅ | ✅ |

### execute_swap_v1.zk (Partial Fill)

Proves both parties' secrets and locks are valid with **partial fill support**:

```zk
# Using verified LessThanOrEqual (returns Boolean)
is_lte = less_than_or_equal(fill_amount, alice_amount);
constrain_equal_base(is_lte, ONE);
```

### execute_swap_slippage_v1.zk

Proves swap execution respects slippage tolerance:

```zk
# Verify: received >= min_expected * (1 - slippage_bps / 10000)
tolerance_multiplier = base_div(BPS - slippage_bps, BPS);
min_acceptable = base_mul(bob_amount, tolerance_multiplier);
less_than_or_equal(min_acceptable, received);
```

### execute_swap_fee_v1.zk

Proves fee is correctly calculated and deducted:

```zk
# Calculate fee = fill_amount * fee_bps / 10000
fee_numerator = base_mul(fill_amount, fee_bps);
fee = base_div(fee_numerator, BPS);
net_received = fill_amount - fee;
```

## Atomic Swap Flow

### Standard Atomic Swap

```
1. Alice creates swap (locks funds, holds secret)
2. Bob accepts swap (locks funds)
3. Alice or Bob calls ExecuteSwap (both secrets needed)
```

### Open Execution (Instant Fill)

```
1. Alice creates swap with open_execution=true (secret public)
2. Bob accepts with immediate_execute=true
3. Swap executes automatically in same transaction!
```

**Warning**: Open execution reveals Alice's secret. Use only with trusted counterparties.

## Opcode Status

| Opcode | Status | Use |
|--------|--------|-----|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Partial fill comparisons |
| `BaseDiv` (0x58) | ✅ Implemented | Ratio calculations, slippage, fees |
| `less_than_strict` | ✅ Sound | Bounded comparisons |
| `IsEqualBase` (0x54) | ❌ Bug | Do not use |

## Implementation Status

### Complete

- [x] Contract structure and entrypoint
- [x] All 9 functions (0x00-0x08)
- [x] Atomic swap DAO implementation
- [x] 6 ZK circuits (all compiled)
- [x] Partial fill via LessThanOrEqual
- [x] Open execution (open_execution + immediate_execute)
- [x] Slippage tolerance circuits (execute_swap_slippage_v1.zk)
- [x] Fee calculation circuits (execute_swap_fee_v1.zk)
- [x] Modular transparency architecture
- [x] Test harness with full entrypoint coverage
- [x] Heavyweight pipeline endpoint testing (CreateSwapV1, AcceptSwapV1, ExecuteSwapV1)
- [x] promissory_note child call validation (ExecuteSwapV1, CancelSwapV1)
- [x] Cross-contract FuncRef constraints in circuit
- [x] Full promissory_note integration with OtcSwap proofs

### Future

- [ ] Integration with money contract (cross-contract ZK)
- [ ] SMT order book (Level 1)
- [ ] Integration tests

## Key Blockers

| Blocker | Severity | Status |
|---------|----------|--------|
| Manual matching required | High | Open execution provides instant fill |
| Cross-contract ZK | High | Needs opcode development |
| SMT order book | Future | Needs solver/oracle |

## References

- [DarkWow DEX Architecture Document](../../../doc/src/arch/dex.md)
- [Promissory Note](../promissory_note/)
- [DarkWow Bridge Contract](../bridge/)
