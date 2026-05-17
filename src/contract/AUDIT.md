# DarkWow Smart Contract Security Audit

> **USE AT YOUR OWN RISK.** The smart contracts in this repository have undergone internal review and simulation-based security analysis but have NOT been audited by an independent third-party firm. Smart contracts carry inherent risks including but not limited to bugs, economic exploits, and cross-chain bridge vulnerabilities. No warranty is provided. You are solely responsible for any funds you deposit.

## Scope

| Contract | Version | Audit Date | Auditor |
|----------|---------|------------|---------|
| Bridge (`src/contract/bridge/`) | v1 (May 2026 hardening) | 2026-05-16 | Internal — simulation-based |
| Relayer Endowment (`src/contract/relayer_endowment/`) | v1 (May 2026 hardening) | 2026-05-16 | Internal — simulation-based |
| Identity (`src/contract/identity/`) | v1 (May 2026 hardening) | 2026-05-17 | Internal — Phase 2d integration |
| Attestation (`src/contract/attestation/`) | v1 (May 2026 hardening) | 2026-05-17 | Internal — Phase 2d integration |
| Pool Stake (`src/contract/pool_stake/`) | v1 (May 2026 hardening) | 2026-05-17 | Internal — Phase 2d integration |

**Out of scope**: universal_relayer binary, all other 23 contracts in this repository, ZK circuit soundness proofs, external chain integration code.

## Methodology

The audit combined three approaches:

1. **Discrete-Event Simulation**: 10 adversarial scenarios modeled block-by-block across the bridge + relayer_endowment + universal_relayer system — crash recovery, capital exhaustion, fee evasion, malicious relayers, slash loops, bank runs, network partitions, fee manipulation, pool tragedy of commons, and HTLC race conditions.

2. **State Machine Analysis**: Manual review of all state transitions in the bridge contract (deposit, withdraw, HTLC create/claim/refund, cancel, execute, reassign) and relayer endowment contract (initialize, deploy, withdraw deployment, claim fees, settle fees, update config, force settle).

3. **ZK Circuit Constraint Review**: Reviewed `withdraw_v1.zk`, `deposit_v1.zk` for missing or incorrect constraints — Merkle proof verification, range checks, equality constraints, dust thresholds.

## Findings

17 failure modes were identified: 1 CRITICAL, 6 HIGH, 8 MEDIUM, 2 LOW.

| # | Severity | Category | Description | Contract | Status |
|---|----------|----------|-------------|----------|--------|
| 1 | **CRITICAL** | Capital Exhaustion | 1960 guaranteed withdrawals rejected due to insufficient stake coverage — users cannot access their funds | Bridge | FIXED |
| 2 | **HIGH** | Liveness | No multi-relayer redundancy — other relayers did not process withdrawals during relayer crash | Bridge | FIXED |
| 3 | **HIGH** | Liveness | 20 withdrawals stuck in pending state at simulation end — no dynamic capacity scaling | Bridge | FIXED |
| 4 | **HIGH** | Capital | Total relayer stake < 10% of user deposit capacity — sudden withdrawal surge exhausts coverage | Bridge | FIXED |
| 5 | **HIGH** | Liveness | No withdrawal reassignment during network partition — stuck until timeout | Bridge | FIXED |
| 6 | **HIGH** | Economic | No fee cap — monopoly relayer can charge extortionate rates | Bridge | FIXED |
| 7 | **HIGH** | Economic | Pool slashes degrade all members equally — no per-member accountability | Pool Stake | FIXED |
| 8 | **MEDIUM** | Capital | Pending withdrawals exceed total relayer stake — coverage ratio violated at system level | Bridge | FIXED |
| 9 | **MEDIUM** | Economic | Backer ROI zero or negative — no fee settlement detection | Relayer Endowment | FIXED |
| 10 | **MEDIUM** | Economic | No mechanism to detect fee evasion early | Relayer Endowment | FIXED |
| 11 | **MEDIUM** | Economic | No auto-withdraw from dishonest relayers | Relayer Endowment | FIXED |
| 12 | **MEDIUM** | Economic | Only 0/11 guaranteed withdrawals received slash refunds — flat slash too small | Bridge | FIXED |
| 13 | **MEDIUM** | Economic | Total slashed < 1% of guaranteed withdrawal volume — slash doesn't scale | Bridge | FIXED |
| 14 | **MEDIUM** | UX | No fee discovery — users cannot query relayer fees before committing | Attestation | FIXED |
| 15 | **MEDIUM** | Economic | Adverse selection — good relayers leave pools due to shared slashing | Pool Stake | FIXED |
| 16 | **LOW** | ZK Soundness | ZK circuit assumed Merkle verification but didn't enforce it in constraints | Bridge ZK | FIXED |
| 17 | **LOW** | Atomicity | HTLC race condition — claim + refund can both succeed in same block | Bridge | FIXED |

## Hardening Applied (May 2026)

### Phase 1: Critical State Machine Fixes

**HTLC State Machine Atomicity** (finding #17):
- Added `claimed_at: Option<u64>` and `refunded_at: Option<u64>` to `HtlcSwapInfo`
- `claim_htlc` now only valid from `Pending` state
- `refund_htlc` now checks `claimed_at.is_none()` atomically
- Both timestamps set in `process_update` for mutual exclusion

**Circuit Breaker for Guaranteed Withdrawals** (findings #1, #3, #4, #8):
- Added `GUARANTEED_PENDING` counter that increments/decrements with guaranteed withdrawal lifecycle
- Added `MAX_GUARANTEED_TOTAL` configurable cap
- Bridge rejects new guaranteed withdrawals when `guaranteed_pending + amount > max_guaranteed_total`
- Added `MIN_GUARANTEED_COVERAGE_RATIO = 15000` (150% in basis points)

**Withdrawal Reassignment** (findings #2, #5):
- New `ReassignWithdrawalV1` function (opcode `0x09`)
- Added `reassignable_after: Option<u64>` and `heartbeat_at: Option<u64>` to `PendingWithdrawal`
- Any relayer can claim a stuck withdrawal after `reassignable_after` block
- Original relayer is partially slashed (50% of slash amount) for abandonment

### Phase 2: Economic Hardening

**Proportional Slashing** (findings #12, #13):
- `MIN_SLASH = 1_000_000` (floor) + `SLASH_BP = 1000` (10%)
- Slash computed as `max(MIN_SLASH, amount * SLASH_BP / BP_PRECISION)`
- Previously flat 1,000,000 regardless of withdrawal size

**Fee Caps** (finding #6):
- `MAX_FEE_BP = 1000` (10% maximum) enforced by bridge contract
- Users can specify tighter `max_fee_bp: Option<u64>` in `WithdrawParams`
- Withdrawal validation rejects if `fee > amount * max(effective_max_fee_bp) / BP_PRECISION`

**Backer-Initiated Force Settlement** (findings #9, #10, #11):
- New `ForceSettleV1` function (opcode `0x06`) in relayer_endowment contract
- Backers can force pro-rata fee distribution after `FORCE_SETTLEMENT_TIMEOUT = 1000` blocks of inactivity
- Added `last_settlement_height: u64` and `total_collected_fees_log: u64` to `RelayerEndowmentAccount`
- Each `SettleFeesV1` call resets `last_settlement_height`

### Phase 4: ZK Circuit Hardening

**Merkle Proof Verification** (finding #16):
- `withdraw_v1.zk` rewritten to use `SparseMerklePath` type instead of individual merkle proof elements
- Added `computed_root = sparse_merkle_root(leaf_index, merkle_path, deposit_leaf)` + `constrain_equal_base(computed_root, merkle_root)`
- Replaced hardcoded `witness_base(100_000_000)` dust threshold with `token_minimum: Base` public input

### Phase 2d/3: Identity & Attestation Integration (May 2026)

**Identity Contract — Relayer Registration** (findings #7, #15):
- New `RegisterIssuerV1` (0x0e): Allows trusted entities to register as credential issuers, establishing a bootstrapped trust root for the identity system
- New `UpdateReputationV1` (0x0f): Issuers update relayer reputation scores on-chain — slash_count, success_count, total_volume, settlement_frequency — stored in a new `reputations` tree keyed by `poseidon_hash(issuer_pub, relayer_pub)`

**Bridge Contract — Relayer Identity & Discovery** (findings #7, #14):
- New `RegisterRelayerV1` (0x0a): Relayers register their pubkey with the bridge, stored in a new `relayers` tree with `RelayerInfo` (pubkey, registered_at, total_slashed, total_withdrawals, total_successful, is_active, fee_schedule_id)
- New `AcceptWithdrawalV1` (0x0b): Relayer explicitly accepts a withdrawal — sets `PendingWithdrawal.relayer` from `[0u8;32]` to actual pubkey, records `accepted_at` block, binds `max_fee_bp` commitment
- Modified `ReassignWithdrawalV1`: Now restricts reassignment to registered relayers only
- New `VerifyRelayerReputationV1` (0x0c): Read-only query returning `ReputationInfo` (slash_count, success_count, total_volume, settlement_frequency, is_registered) from bridge-local relayer data
- New `RegisterFeeScheduleV1` (0x0d): Bridge-side endpoint to register fee schedule attestations

**Attestation Contract — Slash & Fee Schedule Attestation** (findings #7, #14):
- New `AttestSlashV1` (0x0b): Creates an on-chain attestation for relayer slash events with `claim_type = Predicate::Custom`, recording `poseidon_hash(relayer_x, relayer_y, slash_amount, withdrawal_id)` — enables privacy-preserving reputation queries
- New `CommitFeeScheduleV1` (0x0c): Relayers commit fee schedules on-chain as attestations with fee parameters (base_fee_bp, guaranteed_premium_bp, max_amount, min_amount) — addresses fee discovery
- **New ZK circuits**: `attest_slash_v1.zk` (k=11) constrains relayer pubkey coordinates as public inputs; `commit_fee_schedule_v1.zk` (k=11) constrains attestor pubkey coordinates as public inputs — both compiled and embedded in contract init

**Pool Stake Contract — Per-Member Slash Tracking & Rebalancing** (findings #7, #15):
- Added `slash_count: u64` to `PoolMemberStake` and `total_slashed: u64` + `pool_slash_count: u64` to `PoolStakeRegistry` for per-member accountability
- Modified `SlashCoverageV1`: After slashing coverage, iterates `contributing_members` and increments each member's `slash_count`; updates pool-level `total_slashed` and `pool_slash_count`
- New `RebalancePoolSharesV1` (0x08): Adjusts member pool shares based on slash history — `new_weight = base_share / (1 + slash_count)` — good relayers gain weight, bad relayers lose it

**Relayer Endowment — Reputation-Gated Deployment** (findings #7, #15):
- Added `total_slashed: u64` and `total_successful: u64` to `RelayerEndowmentAccount` for per-relayer reputation tracking
- Modified `DeployCapitalParamsV1` with `min_success_rate_bp: Option<u64>` and `max_slash_count: Option<u64>` — backers can set minimum reputation thresholds
- Modified `DeployCapitalV1`: Rejects capital deployment if relayer's slash count exceeds threshold or success rate falls below minimum — prevents adverse selection
- New `ReputationCheckFailed` error (Custom(14))

**Failure Mode Coverage Summary**:
| Finding | Addressed By | Mechanism |
|---------|-------------|-----------|
| #7 (pool slashes degrade all) | Pool Stake `RebalancePoolSharesV1` + per-member slash tracking | Individual share weighting based on slash history |
| #14 (no fee discovery) | Attestation `CommitFeeScheduleV1` + Bridge `RegisterFeeScheduleV1` | On-chain fee schedule commitments verified by attestation |
| #15 (adverse selection) | Relayer Endowment reputation thresholds + Identity reputation credentials | Backers filter relayers by attested slash/success history |

## Residual Risks

The following risks remain after hardening. **Users and operators should understand these before depositing funds.**

### Fund Loss Risk

| Risk | Severity | Details |
|------|----------|---------|
| ZK circuit soundness unproven | **HIGH** | ZK circuits have not undergone formal verification. A soundness bug could allow forging withdrawal proofs. |
| No external block header verification | **HIGH** | `external_block_hash` is accepted as public input but NOT verified against a real chain. A valid Merkle proof could reference a non-existent block. |
| No deposit finality guarantee | **MEDIUM** | The circuit proves a deposit EXISTS, not that it's FINAL. Chain reorganizations could revert deposits after proof submission. |
| Implementation bugs | **MEDIUM** | The contract code has not been formally verified. Bugs in `process_instruction` or `process_update` could cause fund loss. |
| Relayer theft (HTLC path) | **LOW** | In HTLC withdrawals, secret is revealed to the relayer. A malicious relayer could front-run on the external chain. Mitigated by fresh addresses per deposit. |
| Pool reputation tracking | **LOW** | Per-member slash tracking, `RebalancePoolSharesV1`, and reputation-gated capital deployment (Phase 2d) mitigate shared pool degradation. Individual member accountability is enforced but the system has not yet been battle-tested at scale. |
| Per-member slash enforcement | **LOW** | `PoolMemberStake.slash_count` is incremented on each slash event in `SlashCoverageV1`. The `RebalancePoolSharesV1` function adjusts share weights accordingly. Full pool rebalancing requires off-chain member ID tracking until DB iteration is available in WASM. |

### Privacy Risk

| Risk | Severity | Details |
|------|----------|---------|
| Relayer observes recipient addresses | **MEDIUM** | Relayers learn the recipient's external chain address when executing withdrawals. This is inherent to the HTLC model. |
| Deposit correlation via timing | **LOW** | An observer monitoring both DarkWow and an external chain could correlate deposits by timing analysis. Fresh addresses per deposit mitigate this. |
| Nullifier linkability | **LOW** | Nullifiers reveal that a specific deposit was spent but not by whom. Over time, spending patterns could enable heuristics. |

### Liveness Risk

| Risk | Severity | Details |
|------|----------|---------|
| Relayer centralization | **LOW** | `RegisterRelayerV1` establishes a registered relayer set enabling multi-relayer marketplace. `AcceptWithdrawalV1` prevents withdrawal monopolization. Fee caps protect against pricing abuse. Reputation transparency incentivizes competition. |
| No automated relayer restart | **MEDIUM** | Until Phase 3 (health check + watchdog) is deployed, relayer crashes require manual intervention. |
| Guaranteed withdrawal circuit breaker | **LOW** | When `GUARANTEED_PENDING` reaches `MAX_GUARANTEED_TOTAL`, new guaranteed withdrawals are rejected. Users fall back to standard mode. |

### Counterparty Risk

| Risk | Severity | Details |
|------|----------|---------|
| Backer capital at relayer's operational risk | **LOW** | Backers can set `min_success_rate_bp` and `max_slash_count` thresholds in `DeployCapitalV1`. Reputation-gated deployment (Phase 2d) prevents capital from flowing to poorly-performing relayers. Force settlement protects fee distribution. |
| Pool member slashing | **LOW** | Per-member slash tracking is deployed in `SlashCoverageV1` and `PoolStakeRegistry`. Individual members' `slash_count` is incremented on each slash event. `RebalancePoolSharesV1` adjusts share weights accordingly. |
| Bridge contract upgrade risk | **LOW** | Contract upgrades require governance. A malicious upgrade could introduce vulnerabilities. |

## ZK Circuit Safety

### Opcodes Used

| Opcode | Circuit | Safety Status |
|--------|---------|---------------|
| `poseidon_hash` | deposit_v1, withdraw_v1 | Standard — production |
| `constrain_equal_base` | deposit_v1, withdraw_v1 | Standard — production |
| `range_check` | deposit_v1, withdraw_v1 | Standard — production |
| `ec_mul_base` | deposit_v1 | Standard — production |
| `ec_get_x` / `ec_get_y` | deposit_v1 | Standard — production |
| `merkle_root` | deposit_v1 | Standard — production |
| `sparse_merkle_root` | withdraw_v1 | Standard — production |
| `less_than_strict` | withdraw_v1 | Constrain-only — sound |
| `zero_cond` | deposit_v1 | Conditional select — sound |

### Not Used (Experimental / Unverified)

| Opcode | Reason Avoided |
|--------|---------------|
| `LessThanOrEqual` | Gate soundness unverified — returns Boolean |
| `IsEqualBase` | Delta-invert soundness issue when `a == b` |
| `schnorr_verify` | Not implemented in zkVM |

No experimental opcodes are used in production circuits. `less_than_strict` is constrain-only (no Boolean return), which is sufficient for minimum amount assertions.

## User Guidance

If you choose to use these contracts:

1. **Start small** — test with minimal amounts before depositing significant value
2. **Use standard mode** — guaranteed withdrawal premiums are only worthwhile if you trust the specific relayer
3. **Set `max_fee_bp`** — always specify a fee cap in your withdrawal parameters
4. **Monitor your withdrawals** — be prepared to reassign or cancel if the relayer is unresponsive
5. **Verify relayer reputation** — check relayer slash history and settlement frequency before using guaranteed mode
6. **Diversify relayers** — don't depend on a single relayer for critical withdrawals
7. **Keep your secret safe** — your deposit secret is the only way to authorize a withdrawal; losing it means permanent fund loss
8. **Don't trust, verify** — review the contract code yourself or wait for a third-party audit

## Acknowledgments

- **Simulation system** (`sim/`): Python discrete-event engine that modeled all 10 failure scenarios
- **Code contributors**: DarkWow development team
- **Review**: Internal review only — no independent third-party audit has been performed

## See Also

- [Bridge Contract README](bridge/README.md) — Detailed contract documentation
- [Relayer Endowment README](relayer_endowment/README.md) — Endowment contract documentation
- [Simulation Report](../../sim/report.md) — Full simulation results and failure mode catalog
- [Relayer Economics](../../doc/src/relayer/relayer_economics.md) — Economic model and incentives
- [Universal Relayer Docs](../../doc/src/relayer/relayer.md) — Relayer operations guide
