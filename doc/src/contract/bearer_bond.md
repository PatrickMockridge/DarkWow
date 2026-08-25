# Bearer Bond — Fixed-Interest Staking Contract

A toolkit for contracts that need to raise capital. Stake commitments represent capital
positions that earn a fixed interest rate. Maturity is ZK-committed in the commitment
commitment. Interest is computed deterministically from on-chain state — no
issuer reporting, no self-declared profits. Coverage is verifiable by anyone.
If coverage falls below 100%, the terms void and holders can exit early.

## Functions

| Opcode | Function | Who | Does | ZK Circuits |
|--------|----------|-----|------|-------------|
| `0x00` | IssueStakeV1 | Issuer | Create staking pool, mint stake commitments to staker | BlindOutput_V1 |
| `0x01` | TransferStakeV1 | Holder | Transfer stake to new holder. `last_claim_block` preserved — unclaimed interest travels with the commitment | Burn_V1, BlindOutput_V1 |
| `0x02` | RequestInterestV1 | Holder | Request interest payment. Proves bond ownership (Burn_V1), provides fresh payment key. Like presenting a physical bond coupon. | Burn_V1 |
| `0x03` | EmergencyUnstakeV1 | Holder | Exit before maturity when coverage < 100% | Burn_V1, Redeem_V1 |
| `0x04` | UnstakeV1 | Holder | Withdraw principal at or after maturity. Rejects if `current_block < maturity_block` | Burn_V1, Redeem_V1 |
| `0x05` | BurnStakeV1 | Issuer | Retire staking pool, destroy remaining stake commitments | Burn_V1 |
| `0x06` | ProveCoverageV1 | Issuer/Holder | Submit ZK proof that reserves cover principal + interest obligations | ProveCoverage_V1 |
| `0x07` | VerifyCoverageV1 | Holder | Read latest coverage report from `bonds_info` tree (read-only, no state change) | *(none)* |
| `0x08` | PayInterestV1 | Issuer | Pay a pending interest claim. Creates fresh payment commitment (BlindOutput_V1) to holder's payment key. Updates `last_claim_block`, marks claim Paid. | BlindOutput_V1 |

### Parameters

IssueStakeV1:
```rust
struct IssueStakeParamsV1 {
    min_claim: u64,              // Dust protection threshold
    issuer_contract: ContractId, // Parent contract identifier
    asset_id: pallas::Base,      // Staking pool series identifier
    commitment: BondCommitment,        // Initial stake commitment (BlindOutput_V1)
}
```

RequestInterestV1:
```rust
struct RequestInterestParamsV1 {
    bond_input: BondInput, // Proves bond ownership (Burn_V1)
    claim_block: u64,      // Current block height
    payment_key: pallas::Base, // Fresh one-time key for issuer payment
    min_claim: u64,        // Dust protection
}
```

PayInterestV1:
```rust
struct PayInterestParamsV1 {
    bond_token_commit: pallas::Base, // Identifies the bond
    claim_block: u64,                // Identifies which claim
    interest_coin: BondCommitment,         // BlindOutput_V1 to holder's payment_key
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
    series_asset_id: pallas::Base,
    total_outstanding: u64,          // Total staked principal
    total_interest_obligation: u64,  // Total accrued interest obligation
    reserve_amount: u64,             // Issuer's reserve balance
    coverage_ratio_bps: u64,         // reserve / (outstanding + interest) * 10000
    report_block: u64,
    proof: Vec<u8>,                  // ProveCoverage_V1 ZK proof
}
```

## Interest Claim Flow — Two-Step Request → Pay

Interest claims use a two-step flow modeled on how physical bearer bonds work:
the holder presents the bond to claim interest, and the issuer pays against it.
Unlike the old unilateral model where the holder created their own payout commitment,
the burden is on the holder to ask, and the issuer must ringfence reserves to
cover outstanding claims.

### Step 1: RequestInterestV1 — Holder Asks

The holder submits a Burn_V1 ZK proof proving they own the bond. This is like
presenting a physical bond coupon at the issuer's window. The proof reveals the
bond's nullifier in the public inputs, identifying which bond is being claimed
against, but the entrypoint **does not write the nullifier** to the nullifiers
tree — the commitment is not consumed.

The holder also provides a `payment_key`, a fresh one-time public key for the
issuer to pay to. Each request uses a new key, making payments unlinkable.

The entrypoint:
1. Looks up the stake commitment from the commitment set, verifies `claim_block > last_claim_block`
2. Looks up `BondSeriesInfo`, checks the series is `Active`
3. Computes interest deterministically: `principal × rate × blocks_elapsed / (10000 × 15_768_000)`
4. Checks `interest >= min_claim` (dust protection)
5. Rejects if a pending claim already exists for this bond (prevents duplicate requests)
6. Stores a `RequestedClaim` record in the `bonds_info` tree:

```rust
struct RequestedClaim {
    interest_amount: u64,        // Deterministically computed
    payment_key: pallas::Base,   // Holder's one-time receiving key
    status: ClaimStatus,         // Pending or Paid
}
```

The claim is keyed by `(bond_token_commit, claim_block)`. **`last_claim_block` is
NOT updated yet.** The bond commitment stays exactly as it was. The pending claim record
is the only on-chain trace.

### Step 2: PayInterestV1 — Issuer Pays

The issuer (or their wallet) scans the `bonds_info` tree for pending `RequestedClaim`
records on series they issued. For each pending claim, they call `PayInterestV1`
with a BlindOutput_V1 payment commitment addressed to the holder's `payment_key`.

The entrypoint:
1. Looks up the claim record, verifies `status == Pending`
2. Looks up a coverage report for the series — the issuer must have proven reserves
3. Updates `last_claim_block` on the stake commitment to `claim_block`
4. Stores the payment commitment in the commitment set
5. Marks the claim `status = Paid`

The issuer is the ZK prover for the payment commitment, not the holder. Each payment
uses a fresh random `coin_blind` and `value_blind`, so payment addresses are
unlinkable — the issuer cannot track the holder across payments.

### Why This Design

- **Holder asks first, issuer responds.** Like turning up with a physical bond
  certificate. The issuer doesn't know who holds the bond until they present it.
- **No lost funds if the issuer drags their feet.** `last_claim_block` is only
  advanced when the issuer actually pays. If the issuer never pays, the holder
  hasn't lost their claim right — the pending record is on-chain evidence.
- **Ringfencing is enforced.** The coverage check in PayInterestV1 means the
  issuer must have filed a coverage report proving `reserves >= total_outstanding
  + total_interest_obligation` for the series. If they haven't set money aside,
  they can't pay claims, coverage deteriorates, the series voids, and holders
  can EmergencyUnstakeV1.
- **The same nullifier appears twice — by design.** It's revealed in the
  RequestInterestV1 public inputs (identifying which bond) and again later
  when the bond is transferred or unstaked (the actual consumption). This is
  inherent to the bearer instrument model: presenting the coupon identifies
  the bond, just as spending it does.
- **Payment addresses are unlinkable.** Fresh blinding per payment means each
  interest payout looks like a completely new commitment. The issuer can't correlate
  payments to build a profile of the holder.

### What If the Issuer Never Pays?

The pending claim blocks further interest requests for the same bond (the
entrypoint rejects overlapping claims). If the issuer systematically fails to
pay, interest obligations accumulate, the coverage ratio drops below 100%, the
series voids, and holders can EmergencyUnstakeV1. The pending claim record
serves as on-chain evidence of the missed obligation.

There is no cancel mechanism — the claim stays pending until paid. This is
intentional: the issuer committed to pay interest when they created the series.
A pending claim is a liability on their books, visible to all holders.

## Data Model

### CoinAttributes (ZK-committed)

The commitment is a Poseidon hash of these fields — identity is cryptographically
bound:

```rust
struct CoinAttributes {
    public_key: pallas::Base,    // H(owner_secret)
    value: u64,                  // Principal
    asset_id: pallas::Base,      // Series identifier
    spend_hook: pallas::Base,    // Cross-contract callback target
    user_data: pallas::Base,     // Application-specific
    blind: pallas::Base,         // Commitment blinding factor
    maturity_block: u64,         // Block when unstaking is allowed (ZK-committed)
}
// Commitment = poseidon_hash([public_key, value, asset_id, spend_hook, user_data, blind, maturity_block])
```

Maturity is in the hash — the issuer cannot alter it after issuance.

### BondCommitment (on-chain)

ZK-proven fields (common with PN) plus plaintext governance metadata:

```rust
struct BondCommitment {
    value_commit: pallas::Point,    // Pedersen commitment of principal (private)
    token_commit: pallas::Base,     // H(asset_id, token_blind)
    nullifier: Nullifier,           // H(secret, commitment)
    merkle_root: MerkleNode,        // Tree root at commitment creation
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
`poseidon_hash(series_asset_id)`:

```rust
struct BondSeriesInfo {
    series_asset_id: pallas::Base,
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

Stored in `bonds_info` tree keyed by `(series_asset_id, report_block)`:

```rust
struct CoverageReport {
    series_asset_id: pallas::Base,
    total_outstanding: u64,          // Total staked principal
    total_interest_obligation: u64,  // Total interest obligation
    reserve_amount: u64,             // Issuer's reserve balance
    coverage_ratio_bps: u64,         // coverage = reserve / (outstanding + interest) * 10000
    report_block: u64,               // Block height of this report
}
```

### RequestedClaim

On-chain record of a pending or paid interest claim, stored in `bonds_info`
keyed by `(token_commit, claim_block)`:

```rust
enum ClaimStatus {
    Pending = 0,  // Awaiting issuer payment
    Paid = 1,     // Payment completed
}

struct RequestedClaim {
    interest_amount: u64,       // Computed deterministically at request time
    payment_key: pallas::Base,  // Holder's one-time key for receiving payment
    status: ClaimStatus,
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
`last_claim_block` is updated on the stake commitment when the issuer pays
(PayInterestV1), not when the holder requests. The pending claim record blocks
duplicate requests for the same period in the meantime.

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
// In your contract's entrypoint, issue a stake commitment:
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
| Burn_V1 | Reused from PN | Spend proofs (TransferStake, Unstake, EmergencyUnstake, BurnStake) + bond ownership proof (RequestInterest — nullifier NOT written to tree) |
| BlindOutput_V1 | Reused from PN | Output commitment creation (IssueStake, TransferStake) + payment commitment creation (PayInterest — issuer is prover) |
| Redeem_V1 | Reused from PN | Zero-value receipt commitments (Unstake, EmergencyUnstake) |
| ProveCoverage_V1 | Bearer Bond only | Coverage ratio proof with `base_div` |

RequestInterestV1 uses Burn_V1 in a new pattern: the proof proves knowledge of the
secret (ownership), and the nullifier identifies which bond, but the commitment is NOT
consumed — the same nullifier appears again when the bond is eventually
transferred or unstaked. PayInterestV1 shifts BlindOutput_V1 from the holder
to the issuer: the issuer creates the payment commitment with fresh blinding per payment.

## Files

```
src/contract/bearer_bond/
├── Cargo.toml
├── Makefile
├── src/
│   ├── lib.rs                 # BearerBondFunction enum, tree constants, circuit bins
│   ├── error.rs               # BearerBondError (30 variants)
│   ├── model/mod.rs           # CoinAttributes, BondCommitment, BondSeriesInfo, CoverageReport,
│   │                           # all Params/Update types, calculate_interest()
│   ├── entrypoint/mod.rs      # WASM entrypoint (init, exec, apply, metadata)
│   ├── capability.rs          # Capability descriptor (6 capability types)
│   ├── validation.rs          # is_coverage_voided(), MIN_COVERAGE_RATIO_BPS
│   └── client/                # Client-side proof builders (feature = "client")
│       ├── mod.rs
│       ├── issue_stake_v1.rs
│       ├── transfer_stake_v1.rs
│       ├── request_interest_v1.rs
│       ├── pay_interest_v1.rs
│       ├── emergency_unstake_v1.rs
│       ├── unstake_v1.rs
│       ├── burn_stake_v1.rs
│       └── prove_coverage_v1.rs
├── proof/
│   ├── burn.zk
│   ├── blind_output.zk
│   ├── redeem.zk
│   └── prove_coverage.zk
└── tests/
    └── integration.rs
```

## Capability Types

| Discriminant | Name | Source | Consumable |
|---|---|---|---|
| `0x00` | CAP_STAKE | Unspent stake in wallet | Yes |
| `0x01` | CAP_INTEREST_RIGHT | Stake commitment with unclaimed interest | No |
| `0x02` | CAP_UNSTAKE_RIGHT | Stake commitment at or past maturity | No |
| `0x03` | CAP_RECEIPT | Receipt commitment after unstaking | No |
| `0x04` | CAP_COVERAGE_REPORT | Coverage report in bonds_info tree | No |
| `0x05` | CAP_EMERGENCY_UNSTAKE | Coverage < 100% — exit before maturity | No |

## Database Trees

```
commitments     - commitment → BondCommitment
nullifiers      - nullifier → spent
commitment merkle     - Merkle tree of all commitments
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

## See Also

- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis
