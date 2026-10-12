# Bearer Bond — Fixed-Interest Staking Contract

A toolkit for contracts that need to raise capital. Stake commitments represent capital
positions that earn a fixed interest rate. Maturity is ZK-committed in the commitment
commitment. Interest is computed deterministically from on-chain state — no
issuer reporting, no self-declared profits. Coverage is verifiable by anyone.
If coverage falls below 100%, the terms void and holders can exit early.

## Functions

| Opcode | Function | Who | Does | ZK Circuits |
|--------|----------|-----|------|-------------|
| `0x00` | IssueStakeV1 | Issuer | Create staking pool, mint stake commitments to staker | BlindOutput_V2 |
| `0x01` | TransferStakeV1 | Holder | Transfer stake to new holder. `last_claim_block` preserved — unclaimed interest travels with the commitment | Burn_V2, BlindOutput_V2 |
| `0x02` | RequestInterestV1 | Holder | Request interest payment. Proves bond ownership (Burn_V2), provides fresh payment key. Like presenting a physical bond coupon. | Burn_V2 |
| `0x03` | EmergencyUnstakeV1 | Holder | Exit before maturity when coverage < 100% | Burn_V2, Redeem_V2 |
| `0x04` | UnstakeV1 | Holder | Withdraw principal at or after maturity. Rejects if `current_block < maturity_block` | Burn_V2, Redeem_V2 |
| `0x05` | BurnStakeV1 | Issuer | Retire staking pool, destroy remaining stake commitments | Burn_V2 |
| `0x06` | ProveCoverageV1 | Issuer/Holder | Submit ZK proof that reserves cover principal + interest obligations | ProveCoverage_V1 |
| `0x07` | VerifyCoverageV1 | Holder | Read latest coverage report from `bonds_info` tree (read-only, no state change) | *(none)* |
| `0x08` | PayInterestV1 | Issuer | Pay a pending interest claim. Creates fresh payment commitment (BlindOutput_V2) to holder's payment key. Updates `last_claim_block`, marks claim Paid. | BlindOutput_V2 |
| `0x09` | RegisterSeriesV1 | Anyone | **Create the bond series record every other state-changing endpoint reads.** Keyed by `series_asset_id`; sets the series' `interest_rate_bps`, `maturity_block` and `issuer_contract`. Must be the first call on a fresh chain — `IssueStakeV1` requires the record it writes. | *(none)* |

> **`RegisterSeriesV1` exists because the read had no matching write.** `issue_stake_v1` requires
> `bonds_info[asset_id]` to exist and compares its `issuer_contract` against the caller's, while
> nothing in this contract ever created one — `BondSeriesInfo` was constructed only in its own
> `decode`, and `apply_prove_coverage` re-stores an existing series rather than making one. So on a
> fresh chain `IssueStakeV1` — the row above calling itself *"Create staking pool"* — answered
> `StakeNotFound` (error code 1), and every endpoint downstream of a stake inherited that. The
> registration is plaintext and first-come: a caller claims an unused `series_asset_id` and becomes
> its issuer for the series' life. There is no registry of legitimate issuers to check against, so
> the consequence is stated rather than guarded.

### Parameters

IssueStakeV1:
```rust
struct IssueStakeParamsV1 {
    min_claim: u64,              // Dust protection threshold
    issuer_contract: ContractId, // Parent contract identifier
    asset_id: pallas::Base,      // Staking pool series identifier
    commitment: BondCommitment,        // Initial stake commitment (BlindOutput_V2)
}
```

RequestInterestV1:
```rust
struct RequestInterestParamsV1 {
    bond_input: BondInput, // Proves bond ownership (Burn_V2)
    claim_block: u64,      // Current block height
    payment_key: pallas::Base, // Fresh one-time key for issuer payment
    min_claim: u64,        // Dust protection
}
```

PayInterestV1:
```rust
struct PayInterestParamsV1 {
    bond_commitment: pallas::Base,          // Identifies the bond
    claim_block: u64,                       // Identifies which claim
    interest_commitment: BondCommitment,    // BlindOutput_V2 to holder's payment_key
}
```

EmergencyUnstakeV1:
```rust
struct EmergencyUnstakeParamsV1 {
    bond_input: BondInput,
    receipt_commitment: pallas::Base,        // the zero-value receipt's note commitment
    receipt_token_commit: pallas::Base,      // the receipt's own token commitment
    receipt_spend_hook: pallas::Base,
    receipt_value_commit_x: pallas::Base,
    receipt_value_commit_y: pallas::Base,
}
```

UnstakeV1:
```rust
struct UnstakeParamsV1 {
    bond_input: BondInput,
    current_block: u64,  // Checked against maturity_block
    receipt_commitment: pallas::Base,        // the zero-value receipt's note commitment
    receipt_token_commit: pallas::Base,      // the receipt's own token commitment
    receipt_spend_hook: pallas::Base,
    receipt_value_commit_x: pallas::Base,
    receipt_value_commit_y: pallas::Base,
}
```

The `receipt_*` fields are the half of `OBL-C199` the doc was missing: `Redeem_V2` instances the
*receipt's* commitment, token commit, spend hook and value-commitment coordinates, and those are a
different note from the `bond_input` being consumed — the host cannot recompute any of them. The
arms published the *stake's* values for all five until that row moved the record onto the receipt.
`PayInterestParamsV1`'s first and third fields were named `bond_token_commit`/`interest_coin` here;
they are `bond_commitment`/`interest_commitment`.

ProveCoverageV1:
```rust
struct ProveCoverageParamsV1 {
    series_asset_id: pallas::Base,
    total_outstanding: u64,          // Total staked principal
    total_interest_obligation: u64,  // Total accrued interest obligation
    reserve_amount: u64,             // Issuer's reserve balance
    coverage_ratio_bps: u64,         // floor(reserve * 10000 / (outstanding + interest))
    report_block: u64,
    proof: Vec<u8>,                  // ProveCoverage_V2 ZK proof
}
```

**These four numbers are public.** They travel as plaintext in the call data, and the ratio is a
public input of the proof, because the host has to know them. What the proof adds is that the ratio
*is* the quotient of the three amounts — `coverage_ratio_bps` was a plain witness until 2026-09-20,
so the number that decides whether a series is voided was asserted by the issuer and proven by
nothing. What it does not add is secrecy, and it cannot: the reserves are off-chain, so nothing on
chain can check the amounts either. The coverage report is an **issuer attestation** — the issuer's
claim about its own books, made arithmetically consistent by the proof — and `is_coverage_voided` is
an issuer-controlled switch until the series holds reserves the contract can total itself. The
individual stakes (below) are commitments; the aggregate report is not.

## Interest Claim Flow — Two-Step Request → Pay

Interest claims use a two-step flow modeled on how physical bearer bonds work:
the holder presents the bond to claim interest, and the issuer pays against it.
Unlike the old unilateral model where the holder created their own payout commitment,
the burden is on the holder to ask, and the issuer must ringfence reserves to
cover outstanding claims.

### Step 1: RequestInterestV1 — Holder Asks

The holder submits a Burn_V2 ZK proof proving they own the bond. This is like
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
with a BlindOutput_V2 payment commitment addressed to the holder's `payment_key`.

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

### CommitmentAttributes (ZK-committed)

The commitment is a Poseidon hash of these fields — identity is cryptographically
bound:

```rust
struct CommitmentAttributes {
    public_key: pallas::Base,    // H(owner_secret)
    value: u64,                  // Principal
    asset_id: pallas::Base,      // Series identifier
    spend_hook: pallas::Base,    // Cross-contract callback target
    user_data: pallas::Base,     // Application-specific
    blind: pallas::Base,         // Commitment blinding factor
    maturity_block: u64,         // Stored, NOT committed — see below
}
// Commitment = poseidon_hash([4, public_key, value, asset_id, spend_hook, user_data, blind])
```

**Maturity is not in the hash.** This section was headed `CoinAttributes`, listed `maturity_block`
as `ZK-committed`, and ended "Maturity is in the hash — the issuer cannot alter it after issuance"
until 2026-10-12. The circuit hashes **seven** arguments and the maturity is not among them: the
trailing field made `to_commitment` a different function from the one `blind_output.zk` and
`redeem.zk` compute, so no client could produce a public-input vector the circuit accepted
(`OBL-Z15`). The maturity is stored on the record and compared against the chain height on the
unstake path — nothing stops an issuer changing it except that check.

### BondCommitment (on-chain)

ZK-proven fields (common with PN) plus plaintext governance metadata:

```rust
struct BondCommitment {
    value_commit: pallas::Point,    // Pedersen commitment of principal (private)
    commitment: pallas::Base,       // The note commitment = H(4, pub, value, asset, hook, data, blind)
    token_commit: pallas::Base,     // H(2, asset_id, asset_id_blind) — the token commitment
    series_asset_id: pallas::Base,  // The bond's series — the key `BondSeriesInfo` is stored under
    nullifier: Nullifier,           // H(secret, commitment)
    merkle_root: MerkleNode,        // Tree root at commitment creation
    user_data_enc: pallas::Base,    // H(user_data, user_data_blind)
    spend_hook: pallas::Base,       // Cross-contract callback target
    signature_public: pallas::Base, // H(ephemeral_signature_secret)

    last_claim_block: u64,          // Block of last interest claim (plaintext)
    maturity_block: u64,            // Stored, not committed — checked against chain height
    issuer_contract: ContractId,    // Parent contract identifier (plaintext)
}
```

Principal is hidden via Pedersen commitment. Two commitments, not one, and the difference matters:

* `commitment` is the note commitment — the value `BlindOutput_V2` and `Redeem_V2` expose as their
  first public input, the same seven-argument hash promissory note uses. It is carried in the record
  because the host cannot recompute it: its preimage holds `commitment_blind`, which is the holder's.
* `token_commit` binds `(asset_id, asset_id_blind)` and nothing else, so it identifies a note
  without saying anything about its value or its owner — those are carried by `value_commit` and
  `signature_public`, which the circuits expose. It is **not** the series key: `OBL-C199` found
  `issue_stake_v1` writing the series under `params.asset_id` while every later endpoint read it
  under `token_commit`, so the series was written under one key and read under another. The record
  now carries `series_asset_id` for that, because `token_commit` cannot supply it — its blind is
  per-proof, and a bond's `CommitmentAttributes` do not include the asset id.

This section said `maturity_block` appears "in the ZK commitment (cryptographic binding)" until
2026-09-20. It does not: the circuits hash seven arguments and the maturity is not among them. It is
stored and compared against the chain height on the unstake path, which is sufficient for what it is
for — but a reader who believed the old line would have thought the issuer could not alter it after
issuance, and nothing stops that except the entrypoint's own check.

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
total_interest_obligation`. A dedicated circuit (`ProveCoverage_V2`) computes the ratio as an
integer quotient — `base_div` computes `a·b^(p−2)`, a number near *p*, not a ratio:

```
coverage_ratio_bps = floor(reserve_amount * 10000 / (total_outstanding + total_interest_obligation))
```

The entrypoint checks that the series and block are named, that the report does not already exist
for that block, and nothing else:

- it does **not** require `reserve_amount >= total_outstanding + total_interest_obligation`, and
- it does **not** require `coverage_ratio_bps >= 10000`.

Both of those lines were wrong until 2026-09-20. A report *below* 100% is admitted on purpose: it
voids the series, and voiding is what makes `EmergencyUnstakeV1` reachable — rejecting sub-100%
reports is what had made emergency unstake unreachable. The ratio, not a rejection, is the
mechanism.

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
| Burn_V2 | `proof/burn.zk` | Spend proofs (TransferStake, Unstake, EmergencyUnstake, BurnStake) + bond ownership proof (RequestInterest — nullifier NOT written to tree) |
| BlindOutput_V2 | `proof/blind_output.zk` | Output commitment creation (IssueStake, TransferStake) + payment commitment creation (PayInterest — issuer is prover) |
| Redeem_V2 | `proof/redeem.zk` | Zero-value receipt commitments (Unstake, EmergencyUnstake). The receipt's note commitment is carried in the params as `receipt_commitment` — a different note from the `bond_input` being consumed, and one the host cannot recompute — and the stored receipt *record* is otherwise empty, because the params carry nothing else about it |
| ProveCoverage_V2 | `proof/prove_coverage.zk` | Coverage ratio proof, as an integer quotient |

This table's "Source" column read "Reused from PN" for the first three until 2026-10-12, and the
circuit names carried a `_V1` suffix. All four circuits are **this contract's own**, under
`src/contract/bearer_bond/proof/` with `_V2` namespaces, and none is byte-identical to its
PromissoryNote counterpart — what bearer_bond reuses is PN's *validation helpers*
(`validate_child_contract_id`, `validate_child_value_commit`), which is what the paragraph above
this section is about.

RequestInterestV1 uses Burn_V2 in a new pattern: the proof proves knowledge of the
secret (ownership), and the nullifier identifies which bond, but the commitment is NOT
consumed — the same nullifier appears again when the bond is eventually
transferred or unstaked. PayInterestV1 shifts BlindOutput_V2 from the holder
to the issuer: the issuer creates the payment commitment with fresh blinding per payment.

## Files

```
src/contract/bearer_bond/
├── Cargo.toml
├── Makefile
├── src/
│   ├── lib.rs                 # BearerBondFunction enum, tree constants, circuit bins
│   ├── error.rs               # BearerBondError (35 variants)
│   ├── model/mod.rs           # CommitmentAttributes, BondCommitment, BondSeriesInfo, CoverageReport,
│   │                           # all Params/Update types, calculate_interest()
│   ├── entrypoint/mod.rs      # WASM entrypoint (init, exec, apply, metadata)
│   ├── capability.rs          # Wallet-side capability descriptors (six `CAP_*` discriminants)
│   ├── validation.rs          # is_coverage_voided(), MIN_COVERAGE_RATIO_BPS
│   └── client/                # Client-side proof builders (feature = "client")
│       ├── mod.rs
│       ├── zkbins.rs          # the circuits' `.zk.bin` blobs, embedded
│       ├── issue_stake.rs
│       ├── transfer_stake.rs
│       ├── request_interest.rs
│       ├── pay_interest.rs
│       ├── emergency_unstake.rs
│       ├── unstake.rs
│       ├── burn_stake.rs
│       └── prove_coverage.rs
├── proof/
│   ├── burn.zk
│   ├── blind_output.zk
│   ├── redeem.zk
│   └── prove_coverage.zk
└── tests/
    └── integration.rs
```

## Capability Types

These six are **wallet-side discriminants** (`capability.rs`, `CapabilityId::derive(contract_id,
CAP_*, …)`), used to build the position catalogues the wallet resolves. They are not the
contract's published capability ABI: `manifest.toml` declares **one** capability,
`bond_holder` (discriminant `0`), and a manifest-driven resolver sees only that. The gap between
the two is `F-544` — the doc and the manifest disagree about the capability ABI, and nothing
reconciles them.

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
commitment_set      - commitment → BondCommitment
nullifiers          - nullifier → spent
commitment_merkle   - Merkle tree of all commitments
info                - Contract metadata (version)
commitment_roots    - Historical Merkle roots
nullifier_roots     - Historical nullifier roots
bonds_info          - BondSeriesInfo + CoverageReport records
```

The tree names are the manifest's and the contract's own constants'. Three of the seven were
written here as `commitments`, `commitment merkle` and `coin_roots` until 2026-10-12 — prose
descriptions rather than the keys the contract uses, in a list whose whole purpose is to give a
reader the key.

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
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis
