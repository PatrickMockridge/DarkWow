# Bearer Bond — Fixed-Interest Staking Contract

A toolkit for contracts that need to raise capital. Stake coins represent capital
positions that earn a fixed interest rate. Maturity is ZK-committed in the coin
commitment. Interest is computed deterministically from on-chain state — no
issuer reporting, no self-declared profits. Coverage is verifiable by anyone.
If coverage falls below 100%, the terms void and holders can exit early.

## Functions

| Opcode | Function | Who | Does | ZK Circuits |
|--------|----------|-----|------|-------------|
| `0x00` | IssueStakeV1 | Issuer | Create staking pool, mint stake coins to staker | BlindOutput_V1 |
| `0x01` | TransferStakeV1 | Holder | Transfer stake to new holder. `last_claim_block` preserved — unclaimed interest travels with the coin | Burn_V1, BlindOutput_V1 |
| `0x02` | ClaimInterestV1 | Holder | Claim deterministic interest. Stake coin persists, only `last_claim_block` updates | BlindOutput_V1 |
| `0x03` | EmergencyUnstakeV1 | Holder | Exit before maturity when coverage < 100% | Burn_V1, Redeem_V1 |
| `0x04` | UnstakeV1 | Holder | Withdraw principal at or after maturity. Rejects if `current_block < maturity_block` | Burn_V1, Redeem_V1 |
| `0x05` | BurnStakeV1 | Issuer | Retire staking pool, destroy remaining stake coins | Burn_V1 |
| `0x06` | ProveCoverageV1 | Issuer/Holder | Submit ZK proof that reserves cover principal + interest obligations | ProveCoverage_V1 |
| `0x07` | VerifyCoverageV1 | Holder | Read latest coverage report from `bonds_info` tree (read-only, no state change) | *(none)* |

### Parameters

IssueStakeV1:
```rust
struct IssueStakeParamsV1 {
    min_claim: u64,              // Dust protection threshold
    issuer_contract: ContractId, // Parent contract identifier
    token_id: pallas::Base,      // Staking pool series identifier
    coin: BondCoin,              // Initial stake coin (BlindOutput_V1)
}
```

ClaimInterestV1:
```rust
struct ClaimInterestParamsV1 {
    bond_input: BondInput, // The stake coin (not consumed)
    claim_block: u64,      // Current block height
    min_claim: u64,        // Dust protection
}
```

EmergencyUnstakeV1:
```rust
struct EmergencyUnstakeParamsV1 {
    bond_input: BondInput,
    coverage_report: CoverageReport,  // Must show coverage_ratio_bps < 10000
}
```

UnstakeV1:
```rust
struct UnstakeParamsV1 {
    bond_input: BondInput,
    current_block: u64,  // Checked against maturity_block
}
```

ProveCoverageV1:
```rust
struct ProveCoverageParamsV1 {
    series_token_id: pallas::Base,
    total_outstanding: u64,          // Total staked principal
    total_interest_obligation: u64,  // Total accrued interest obligation
    reserve_amount: u64,             // Issuer's reserve balance
    coverage_ratio_bps: u64,         // reserve / (outstanding + interest) * 10000
    report_block: u64,
    proof: Vec<u8>,                  // ProveCoverage_V1 ZK proof
}
```

## Data Model

### CoinAttributes (ZK-committed)

The coin is a Poseidon hash of these fields — identity is cryptographically
bound:

```rust
struct CoinAttributes {
    public_key: pallas::Base,    // H(owner_secret)
    value: u64,                  // Principal
    token_id: pallas::Base,      // Series identifier
    spend_hook: pallas::Base,    // Cross-contract callback target
    user_data: pallas::Base,     // Application-specific
    blind: pallas::Base,         // Coin blinding factor
    maturity_block: u64,         // Block when unstaking is allowed (ZK-committed)
}
// Coin = poseidon_hash([public_key, value, token_id, spend_hook, user_data, blind, maturity_block])
```

Maturity is in the hash — the issuer cannot alter it after issuance.

### BondCoin (on-chain)

ZK-proven fields (common with PN) plus plaintext governance metadata:

```rust
struct BondCoin {
    value_commit: pallas::Point,    // Pedersen commitment of principal (private)
    token_commit: pallas::Base,     // H(token_id, token_blind)
    nullifier: Nullifier,           // H(secret, coin)
    merkle_root: MerkleNode,        // Tree root at coin creation
    user_data_enc: pallas::Base,    // H(user_data, user_data_blind)
    spend_hook: pallas::Base,       // Cross-contract callback target
    signature_public: pallas::Base, // H(ephemeral_signature_secret)

    last_claim_block: u64,          // Block of last interest claim (plaintext)
    maturity_block: u64,            // Copied from CoinAttributes for entrypoint checks
    issuer_contract: ContractId,    // Parent contract identifier (plaintext)
}
```

Principal is hidden via Pedersen commitment. `maturity_block` appears both in
the ZK commitment (cryptographic binding) and as a plaintext copy (for efficient
entrypoint checks without witness data).

### BondSeriesInfo

Per-series configuration stored in `bonds_info` tree, keyed by
`poseidon_hash(series_token_id)`:

```rust
struct BondSeriesInfo {
    series_token_id: pallas::Base,
    interest_rate_bps: u64,     // Annual rate in basis points (500 = 5%)
    maturity_block: u64,        // Block when the series matures
    status: SeriesStatus,       // Active, Voided, or Matured
    issuer_contract: ContractId,
    total_staked: u64,          // Total staked principal in this series
}
```

### SeriesStatus

```rust
enum SeriesStatus {
    Active = 0,   // Staking, transfers, and interest claims allowed
    Voided = 1,   // Coverage failed — only emergency unstake allowed
    Matured = 2,  // Past maturity — only unstake allowed
}
```

### CoverageReport

Stored in `bonds_info` tree keyed by `(series_token_id, report_block)`:

```rust
struct CoverageReport {
    series_token_id: pallas::Base,
    total_outstanding: u64,          // Total staked principal
    total_interest_obligation: u64,  // Total interest obligation
    reserve_amount: u64,             // Issuer's reserve balance
    coverage_ratio_bps: u64,         // coverage = reserve / (outstanding + interest) * 10000
    report_block: u64,               // Block height of this report
}
```

## Interest Formula

Deterministic — no issuer input needed. Anyone can verify.

```
interest = principal × interest_rate_bps × blocks_elapsed / (BP_PRECISION × BLOCKS_PER_YEAR)
```

where `blocks_elapsed = current_block - last_claim_block`.

Constants:
- `BP_PRECISION = 10000`
- `BLOCKS_PER_YEAR = 15_768_000` (2-second blocks)

```rust
fn calculate_interest(principal: u64, interest_rate_bps: u64, blocks_elapsed: u64) -> Option<u64> {
    if blocks_elapsed == 0 { return Some(0); }
    let numerator = (principal as u128) * (interest_rate_bps as u128) * (blocks_elapsed as u128);
    let denominator = (BP_PRECISION as u128) * (BLOCKS_PER_YEAR as u128);
    let result = numerator / denominator;
    if result > u64::MAX as u128 { return None; }
    Some(result as u64)
}
```

The entrypoint reads `interest_rate_bps` from `BondSeriesInfo`, computes
`blocks_elapsed`, and checks `interest >= min_claim` for dust protection.
`last_claim_block` is updated on the stake coin after each claim to prevent
double-claiming.

## Coverage

**ProveCoverageV1** proves reserves cover total obligations: `total_outstanding +
total_interest_obligation`. Uses a dedicated ZK circuit (`ProveCoverage_V1`)
with `base_div` to compute the ratio:

```
coverage_ratio_bps = base_div(reserve_amount, total_outstanding + total_interest_obligation) × 10000
```

The entrypoint checks:
- `reserve_amount >= total_outstanding + total_interest_obligation`
- `coverage_ratio_bps >= 10000` (full coverage)

Coverage reports can be submitted by the issuer or any holder. Reports are
stored in the `bonds_info` tree.

**EmergencyUnstakeV1** unlocks when `coverage_ratio_bps < 10000`. The holder
submits a `CoverageReport` proving under-collateralization. The entrypoint calls
`is_coverage_voided()` to verify the report before allowing early exit.

## How to Use

Any contract that needs capital formation imports bearer bond by embedding its
calls as child calls. Set `issuer_contract` to the parent's `ContractId`.

```rust
// In your contract's entrypoint, issue a stake coin:
let issue_call = ContractCall {
    contract_id: bearer_bond_cid,
    data: serialize(&(BearerBondFunction::IssueStakeV1 as u8, issue_params)),
};
// This mint goes into your transaction's call tree as a child call.
// BearerBond validates proofs, your contract handles the capital.
```

See [Composability](composability.md) for the full cross-contract call pattern.
See [validation.rs](../../../src/contract/promissory_note/src/validation.rs) for
the PN validation helpers that work unchanged with bearer bond child calls.

## ZK Circuits

| Circuit | Source | Used For |
|---------|--------|----------|
| Burn_V1 | Reused from PN | Spend proofs (TransferStake, Unstake, EmergencyUnstake, BurnStake) |
| BlindOutput_V1 | Reused from PN | Output coin creation (IssueStake, TransferStake, ClaimInterest) |
| Redeem_V1 | Reused from PN | Zero-value receipt coins (Unstake, EmergencyUnstake) |
| ProveCoverage_V1 | Bearer Bond only | Coverage ratio proof with `base_div` |

Circuits are identical to PN's — bearer bond extends at the data model and
entrypoint layer, not the circuit layer.

## Files

```
src/contract/bearer_bond/
├── Cargo.toml
├── Makefile
├── src/
│   ├── lib.rs                 # BearerBondFunction enum, tree constants, circuit bins
│   ├── error.rs               # BearerBondError (30 variants)
│   ├── model/mod.rs           # CoinAttributes, BondCoin, BondSeriesInfo, CoverageReport,
│   │                           # all Params/Update types, calculate_interest()
│   ├── entrypoint/mod.rs      # WASM entrypoint (init, exec, apply, metadata)
│   ├── capability.rs          # Capability descriptor (6 capability types)
│   ├── validation.rs          # is_coverage_voided(), MIN_COVERAGE_RATIO_BPS
│   └── client/                # Client-side proof builders (feature = "client")
│       ├── mod.rs
│       ├── issue_stake_v1.rs
│       ├── transfer_stake_v1.rs
│       ├── claim_interest_v1.rs
│       ├── emergency_unstake_v1.rs
│       ├── unstake_v1.rs
│       ├── burn_stake_v1.rs
│       └── prove_coverage_v1.rs
├── proof/
│   ├── burn_v1.zk
│   ├── blind_output_v1.zk
│   ├── redeem_v1.zk
│   └── prove_coverage_v1.zk
└── tests/
    └── integration.rs
```

## Capability Types

| Discriminant | Name | Source | Consumable |
|---|---|---|---|
| `0x00` | CAP_STAKE | Unspent stake in wallet | Yes |
| `0x01` | CAP_INTEREST_RIGHT | Stake coin with unclaimed interest | No |
| `0x02` | CAP_UNSTAKE_RIGHT | Stake coin at or past maturity | No |
| `0x03` | CAP_RECEIPT | Receipt coin after unstaking | No |
| `0x04` | CAP_COVERAGE_REPORT | Coverage report in bonds_info tree | No |
| `0x05` | CAP_EMERGENCY_UNSTAKE | Coverage < 100% — exit before maturity | No |

## Database Trees

```
coins           - coin_commit → BondCoin
nullifiers      - nullifier → spent
coin_merkle     - Merkle tree of all coins
info            - Contract metadata (version)
coin_roots      - Historical Merkle roots
nullifier_roots - Historical nullifier roots
bonds_info      - BondSeriesInfo + CoverageReport records
```

The `bonds_info` tree is unique to bearer bond. PN has no equivalent.

## Limitations

- **Coverage proves arithmetic, not reserves.** ProveCoverageV1 verifies the
  ratio computes correctly. It does not prove reserves exist on-chain or that
  the issuer hasn't withdrawn them.
- **No coverage freshness requirement.** A report filed at block N has no expiry.
  The issuer could withdraw reserves after filing and never file again.
- **Emergency unstake requires a prior report.** If no coverage report exists
  for a series, holders cannot prove under-collateralization even if the issuer
  is insolvent.
- **Interest is deterministic but uninsured.** The math is always correct, but
  if the issuer is insolvent, holders may not recover funds despite correct
  computation.
