# Bearer Bond: Profit-Share Staking Contract

*Open-source governance toolkit for capital formation. Imported by any contract
that needs to raise capital with known rules and a level playing field.*

## Design Philosophy: Money vs. Governance

[Promissory Note](promissory_note.md) is **lightweight fungible currency**.
Mint, transfer, redeem — and nothing else. No governance. Governance is
**outside** the currency, delegated to whoever issues it.

Bearer Bond is the **governance toolkit** that any issuer imports when they
need capital formation. Someone raising capital is raising out of *need* —
they aren't equipped to design bespoke governance, and they shouldn't have
to. They import bearer bond and get a known, open-source set of rules.

This separation keeps both contracts clean:

| | Promissory Note | Bearer Bond |
|---|---|---|
| Role | Lightweight currency | Capital formation + governance |
| Governance | None (delegated to issuer) | Self-contained, imported as plugin |
| Who uses it | Anyone transacting | Issuers raising capital |
| Design goal | Minimal, fast, private | Standard rules, known parameters |

When every issuer imports the same governance toolkit, it's a **level playing
field**. Investors know the parameters going in — no pig in a poke, no mystery
boxes. The same `base_div` coverage proof, the same profit-share formula, the
same audit trail.

## Overview

A stake coin is a tradeable capital position. The holder provides capital to
the issuer, the issuer does work, and profits are shared pro-rata. If there
are no profits, there are no payouts — risk is shared between capital provider
and entrepreneur. Unclaimed profit distributions travel with the stake coin
on transfer.

Bearer Bond does NOT repeat Promissory Note's token model. It reuses PN's ZK
circuits (Burn_V1, BlindOutput_V1, Redeem_V1) unchanged. The extension is
entirely at the data model and entrypoint layer: four extra plaintext fields
on the coin, seven contract functions including self-contained governance,
and a plugin architecture that any parent contract can use for fundraising.

## Why Profit-Share Instead of Fixed Interest

| Problem | Fixed-Interest Bond | Profit-Share Stake |
|---------|---------------------|-------------------|
| Riba (usury) | Prohibited in Islamic finance | Permitted — risk is shared |
| Liquidity risk | Fixed obligations decouple from revenue | Distributions tied to actual earnings |
| Shared destiny | Issuer bears all downside | Capital and entrepreneur aligned |
| Transferability | Bond is a debt claim | Stake coin IS the position — tradeable |

The design eliminates riba (making the contract accessible to ~2 billion
Muslims), prevents liquidity cascades by linking payouts to real revenue, and
aligns incentives: the issuer only distributes what was actually earned.

## What Bearer Bond Adds on Top of Promissory Note

Bearer Bond reuses PN's privacy infrastructure — coin commitments, nullifiers,
Pedersen value conservation, AEAD note encryption, spend hooks — exactly as
documented in [Promissory Note](promissory_note.md). The additions are:

| Layer | PN | Bearer Bond |
|-------|----|-------------|
| Role | Lightweight currency | Capital formation + governance toolkit |
| Coin fields | `value`, `token_id`, `spend_hook`, `user_data` | **+** `last_claim_block`, `maturity_block`, `issuer_contract` (plaintext) |
| ZK circuits | Burn_V1, BlindOutput_V1, Redeem_V1, Mint_V1, TokenMint_V1 | Burn_V1, BlindOutput_V1, Redeem_V1, **ProveCoverage_V1** |
| Functions | TokenMint, Mint, Transfer, Burn, Redeem, OtcSwap | IssueStake, TransferStake, DeclareProfits, ClaimProfits, Unstake, BurnStake, **ProveCoverage** |
| Governance | None (delegated to issuer) | **Self-contained** — imported by any issuer, known rules |
| Value model | User-defined | `principal` = staked capital (Pedersen-committed, ZK-private) |
| Payouts | Redemption via RedeemV1 | Pro-rata profit claims + Unstake for principal |
| Issuer relationship | `token_auth_parent` capability | `issuer_contract` field + parent contract child calls |

Principal is ZK-committed via Pedersen commitment (`value_commit`) — just like
PN's coin value. `last_claim_block`, `maturity_block`, and `issuer_contract`
are plaintext governance metadata on `BondCoin` outside the ZK coin commitment.
This means PN's circuits work without modification. The entrypoint validates
bond metadata independently; the circuits only constrain the core privacy fields.

## The Lifecycle

```
IssueStakeV1 → TransferStakeV1 (xN) → ClaimProfitsV1 (xN) → UnstakeV1 → receipt
    0x00              0x01                    0x03                0x04
                       ↑                                            ↑
                   DeclareProfitsV1 (0x02)                    BurnStakeV1 (0x05)
                   (issuer declares profits)                 (issuer retires pool)

ProveCoverageV1 (0x06) — governance: issuer proves reserves >= outstanding stake
```

| Function | Opcode | Who | Description |
|----------|--------|-----|-------------|
| IssueStakeV1 | `0x00` | Issuer | Create staking pool, set terms, receive capital, mint stake coins |
| TransferStakeV1 | `0x01` | Holder | Transfer stake position — unclaimed profits travel with the coin |
| DeclareProfitsV1 | `0x02` | Issuer | Declare profit distribution for a series (amount + block range) |
| ClaimProfitsV1 | `0x03` | Holder | Claim pro-rata share of declared but unclaimed profits |
| UnstakeV1 | `0x04` | Holder | Burn stake coin, receive principal + unclaimed profits |
| BurnStakeV1 | `0x05` | Issuer | Retire staking pool, destroy remaining stake coins |
| ProveCoverageV1 | `0x06` | Issuer | Prove reserves cover outstanding stake (base_div ZK proof) |

### IssueStakeV1 (`0x00`)

Creates a new staking pool. The issuer defines the terms (maturity block) and
the initial stake coin is minted to the staker via a BlindOutput_V1 proof. The
staked principal is ZK-committed in the coin's `value_commit` — it never appears
on-chain as plaintext.

**Parameters:**
```rust
struct IssueStakeParamsV1 {
    maturity_block: u64,         // Block when stake can be withdrawn
    min_claim: u64,              // Dust protection threshold
    issuer_contract: ContractId, // Parent contract (or self)
    token_id: pallas::Base,      // Staking pool series identifier
    coin: BondCoin,              // Initial stake coin (BlindOutput_V1)
}
```

### TransferStakeV1 (`0x01`)

Transfer a stake position to a new holder. Internally identical to PN's
TransferV1 — Burn_V1 for the old coin, BlindOutput_V1 for the new coin.
The key difference: **`last_claim_block` is preserved on the output coin**.
Unclaimed profit distributions travel with the coin. The new holder inherits
the right to claim all unpaid profits from the previous holder's era.

### DeclareProfitsV1 (`0x02`)

Issuer declares: "between blocks X and Y, this series earned Z in profit."
No ZK proof — the issuer self-reports. The trust model is simple: if the
issuer lies, holders sell, and the stake coin price goes to zero. Future
phases will add profit verification via cross-contract attestations.

**Parameters:**
```rust
struct DeclareProfitsParamsV1 {
    series_token_id: pallas::Base, // Staking pool series
    profit_amount: u64,            // Total profit declared
    start_block: u64,              // Start of earning period
    end_block: u64,                // End of earning period
}
```

Declarations are stored in the `bonds_info` tree keyed by `(series_token_id,
end_block)`. The entrypoint validates: `profit_amount > 0`, `start_block <
end_block`.

### ClaimProfitsV1 (`0x03`)

Holder claims their pro-rata share of declared but unclaimed profits. The
stake coin is **not consumed** — only `last_claim_block` is updated. A
BlindOutput_V1 proof creates the profit payout coin.

The profit share is computed off-chain by scanning `ProfitDeclaration`
records since `last_claim_block`:

```
share = staked_principal × declared_profit / total_staked_in_series
```

The entrypoint verifies `share >= min_claim` (dust protection), checks that
the claim hasn't already been made for this block, and updates the stake coin.

### UnstakeV1 (`0x04`)

Withdraw principal + unclaimed profits at or after maturity. Combines Burn_V1
for the stake coin with Redeem_V1 for a zero-value receipt coin. The receipt
proves unstaking occurred — non-transferable, permanent on-chain record.

### BurnStakeV1 (`0x05`)

Issuer retires the staking pool. All remaining stake coins are burned via
Burn_V1 proofs. No outputs — the pool is destroyed.

### ProveCoverageV1 (`0x06`) — Governance

Issuer proves reserves cover outstanding stake obligations. Uses a dedicated
ZK circuit (ProveCoverage_V1) with `base_div` to compute the coverage ratio:

```
coverage_ratio_bps = base_div(reserve_amount, total_outstanding) × 10000
```

The entrypoint independently verifies `reserve_amount >= total_outstanding`
(>= 100% coverage required). Coverage reports are stored in the `bonds_info`
tree keyed by `(series_token_id, report_block)` — producing a permanent,
auditable trail.

**Parameters:**
```rust
struct ProveCoverageParamsV1 {
    series_token_id: pallas::Base, // Staking pool series
    total_outstanding: u64,         // Total staked principal
    reserve_amount: u64,            // Issuer's reserve balance
    coverage_ratio_bps: u64,        // reserve / outstanding * 10000
    report_block: u64,              // Block height of report
    proof: Vec<u8>,                 // ZK proof (ProveCoverage_V1)
}
```

Parent contracts verify solvency by reading the latest coverage report and
calling the stateless `verify_coverage()` helper in
[validation.rs](../../../src/contract/bearer_bond/src/validation.rs) —
no bespoke governance per contract.

## BondCoin: The NFT Stake Coin

`BondCoin` models each stake position as a **non-fungible coin** — an NFT
version of a Promissory Note. Principal is ZK-committed in `value_commit`
(like PN's `value`), not leaked as plaintext. Only governance/timing
metadata is plaintext.

```rust
struct BondCoin {
    // ZK-proven fields (same as PN's Input/Output)
    value_commit: pallas::Point,      // Pedersen commitment of principal (PRIVATE)
    token_commit: pallas::Base,       // H(token_id, token_blind)
    nullifier: Nullifier,             // H(secret, coin)
    merkle_root: MerkleNode,          // Tree root at coin creation
    user_data_enc: pallas::Base,      // H(user_data, user_data_blind)
    spend_hook: pallas::Base,         // Cross-contract callback target
    signature_public: pallas::Base,   // H(ephemeral_signature_secret)

    // Governance metadata (plaintext — timing/custody, not capital)
    last_claim_block: u64,            // Block of last profit claim
    maturity_block: u64,              // Block when unstaking is allowed
    issuer_contract: ContractId,      // Parent contract identifier
}
```

The coin commitment itself is identical to PN:
```
Coin = poseidon_hash(owner_pub, value, token_id, spend_hook, user_data, blind)
```
Bond metadata is NOT in this hash. This is by design — it means PN's ZK
circuits work unchanged, and the bearer bond's extension is purely at the
entrypoint layer.

### Privacy Model

Principal is hidden on-chain via Pedersen commitment (`value_commit`), just
like PN's coin value. An observer cannot sum principals across the coins
tree to determine how much capital a contract holds.

`last_claim_block`, `maturity_block`, and `issuer_contract` remain plaintext
— these are governance/timing fields that don't leak capital size.
Ecosystem brokers can handle fractionalization into smaller units.

## Profit Share Formula

Profit shares are computed pro-rata. The holder's share of a profit
declaration is proportional to their stake relative to the total staked
in that series:

```
share = staked_principal × declared_profit / total_staked
```

The calculation uses u128 intermediate arithmetic to prevent overflow:

```rust
fn calculate_profit_share(staked: u64, total_staked: u64, declared_profit: u64) -> Option<u64> {
    if total_staked == 0 { return None; }
    let numerator = (staked as u128) * (declared_profit as u128);
    let result = numerator / (total_staked as u128);
    if result > u64::MAX as u128 { return None; }
    Some(result as u64)
}
```

`total_staked_in_series` is computed by the entrypoint from all stake coins
with matching `token_commit`. The client scans profit declarations since
`last_claim_block` and sums the holder's pro-rata share. The contract then
validates the claim parameters.

## Plugin Architecture

Bearer Bond is designed as a **plugin** — any contract that needs capital
formation can embed its calls as child calls. The `issuer_contract` field on
`BondCoin` identifies the parent contract.

```
Parent Contract (promissory_note issuer, betting contract, auction)
  │
  ├── Child Call: BearerBond::IssueStakeV1
  │   → Creates staking pool, mints stake coins to investors
  │
  ├── Child Call: BearerBond::DeclareProfitsV1
  │   → Issuer reports profits (self-reported or verified)
  │
  └── Child Call: BearerBond::BurnStakeV1
      → Retire pool when venture concludes
```

### Cross-Contract Validation

Parent contracts verify bearer bond child calls using the same validation
helpers PN exposes ([validation.rs](../../../src/contract/promissory_note/src/validation.rs)):

```rust
// In the parent contract's entrypoint:
validate_child_contract_id(&child_call.contract_id, &bearer_bond_cid)?;
// Validates the Pedersen value commitment matches expected principal
validate_child_value_commit(&child_call.data, expected_principal, blind_seed)?;
```

The `spend_hook` mechanism works identically to PN — set it to the issuer's
contract ID to route all burns through the parent contract for atomic
balance-sheet updates.

### Profit Verification (Future)

Phase 1 trusts the issuer (self-reporting). Phase 2+ will use the
`issuer_contract` field as a profit oracle: if the issuer is itself a
contract, the bearer bond can call `issuer_contract.verify_profits(amount,
start, end)` to validate declarations on-chain. This enables:

- **Promissory Note issuers**: stake coins backed by token redemption rights
- **Betting contracts**: house stake with profit sharing for liquidity providers
- **Auction contracts**: fundraising via staked participation

## Capability Types

Bearer Bond defines four capability types extending PN's capability model:

| Type | Discriminant | Source | Consumable |
|------|-------------|--------|------------|
| Stake Coin (`CAP_STAKE`) | `0x00` | Unspent stake in wallet | Yes |
| Profit Right (`CAP_PROFIT_RIGHT`) | `0x01` | Stake coin with unclaimed declared profits | No |
| Unstake Right (`CAP_UNSTAKE_RIGHT`) | `0x02` | Stake coin at or past maturity | No |
| Receipt (`CAP_RECEIPT`) | `0x03` | Receipt coin after unstaking | No |
| Coverage Report (`CAP_COVERAGE_REPORT`) | `0x04` | Governance: issuer proved solvency | No |

The capability lifecycle:
```
IssueStakeV1 → CAP_STAKE (tradeable)
    │
    ├── DeclareProfitsV1 → CAP_PROFIT_RIGHT (derived right)
    │   └── ClaimProfitsV1 → profit payout (exercises right)
    │
    ├── maturity reached → CAP_UNSTAKE_RIGHT (derived right)
    │   └── UnstakeV1 → CAP_RECEIPT (exercises right, coin consumed)
    │
    └── TransferStakeV1 → new CAP_STAKE (capability delegated)
```

## Client Builders

Seven client-side proof builders follow the PN builder pattern (`XxxCallInput`
→ `XxxRevealed` → `XxxCallDebris` → `XxxCallBuilder`):

| Builder | ZK Circuits | Key Detail |
|---------|-------------|------------|
| `IssueStakeCallBuilder` | BlindOutput_V1 | Initial stake coin minted to staker |
| `TransferStakeCallBuilder` | Burn_V1 + BlindOutput_V1 | `last_claim_block` preserved — profits travel with coin |
| `DeclareProfitsCallBuilder` | *(none)* | Issuer self-reports; no ZK needed |
| `ClaimProfitsCallBuilder` | BlindOutput_V1 | Stake coin NOT consumed — only `last_claim_block` updated |
| `UnstakeCallBuilder` | Burn_V1 + Redeem_V1 | Zero-value receipt coin proves unstaking |
| `BurnStakeCallBuilder` | Burn_V1 | Issuer retires pool |
| `ProveCoverageCallBuilder` | ProveCoverage_V1 | Issuer proves solvency (governance) |

Builders are gated behind `#[cfg(feature = "client")]` and live in
[src/contract/bearer_bond/src/client/](../../../src/contract/bearer_bond/src/client/).

Each builder returns params + proofs that parent contracts embed into their
`ContractCall` trees. See [Composability](composability.md) for the
cross-contract call pattern.

## Database Trees

```
BEARER_BOND_CONTRACT_COINS_TREE                - coin → BondCoin
BEARER_BOND_CONTRACT_NULLIFIERS_TREE            - nullifier → spent
BEARER_BOND_CONTRACT_COIN_MERKLE_TREE           - Merkle tree of all coins
BEARER_BOND_CONTRACT_INFO_TREE                  - contract metadata
BEARER_BOND_CONTRACT_COIN_ROOTS_TREE            - historical Merkle roots
BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE       - historical nullifier roots
BEARER_BOND_CONTRACT_BONDS_INFO_TREE            - staking pool metadata + profit declarations
```

The `bonds_info` tree is unique to Bearer Bond — it stores `ProfitDeclaration`
records keyed by `(series_token_id, end_block)`. PN has no equivalent.

## Files

```
src/contract/bearer_bond/
├── Cargo.toml              # dwow_bearer_bond_contract
├── Makefile                 # WASM + zkas compilation
├── src/
│   ├── lib.rs               # BearerBondFunction enum (7 variants), tree constants
│   ├── error.rs             # BearerBondError enum (31 variants)
│   ├── model/mod.rs         # BondCoin, BondInput, CoinAttributes, ProfitDeclaration,
│   │                         # all Params/Update types, calculate_profit_share()
│   ├── entrypoint/mod.rs    # WASM entrypoint (init, exec, apply, metadata)
│   ├── capability.rs        # Capability descriptor (CAP_STAKE, CAP_PROFIT_RIGHT, etc.)
│   ├── validation.rs        # Cross-contract validation helpers
│   └── client/              # Client API (feature = "client")
│       ├── mod.rs           # BearerBondNote, point_coords()
│       ├── issue_stake_v1.rs    # IssueStakeCallBuilder
│       ├── transfer_stake_v1.rs # TransferStakeCallBuilder
│       ├── declare_profits_v1.rs# DeclareProfitsCallBuilder
│       ├── claim_profits_v1.rs  # ClaimProfitsCallBuilder
│       ├── unstake_v1.rs        # UnstakeCallBuilder
│       ├── burn_stake_v1.rs     # BurnStakeCallBuilder
│       └── prove_coverage_v1.rs # ProveCoverageCallBuilder (governance)
├── proof/
│   ├── burn_v1.zk           # Copied from PN (identical)
│   ├── blind_output_v1.zk   # Copied from PN (identical)
│   ├── redeem_v1.zk          # Copied from PN (identical)
│   ├── prove_coverage_v1.zk # Coverage ratio proof (base_div)
│   └── *.zk.bin             # Compiled ZK binaries
└── tests/
    └── integration.rs       # Integration tests
```

## Wallet Integration

Bearer Bond is wired into the drk wallet so any contract that needs capital
formation can discover, deploy, and invoke it. The integration follows the
standard 6-file wallet surface:

| File | What It Does |
|------|-------------|
| `bin/drk/Cargo.toml` | `dwow_bearer_bond_contract` dependency with `client` + `no-entrypoint` features |
| `bin/drk/src/contract_imports.rs` | `BEARER_BOND_CONTRACT_ID` OnceLock, register arm, module re-exports, `BearerBondContract: Contract` trait impl (depends on PromissoryNote for ZK circuits) |
| `bin/drk/src/contract_metadata.rs` | 7-function metadata: issue_stake, transfer_stake, declare_profits, claim_profits, unstake, burn_stake, prove_coverage |
| `bin/drk/src/lib.rs` | `"bearer_bond"` arm in `invoke_contract` dispatch |
| `bin/drk/src/capability.rs` | `resolve_bearer_bond()` — scans `coins` sled tree, derives `CAP_STAKE`/`CAP_PROFIT_RIGHT`/`CAP_UNSTAKE_RIGHT`, builds per-coin actions |
| `bin/drk/src/main.rs` | Descriptor registration: `resolver.register_descriptor(dwow_bearer_bond_contract::capability::descriptor(*cid))` |

### Deploy and Register

```bash
# Deploy bearer bond (one-time per chain)
drk contract deploy bearer_bond

# Register with the wallet so subsequent commands find it
drk contract register bearer_bond <contract_id>
```

After registration, `drk capability` shows the bearer_bond descriptor and any
stake coins the wallet holds.

### Capability Resolution

The resolver scans the contract's `coins` sled tree for BondCoin instances
owned by the wallet. Ownership is checked via `poseidon_hash([secret.inner()]) ==
BondCoin.signature_public` — matching Promissory Note's ZK privacy model where
pubkeys are hashed base field elements, not raw EC points.

Five capability types are resolved:

| Capability | When Derived |
|---|---|
| `CAP_STAKE` | Wallet holds an unspent BondCoin |
| `CAP_PROFIT_RIGHT` | Unclaimed profit declarations exist in `bonds_info` tree since `last_claim_block` |
| `CAP_UNSTAKE_RIGHT` | Always derived (contract enforces maturity check on-chain) |
| `CAP_RECEIPT` | Receipt coin after unstaking |
| `CAP_COVERAGE_REPORT` | Governance — issuer's coverage report visible to all |

### How Parent Contracts Plug In

Any contract that needs capital formation imports bearer bond by embedding its
calls as child calls. The `issuer_contract` field on `BondCoin` identifies the
parent. The wallet discovers bearer bond calls during scanning via the
`BEARER_BOND_CONTRACT_ID` handler — BlindOutput_V1 outputs are decrypted as
BondCoin notes, and profit declarations are tracked for observability.

No bespoke governance per parent contract. Every issuer imports the same
open-source toolkit with known parameters.

See **[Wallet Architecture](../arch/wallet.md)** for the full resolver walkthrough,
**[Wallet Scanning](../arch/wallet_scanning.md)** for the scanning handler, and
**[Wallet Contract Tracking](../arch/wallet_contract_tracking.md)** for the
contract matching architecture.

## Related Contracts

- **[Promissory Note](promissory_note.md)** — The foundation. Bearer Bond reuses
  PN's ZK circuits, token model, spend hooks, and privacy layer. Read this first.
- **[Stablecoin](stablecoin.md)** — Could use bearer bonds for staked USDx
  issuance, sharing seigniorage with liquidity providers.
- **[Betting Stake](betting_stake.md)** — House stake with profit sharing for
  liquidity providers.
- **[Insurance Market](insurance_market.md)** — Premium staking with
  underwriting profit distributions.
- **[Composability](composability.md)** — Cross-contract call patterns for
  embedding Bearer Bond as a child contract.
