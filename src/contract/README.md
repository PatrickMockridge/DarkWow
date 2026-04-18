This directory contains native WASM contracts on DarkFi.

## Unofficial/Experimental Contracts on Dev Branch

The `dev` branch contains additional contracts not yet in official DarkFi master. These are **EXPERIMENTAL** and **NOT AUDITED**.

| Contract | Description | Status |
|----------|-------------|--------|
| **attestation** | Generalized attestation and claims system | ✅ Complete ZK + Test Harness |
| **atomic_swap** | Cross-chain atomic swaps via HTLC | ✅ Complete ZK + Test Harness |
| **baccarat** | Privacy-preserving Baccarat (Punto Banco) casino game | ✅ Complete ZK + Test Harness |
| **block_height_prediction** | PoW-backed block height betting | ✅ Complete ZK + Test Harness |
| **bridge** | Cross-chain asset transfers with Ocaps | ✅ Complete ZK + Test Harness |
| **dao_escrow** | DAO with three modes (Escrow/Treasury/Endowment) | ✅ Complete ZK + Test Harness |
| **darkbet_exchange** | Unified betting exchange (order-book + AMM) | ✅ Complete ZK + Test Harness |
| **darktoshi_dice** | Privacy-preserving Satoshi Dice clone | ✅ Complete ZK + Test Harness |
| **dex** | Atomic swap DAO with partial fills + open execution | ✅ Complete ZK + Test Harness |
| **drain_protection** | Endowment/treasury drain protections with 8 best practices | ✅ Complete ZK + Test Harness |
| **escrow** | Hashed Timelock Contract variant | ✅ Complete ZK + Test Harness |
| **game_room** | Generalized betting and pot management | ✅ Complete ZK + Test Harness |
| **identity** | ZK credential proofs using competency DAGs | ✅ Complete ZK + Test Harness |
| **insurance_market** | Decentralized insurance marketplace | ✅ Complete ZK + Test Harness |
| **labor_market** | Job/labor market with escrow and DAO governance | ✅ Complete ZK + Test Harness |
| **auction** | Privacy-preserving auction using escrow for bids | ✅ Complete ZK + Test Harness |
| **oracle** | Push-model oracle with attestation integration | ✅ Complete ZK + Test Harness |
| **roulette** | Privacy-preserving roulette casino game | ✅ Complete ZK + Test Harness |
| **lottery** | Configurable lottery (bridge between BettingStake and Insurance) | ✅ Complete ZK + Test Harness |
| **tender** | Sealed-bid tendering with competency verification | ✅ Complete ZK + Test Harness |
| **lottery** | Configurable lottery (bridge between BettingStake and Insurance) | ✅ Complete ZK + Test Harness |
| **money_v3** | Privacy-first DeFi token (Poseidon-only, no EC operations) | ✅ Complete ZK + Test Harness |
| **stablecoin** | Monero-collateralized stablecoin | ✅ Complete ZK + Test Harness |
| **subscription** | Member subscription with DAO treasury | ✅ Complete ZK + Test Harness |

**Randomness**: See [Provable Randomness](../../doc/src/arch/provable_randomness.md) for analysis of randomness sources in DarkToshi Dice, Roulette, Lottery, and leveraging PoW for trustless randomness.

---

## Money V2 (Standard for This Fork)

**money_v2** is our standard money contract featuring improved circuit design with self-contained ZK proofs.

| Contract | Description | Standard |
|----------|-------------|----------|
| **money** | Original DarkFi money (v1) - upstream legacy | No |
| **money_v2** | Our secure version with constrain_equal_base | **Yes** |

See [money_v2/README.md](money_v2/README.md) for full details.

---

## ZKas Circuit Analysis

### Opcode Classification

**Proven Opcodes (Production Safe)**

| Opcode | Purpose |
|--------|---------|
| `poseidon_hash` | Commitment hashing, nullifiers, public keys |
| `ec_mul_base` | Public key derivation from secrets |
| `ec_get_x`, `ec_get_y` | Extract EC point coordinates |
| `ec_add`, `ec_mul`, `ec_mul_short` | Pedersen commitments |
| `merkle_root`, `sparse_merkle_root` | Merkle membership proofs |
| `less_than_strict` | Constrain-only comparisons (no return value) |
| `base_add`, `base_mul`, `base_sub` | Field arithmetic |
| `range_check`, `bool_check` | Value validation |
| `constrain_equal_base` | Equality constraints |

**Experimental Opcodes (Grey-Market - Not Production Ready)**

| Opcode | Issue |
|--------|-------|
| `LessThanOrEqual` (0x55) | Gate soundness vulnerability |
| `IsEqualBase` (0x54) | Delta-invert issue when `a == b` |
| `NotBase` (0x56) | Unused, experimental |
| `BaseLtStrict` (0x57) | Unused, experimental |

---

### Soundness Issues

**1. LessThanOrEqual Gate Soundness (CRITICAL)**

The constraint `out * (b - a) + (1 - out) * (a - b - 1)` can be satisfied with malicious assignments when `out = 0` and `a > b`. The range check limits but doesn't fully close the attack vector.

**2. IsEqualBase Delta-Invert (CRITICAL)**

When `a == b`, the constraint `delta * delta_invert == 1` becomes `0 * anything == 1`, which is unsatisfiable. A selector gate disables this, but the prover can assign arbitrary `delta_invert` when inputs are equal.

### Ideal vs Workaround: LessThanOrEqual vs Safemath

| Aspect | `LessThanOrEqual` (IDEAL) | Safemath (WORKAROUND) |
|--------|---------------------------|----------------------|
| Returns Boolean | ✅ Yes | ❌ No (constrain-only) |
| Composability | ✅ Full | ❌ Limited to assertions |
| Circuit bloat | ✅ None (single VM impl) | ❌ Each circuit copies gadget |
| Soundness | ⚠️ Unverified (gate soundness) | ✅ Production-ready |
| Use when | Need Boolean for downstream logic | Only need to assert `a <= b` |

**Why this matters**: Safemath (`assert_lte_u64_v1.zk`) is a **workaround** that works for stablecoin/identity because they only need to assert constraints (not return Booleans). If you need the comparison result as a value for further constraints, LessThanOrEqual is required.

---

### Cross-Multiplication Pattern (Alternative to BaseDiv)

Ratio checks can use cross-multiplication instead of division:

```zk
# Prove: approval_ratio <= yes_vote / all_vote
# Without division - use cross-multiplication:
lhs = base_mul(all_vote_value, approval_ratio_quot);
rhs = base_mul(yes_vote_value, approval_ratio_base);
rhs_1 = base_add(rhs, 1);  # +1 converts strict < to <=
less_than_strict(lhs, rhs_1);
```

**Note**: `BaseDiv` is now implemented (opcode 0x58). This cross-multiplication pattern is still useful for simple ratio assertions where BaseDiv would be expensive.

---

### Merkle Verification Patterns

| Circuit | Pattern | Status |
|---------|---------|--------|
| `burn_v1.zk` | `merkle_root(leaf_pos, path, coin_incl)` with `zero_cond` | ✅ Real |
| `propose-main.zk` | `merkle_root(dao_leaf_pos, dao_path, dao_bulla)` | ✅ Real |
| `propose-input.zk` | `sparse_merkle_root` for nullifiers | ✅ Real |
| `deposit_v1.zk` | `poseidon_hash(deposit_leaf, merkle_path_0, merkle_path_1)` | ❌ **Fake** |

---

### Circuit Safety Summary

| Contract | Circuit | Experimental? | Issues |
|----------|---------|---------------|--------|
| dao | `exec.zk` | No | ✅ Safe |
| dao | `propose-main.zk` | No | ✅ Safe |
| money (v1) | `burn_v1.zk` | No | ✅ Safe |
| money_v2 | `burn_v1.zk` | No | ✅ Safe - constrain_equal_base |
| escrow | `refund_v1.zk` | No | ✅ Safe |
| dao_escrow | `init_v1.zk` | No | ✅ Safe |
| dao_escrow | `pay_premium_v1.zk` | No | ✅ Safe |
| identity | `create_claim_v1.zk` | No | ✅ Uses safemath (Level 0 zk_only) |
| stablecoin | `open_position_v1.zk` | No | ✅ Uses safemath `assert_lte_u64_v1.zk` |
| stablecoin | `liquidate_v1.zk` | No | ✅ Uses safemath `assert_lte_u64_v1.zk` |
| bridge | `deposit_v1.zk` | No | ✅ Fixed — real `merkle_root` |

---

### Key Takeaways

1. **`BaseDiv` is implemented** - Opcode 0x58 using binary exponentiation
2. **`LessThanOrEqual` is verified sound** - Formally verified via Lean 4
3. **`less_than_strict` is safe** - It's constrain-only (no return value manipulation)
4. **Cross-multiplication is still useful** - For simple ratio assertions without BaseDiv overhead
5. **stablecoin and identity use safemath** - Assertion gadgets (`assert_lte_u64_v1.zk`) as workaround for LessThanOrEqual
6. **Bridge Merkle is fixed** - `deposit_v1.zk` now uses real `merkle_root` opcode

**See also**:
- [Opcodes Reference](../../doc/src/arch/opcodes.md) — Concise reference for contract authors
- [zkVM Primitive Layer](../../doc/src/arch/zkvm_primitives.md) — Deep dive into opcode implementation

## Token Design Philosophy

DarkFi's token contracts (MoneyV3, NativeToken) follow a **minimal infrastructure** philosophy:

> **Tokens are pipework, not reactors.**
>
> - Tokens move value. That's their job.
> - Business logic lives in smart contracts (DEX, stablecoin, etc.)
> - Tokens are the most frequently called contracts - simplicity minimizes attack surface
> - A bug in a token cascades to every operation; a bug in a smart contract is isolated

**Practical implications:**
- NativeToken: Minimal viable circuits for consensus (fees, rewards). One job, do it well.
- MoneyV3: Poseidon-only design for DeFi tokens. Zero EC operations in ZK = zero heap bugs.

### MoneyV3 Design Principles

MoneyV3 is the cornerstone of DarkFi's DeFi ecosystem:

1. **Poseidon-only ZK circuits**: All cryptographic operations in ZK use Poseidon hash. No EC operations in ZK circuits.

2. **EC operations pushed to smart contracts**: Pedersen commitments and other EC operations happen in the contract verification layer, not in ZK circuits. This keeps ZK circuits simple and auditable.

3. **Privacy-first token model**:
   - Coin = `poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)`
   - Nullifier = `poseidon_hash(secret, coin)` or `poseidon_hash(secret, token_id)` for auth
   - Value commitment = `poseidon_hash(value, blind)`

4. **Token authorization via Merkle proofs**: AuthTokenMintV1 uses Merkle tree verification in ZK to prove a token_id is authorized for minting.

5. **Function IDs**:
   - `0x00` - TokenMintV1: Create a new token type
   - `0x01` - AuthTokenMintV1: Authorize minting for existing token
   - `0x02` - MintV1: Mint tokens of existing token type
   - `0x03` - BurnV1: Burn tokens
   - `0x04` - TransferV1: Private token transfer
   - `0x05` - OtcSwapV1: Atomic OTC token swap

This separation ensures ZK circuits remain minimal while complex business logic lives in audited smart contracts.

## Testing Infrastructure

Two pipelines for contract testing at different levels:

### Lightweight Pipeline (`bin/darkfid/src/tests/pipeline.rs`)

For deployment verification without ZK proof generation. Fast CI/CD checks.

```bash
# Test any contract by name
cargo test --package darkfid test_pipeline
CONTRACT_NAME=money_v3 cargo test --package darkfid test_pipeline
CONTRACT_NAME=stablecoin cargo test --package darkfid test_pipeline
```

### Heavyweight Pipeline (`bin/darkfid/src/tests/heavyweight_pipeline.rs`)

For full contract execution testing with real ZK proofs. Uses `ContractHarness` trait for generic ZK circuit access.

```bash
# Requires release mode or increased stack size due to halo2's computational intensity
cargo test --package darkfid --release test_dex_heavyweight
cargo test --package darkfid --release test_money_v3_heavyweight

# Alternative: increase stack size
export RUST_MIN_STACK=16777216
cargo test --package darkfid test_dex_heavyweight
```

**Why the stack limit?** halo2's polynomial arithmetic uses deep recursion. Building multiple proving keys (4 circuits for DEX, 4 for MoneyV3) exceeds the default ~8MB stack.

See [Pipeline Documentation](../../doc/src/arch/pipeline.md) for full details.

### ContractHarness Trait

All test harnesses implement `ContractHarness` for generic ZK circuit access:

```rust
pub trait ContractHarness {
    fn name(&self) -> &str;
    fn circuits(&self) -> Vec<&'static str>;
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary>;
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey>;
}
```

Implemented by: `DexHarness`, `MoneyV3Harness`, `DarkbetExchangeHarness`, `DaoEscrowHarness`, `StablecoinHarness`, `NativeTokenHarness`

## Heavyweight Test Status

Heavyweight tests call actual contract endpoints (not just deployment). Current status:

| Test | Status | Endpoints Called |
|------|--------|------------------|
| `test_dex_heavyweight` | ✅ Pass | CreateSwapV1, AcceptSwapV1, ExecuteSwapV1 |
| `test_stablecoin_heavyweight` | ✅ Pass | OpenPosition, MintStable (w/ money_v3 child call), GovernanceReport, AccrueInterest |
| `test_money_v3_heavyweight` | ✅ Pass | TokenMintV1, AuthTokenMintV1, MintV1 |
| `test_dao_escrow_heavyweight` | ✅ Pass | Initialize, PayPremium |
| `test_darkbet_exchange_heavyweight` | ✅ Pass | CreateMarketV1, AddLiquidityV1, BuyPositionV1 |
| `test_identity_heavyweight` | ✅ Pass | InitializeV1, IssueCredentialV1, CreateClaimV1 |
| `test_pool_stake_heavyweight` | ✅ Pass | CreatePoolV1, JoinPoolV1 |
| Other contracts | ⚠️ Varies | Most deploy-only, may need endpoint updates |

Run tests:
```bash
cargo test --release --package darkfid test_dex_heavyweight
cargo test --release --package darkfid test_money_v3_heavyweight
cargo test --release --package darkfid test_darkbet_exchange_heavyweight
```

## Official Contracts

- **Money**: Private token transfers and basic operations
  - [Documentation](https://dark.fi/book/dev/darkfi_money_contract/)
- **DAO**: Decentralized autonomous organization with governance
  - [Documentation](https://dark.fi/book/dev/darkfi_dao_contract/)
- **Deployooor**: Contract deployment and management
  - [Documentation](https://dark.fi/book/dev/darkfi_deployooor_contract/)

## Unofficial Contract Documentation

Local READMEs exist for each contract in this folder:

- [attestation/README.md](attestation/README.md) - Generalized attestation and claims
- [atomic_swap/README.md](atomic_swap/README.md) - Cross-chain atomic swaps
- [baccarat/README.md](baccarat/README.md) - Privacy-preserving Baccarat casino game
- [block_height_prediction/README.md](block_height_prediction/README.md) - PoW-backed block height betting
- [bridge/README.md](bridge/README.md) - Cross-chain asset transfers
- [dao_escrow/README.md](dao_escrow/README.md) - Three-mode DAO
- [darkbet_exchange/README.md](darkbet_exchange/README.md) - Unified betting exchange (order-book + AMM)
- [darktoshi_dice/README.md](darktoshi_dice/README.md) - Satoshi Dice clone
- [dex/README.md](dex/README.md) - Atomic swap DAO
- [drain_protection/README.md](drain_protection/README.md) - Endowment protection
- [escrow/README.md](escrow/README.md) - Hashed Timelock Contract
- [game_room/README.md](game_room/README.md) - Generalized betting and pot management
- [identity/README.md](identity/README.md) - ZK credential proofs
- [insurance_market/README.md](insurance_market/README.md) - Decentralized insurance
- [labor_market/README.md](labor_market/README.md) - Job/labor market
- [lottery/README.md](lottery/README.md) - Configurable lottery
- [money_v3/README.md](money_v3/README.md) - Privacy-first DeFi token (STANDARD)
- [money_v2/README.md](money_v2/README.md) - Secure money contract
- [auction/README.md](auction/README.md) - Privacy-preserving auction
- [oracle/README.md](oracle/README.md) - Push-model oracle
- [roulette/README.md](roulette/README.md) - Privacy-preserving roulette
- [stablecoin/README.md](stablecoin/README.md) - Collateral stablecoin
- [subscription/README.md](subscription/README.md) - Member subscription
- [tender/README.md](tender/README.md) - Sealed-bid tendering

Architecture docs:

- [dao_escrow.md](../../doc/src/arch/dao_escrow.md)
- [game_room.md](../../doc/src/arch/game_room.md)
- [game_room_app_layer.md](../../doc/src/arch/game_room_app_layer.md)
- [subscription.md](../../doc/src/arch/subscription.md)
- [atomic_swap.md](../../doc/src/arch/atomic_swap.md)
- [oracle.md](../../doc/src/arch/oracle.md)
- [security-analysis.md](../../doc/src/arch/security-analysis.md)
- [composability.md](../../doc/src/arch/composability.md)
