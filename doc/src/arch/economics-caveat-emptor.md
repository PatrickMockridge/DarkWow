# Caveat Emptor: Pricing, Coverage, and the Adversarial Market for Bearer Instruments

## Introduction — The Lego Brick Protocol

DarkWow provides the lego bricks, not the building. The
[Promissory Note](../contract/promissory_note.md) contract gives you multi-token
bearer instruments. The [Bearer Bond](../contract/bearer_bond.md) contract gives
you profit-share staking with coverage governance. Neither guarantees that notes
will be redeemed or that bonds will pay yield or hold principal. That is not a
gap — it is the design.

The working principle is the Conder token model. During Britain's Industrial
Revolution, private merchants issued redeemable tokens. The tokens circulated not
because a smart contract enforced solvency but because the market trusted the
issuer's promise. Tokens with credible redeemability traded at par. Tokens from
unreliable issuers traded at a discount or not at all. The protocol provides the
infrastructure for redeemability — token minting, private transfer, on-chain
redemption receipts — but the actual redemption depends on the issuer's off-chain
honesty.

**"Use at your own risk" is not a disclaimer to skip past.** It is the organizing
principle of the entire token economics model. The protocol gives you lego bricks.
The market builds the cathedral — or lets it crumble.

### The Correlation Thesis

Promissory Note token prices and Bearer Bond stake coin prices move together
because both derive their value from the same thing: **the issuer's credibility**.
An attack that undermines coverage report trust also damages PN token prices
(because expected redemption value falls). An attack that undermines PN redemption
credibility also damages BB stake value (because staked capital may be
devalued PN tokens). And because 22 contracts interact with Promissory Note,
many holding each other's tokens as reserves, a single collateral failure can
cascade through the composability layer.

This document walks through how prices form (or don't), eight adversarial
scenarios with detection and mitigation strategies, the defense toolkit available
to the ecosystem, and the explicit contract between the protocol and its users.

---

## How Prices Form (or Don't)

### Promissory Note Token Pricing: The Redemption Expectation Model

There is no on-chain price discovery for PN tokens. No oracle, no exchange rate
feed, no automated market maker at the PN layer. All pricing happens OTC — buyer
and seller agree on a rate and execute an
[OtcSwapV1](../contract/promissory_note.md#otcswapv1---opcode-0x05).

In a rational market, the price of a PN token approximates:

$$ \text{Expected Price} = P(\text{redemption honored}) \times \text{Expected Redemption Value} $$

Where $P(\text{redemption honored})$ is the market's subjective probability that
the issuer will actually redeem, and $\text{Expected Redemption Value}$ is the
value of whatever the issuer promises to deliver on redemption (fiat, another
token, a real-world asset, etc.).

Several structural facts complicate this pricing:

**No supply cap.** [MintV1](../contract/promissory_note.md#mintv1---opcode-0x02)
has no `max_supply` parameter. The only gate is knowledge of the `mint_secret` —
whoever proves they know it can mint unlimited coins of that token type. There is
no on-chain mechanism to cap or audit total supply. If the `mint_secret` leaks,
unlimited minting occurs with no on-chain detection until someone does a
retrospective scan of all MintV1 nullifiers.

**Supply is computed off-chain.** Outstanding circulation for a token type is
calculated by scanning nullifier and coin events from blockchain history:
$\text{Outstanding} = \sum \text{MintV1 outputs} - \sum \text{RedeemV1 inputs}$.
There is no on-chain `total_supply` counter for PN token types (unlike
[NativeToken](../contract/native_token.md) which tracks `TOTAL_SUPPLY`).

**Redemption is optional.** [RedeemV1](../contract/promissory_note.md#redeemv1---opcode-0x01)
exists and is fully implemented — circuit, entrypoint, client builder, wallet
scanner. But per the
[Intermediary Contract Audit](../contract/promissory_note_intermediaries.md),
only the stablecoin contract calls it. For every other token type, there is
**zero on-chain redemption evidence**. The full bearer-instrument lifecycle
(issue → circulate → redeem) is broken for all non-stablecoin tokens. Token
prices cannot incorporate actual redemption execution rates because redemption
never happens on-chain for most token types.

### Bearer Bond Stake Pricing: The Coverage Confidence Model

Bearer Bond stake coins are priced based on the market's confidence in the
issuer's coverage and profit declarations:

$$ \text{Stake Price} = f(\text{coverage freshness}, \text{profit history}, \text{issuer reputation}, \text{market liquidity}) $$

The key on-chain signal is [ProveCoverageV1](../contract/bearer_bond.md#provecoveragev1---opcode-0x06):
the issuer submits `reserve_amount` and `total_outstanding`, and a ZK proof
verifies that $\text{coverage\\_ratio\\_bps} = \frac{\text{reserve\\_amount}}{\text{total\\_outstanding}} \times 10000$
computes correctly. The entrypoint enforces `reserve_amount >= total_outstanding`
and `coverage_ratio_bps >= 10000` (minimum 100% coverage).

But the ZK proof proves **arithmetic only** — it proves that the issuer's
self-reported numbers are consistent with each other. It does NOT prove:

- That `reserve_amount` actually exists on-chain or off-chain
- That `total_outstanding` matches the actual circulating supply
- That the issuer hasn't withdrawn the reserves after filing the report

Profits are [self-reported](../contract/bearer_bond.md#declareprofitsv1---opcode-0x02)
via `DeclareProfitsV1` with no ZK proof at all. The entrypoint only checks
`profit_amount > 0` and `start_block < end_block`. The trust model, per the
Bearer Bond documentation, is: "if the issuer lies, holders sell, and the stake
coin price goes to zero." This is correct but incomplete — it works **if and
only if** holders detect the lie before they exit.

### The Correlation: Why These Prices Move Together

Both instruments derive value from the same issuer's credibility:

1. **Coverage → PN price.** If coverage reports are exposed as fraudulent, the
   market revises $P(\text{redemption honored})$ downward. PN token prices fall
   because the same issuer who lied about reserves is also the counterparty for
   redemption.

2. **PN redemption → BB stake value.** If PN token prices collapse, any BB
   staking pool holding those tokens as reserves has an immediate coverage gap.
   The last ProveCoverageV1 report, filed before the collapse, is now false —
   but there is no on-chain trigger to force re-reporting.

3. **Composability cascade.** The 22 PN-interacting contracts form a dependency
   graph. If Contract A holds Contract B's PN tokens as reserves, and Contract B's
   token collapses, Contract A faces a solvency crisis. This propagates through
   the graph, with coverage reports going stale silently at each layer.

The correlation is not just theoretical — it is **mechanical through the
composability layer**. Every contract that holds PN tokens as reserves for its
own operations is exposed to the redemption credibility of every token type it
holds.

---

## Tabletop Exercise — Adversarial Scenarios

Each scenario follows a consistent structure: **Actor** (who), **Mechanism**
(which contract functions, which checks pass, which are missing), **Detection**
(how the market would detect it, if at all), **Impact** (what breaks, who loses),
and **Existing vs. Needed Mitigations** (what the protocol does now vs. what
could be added).

### 1. The Ghost Reserve Attack

**Actor:** Dishonest issuer.

**Mechanism:** The issuer creates a staking pool via IssueStakeV1. Capital is
raised from holders. The issuer files ProveCoverageV1 with `reserve_amount = X`,
`total_outstanding = Y`, where $X \ge Y$. The ZK proof verifies the arithmetic.
The entrypoint passes because `reserve_amount >= total_outstanding` and
`coverage_ratio_bps >= 10000`. But the reserves are in an off-chain wallet the
issuer controls. Nothing on-chain prevents the issuer from withdrawing the
entire reserve at any time. The coverage report remains permanently on-chain
with no expiry.

**Detection:** Compare reported `reserve_amount` against the issuer's visible
on-chain holdings. This only works if reserves are held transparently on-chain.
Off-chain reserves are invisible — the market cannot detect the withdrawal.

**Impact:** Stake coins trade at or near par (100% coverage) with zero actual
backing. New investors buy in. When the truth emerges, the price collapses
instantly — likely before most holders can exit.

**Mitigations:**

| Existing | Needed |
|----------|--------|
| Coverage ratio arithmetic enforced | Merkle proof of on-chain reserve (ProveCoverageV2 with inclusion proof against coins tree root) |
| One report per block (no spam) | Report freshness requirement: `current_height - report_block <= MAX_AGE` |
| Entrypoint checks `coverage_ratio_bps >= 10000` | On-chain reserve segregation — each pool's reserves held in a verifiable on-chain location |

### 2. The Phantom Profit Declaration

**Actor:** Dishonest issuer (or issuer disguising a ponzi).

**Mechanism:** DeclareProfitsV1 submits a fake `profit_amount` with no ZK proof.
The entrypoint only checks `profit_amount > 0` and `start_block < end_block`.
Holders call ClaimProfitsV1 and receive pro-rata shares of phantom profits. The
output coins are well-formed (BlindOutput_V1 ZK proof passes) — they just aren't
backed by real earnings. The mechanism is: new investor capital enters → gets
declared as "profit" → gets distributed to earlier holders → requires continuous
new inflow to sustain.

**Detection:** Compare declared profits against the issuer's actual observable
business activity. Hard to detect if the business is off-chain — profits could
be real revenue or recycled investor capital. On-chain detection would require
profit attestation infrastructure (not yet implemented).

**Impact:** Ponzi dynamics. As long as new capital inflow exceeds profit claims,
the system runs. When claiming outpaces inflow, it collapses. First claimants
get real value from later investors. Last claimants get nothing.

**Mitigations:**

| Existing | Needed |
|----------|--------|
| `issuer_contract` field exists on BondCoin (placeholder for future oracle) | Profit attestation: ZK proof that declared revenue corresponds to on-chain events |
| Market discipline ("if the issuer lies, holders sell") | Require DeclareProfitsV1 to reference on-chain revenue events (DEX fee logs, bridge fee records) |
| | Multi-issuer profit sign-off: require N-of-M signatures from independent parties |

### 3. The Supply Explosion

**Actor:** Malicious insider with `mint_secret` knowledge, or hacked issuer.

**Mechanism:** TokenMintV1 registers a token type with `token_auth_parent =
\text{H}(\text{mint_secret})`. Any MintV1 that proves knowledge of `mint_secret`
against this stored commitment mints new coins. There is no `max_supply`
parameter. The `mint_secret` is the only gate. If it leaks, anyone who knows it
can mint unlimited coins of that token type, and **no on-chain mechanism can
distinguish authorized mints from unauthorized ones** — the ZK proof is
identical in both cases. This would be the equivalent of someone discovering the
private key that mints USDC on Ethereum with no cap.

**Detection:** Off-chain supply scanning. Someone must count all MintV1
nullifiers for the token type and compare to the declared outstanding. Detection
is entirely retroactive — coins are already in circulation by the time someone
notices the discrepancy.

**Impact:** Token value collapses to near zero as supply dilutes. If the token
is used as reserves for a BB staking pool, the coverage ratio also collapses.
Cascades to all contracts holding the token as reserves.

**Mitigations:**

| Existing | Needed |
|----------|--------|
| Only `mint_secret` holder can mint (ZK proof of capability) | Optional `max_supply` parameter on TokenMintV1, checked at MintV1 |
| Nullifier uniqueness prevents double-spending individual coins | Mint cap per token type stored in token registry, enforced on-chain |
| | Mint_secret rotation protocol (change secret, re-register `token_auth_parent` while keeping existing coins valid) |
| | Multi-signature mint authorization (require N-of-M to mint above threshold) |

### 4. The Collateral Cascade (Composability Attack)

**Actor:** Market/systemic — not necessarily a single malicious actor.

**Mechanism:** The 22 PN-interacting contracts form a dependency graph. Each
contract holding another contract's tokens as reserves is exposed to that
token's price. If Token A collapses (scenario 1, 2, or 3 above), Contract X
holding Token A as reserves becomes under-collateralized. If Contract X also has
a BB staking pool, its coverage report (filed before the collapse) is now false.
Other contracts holding Contract X's tokens face the same problem. The cascade
propagates through the dependency graph. No on-chain trigger forces
re-reporting, so the false coverage ratios can persist indefinitely.

**Detection:** Dependency graph analysis — scan which contracts hold which
tokens, build the exposure matrix, and identify concentration risks. Prediction
markets could price this correlation risk. Currently, this is entirely off-chain
analytics.

**Impact:** Single token failure cascades through multiple contracts. Related to
the systemic risk described in the
[Risk Market Ecosystem](../contract/risk_market_ecosystem.md) documentation.

**Mitigations:**

| Existing | Needed |
|----------|--------|
| Per-contract isolation (each contract has its own state) | Circuit breakers on coverage ratio: auto-freeze if market price implies ratio below threshold |
| Token-level nullifier sets prevent double-spend within a contract | Exposure limits per contract (max percentage of reserves in any single token type) |
| | Diversification requirements enforced at the contract level |
| | Automatic coverage re-reporting triggers tied to market events |

### 5. The Stale Coverage Window

**Actor:** Issuer who reported once and never updates.

**Mechanism:** ProveCoverageV1 creates a coverage report keyed by
`(series_token_id, report_block)`. The entrypoint rejects duplicate reports for
the same block — but there is **no freshness requirement**. An issuer can:
(1) file coverage at block 1000 showing 120% coverage, (2) withdraw all reserves
at block 2000, (3) never file another report. Any investor checking at block
5000 sees only the block 1000 report. The effective coverage ratio is unknown,
but the last on-chain report says everything is fine.

**Detection:** Compare `report_block` with current block height for each series.
Off-chain watchers can track report freshness. But the protocol does not enforce
this — stale data is treated as valid data by the contract.

**Impact:** Investors rely on months-old data. The effective coverage ratio is
unknown, but the last on-chain report says everything is fine. This is a classic
stale data problem.

**Mitigations:**

| Existing | Needed |
|----------|--------|
| One report per block (prevents spam but not staleness) | `max_report_age_blocks` parameter on staking pool config |
| Entrypoint verifies ZK proof of ratio | Contracts reading coverage reports check `current_height - report.report_block <= max_age` |
| | Auto-expiry: reports older than threshold are treated as invalid by wallet resolvers |
| | On-chain report expiry mechanism: after N blocks, coverage status flips to "stale" unless renewed |

### 6. The Redemption Desert

**Actor:** The ecosystem — no one builds redemption paths.

**Mechanism:** RedeemV1 (opcode 0x01) is fully implemented at the protocol layer:
circuit, entrypoint, client builder, wallet scanner. The Redeem_V1 ZK circuit
constrains the output coin to have `value = 0` (using the `is_notequal` boolean
gate), so the receipt coin is a real ZK proof that redemption occurred. But per
the [Intermediary Contract Audit](../contract/promissory_note_intermediaries.md),
only the stablecoin (RedeemStableV1, opcode 0x0A) calls RedeemV1. Every other PN
token type has zero on-chain redemption evidence. Tokens can be minted,
transferred, burned (via TransferV1 to a contract, which is "simulated burn"),
and OTC swapped — but never actually redeemed through the protocol's redemption
path.

**Detection:** Scan blockchain history for RedeemV1 calls per `token_id`. The
gap is immediately visible off-chain, but the contract does not track this.

**Impact:** The full bearer-instrument lifecycle (issue → circulate → redeem) is
broken for all non-stablecoin tokens. The "promise" in promissory note is purely
off-chain. Token prices cannot incorporate redemption success rates because
redemption never happens on-chain for most token types.

**Mitigations:**

| Existing | Needed |
|----------|--------|
| RedeemV1 is fully implemented and available | More contracts need to build RedeemV1 paths. The stablecoin pattern works and is reusable. |
| Redeem_V1 ZK circuit correctly constrains zero-value receipt coin | Documentation should emphasize: TokenMintV1 without a corresponding RedeemV1 path creates a token that can never be formally redeemed on-chain. |
| | Wallet resolvers could flag tokens with zero RedeemV1 usage as "unproven redemption path" |

### 7. The Maturity Arbitrage

**Actor:** Malicious holder attempting early unstaking.

Maturity is enforced at the entrypoint level — not just the wallet.
The `unstake_v1` entrypoint at
`src/contract/bearer_bond/src/entrypoint/mod.rs:878` enforces maturity
on-chain:
```rust
if params.current_block < stake_coin.maturity_block {
    return Err(BearerBondError::StakeNotMatured { ... }.into());
}
```

**Impact:** Resolved. On-chain maturity enforcement prevents early unstaking
regardless of how the transaction is constructed (wallet or manual).

**Mitigations:**

| Existing | Status |
|----------|--------|
| Wallet-level capability check (CAP_UNSTAKE_RIGHT derivation) | Defense in depth |
| Entrypoint-level `current_block >= maturity_block` check | ✅ IMPLEMENTED (July 2026) |

### 8. The Double-Count Collateral

**Actor:** Issuer operating multiple pools or token types.

**Mechanism:** An issuer operates two PN token types (A and B) with two BB
staking pools (X and Y). They hold reserves in a single off-chain wallet.
ProveCoverageV1 for pool X reports `reserve = Z`. ProveCoverageV1 for pool Y
reports `reserve = Z`. Both reports pass — the ZK proof verifies arithmetic on
each report individually. Nothing on-chain proves that Z is not double-counted.
The total reported reserves ($2Z$) exceed the actual reserves ($Z$), creating
fractional coverage in aggregate while each pool individually appears fully
covered.

**Detection:** Aggregate all coverage reports for a known issuer's public key.
Compare total reported reserves to the issuer's known on-chain holdings.
Off-chain reserves make this nearly impossible to detect.

**Impact:** Each pool individually looks 100% covered. In aggregate, they are
fractionally reserved. This is particularly dangerous when one issuer operates
multiple pools that appear independent but share the same off-chain reserves.

**Mitigations:**

| Existing | Needed |
|----------|--------|
| Per-pool coverage reports with ZK arithmetic proof | On-chain reserve segregation — each pool's reserves held in a specific on-chain address |
| | Global reserve registry: each ProveCoverageV1 includes a Merkle proof that the reported reserves are held at a specific coins tree root |
| | Cross-pool coverage audit: check that sum(reported reserves across issuer's pools) does not exceed issuer's total on-chain holdings |

---

## Defense Strategies — How the Ecosystem Fights Back

### Market Discipline (The Primary Defense Today)

The Conder token model worked because reputation was everything. A merchant whose
tokens went unredeemed lost the ability to issue new tokens — the market
remembered. In DarkWow, the same dynamic applies: bad actors lose reputation,
their stake coins become illiquid, their business dies.

The mechanism:
- Holders monitor coverage reports and profit declarations for anomalies
- When dishonesty is detected, holders sell → price drops → new investors stay away
- The issuer's future capital formation becomes impossible

**Limitation:** Market discipline works after the fact. It punishes bad behavior
but does not prevent it. And it relies entirely on detection — if no one notices
the fraud, the market doesn't discipline it. In a privacy-preserving system where
values and ownership are ZK-hidden, detection is inherently harder than in a
transparent blockchain.

### On-Chain Enhancements (What Can Be Added)

These are protocol-level upgrades the ecosystem could adopt to harden the system
without requiring trust in a central regulator:

- **Coverage freshness requirements:** `max_report_age_blocks` parameter on
  staking pool config. Reports older than threshold are treated as stale by the
  wallet and by contracts that read coverage data.
- **Merkle proof of on-chain reserve:** ProveCoverageV2 could include a Merkle
  inclusion proof against the coins tree root, proving the reported
  `reserve_amount` actually exists at a specific on-chain location.
- **Supply cap enforcement:** Optional `max_supply` parameter on TokenMintV1,
  checked cumulatively across all MintV1 calls for that token type.
- **Profit attestation:** Require DeclareProfitsV1 to reference on-chain revenue
  events (DEX fee records, bridge fee logs) with ZK proofs of revenue inclusion.
- **Maturity enforcement at entrypoint level:** Add `require(current_height >=
  coin.maturity_block)` to `unstake_v1` — the simplest fix in this document,
  since the data field already exists on the coin struct.

### Ecosystem-Level Defenses

- **Watchtowers:** Automated off-chain scanners that compare coverage report data
  against on-chain state. Flag stale reports, reserve-withdrawal events, and
  coverage ratio anomalies. Run by relayers, block explorers, or independent
  analysts.
- **Reputation registries:** Identity-attested issuer histories. An issuer's
  track record of profit declarations, coverage filings, and redemption events
  becomes a public good — verifiable on-chain through identity contract
  attestations.
- **Prediction markets:** Bet on coverage report validity. If the market prices
  a coverage report at 80% credibility, that price signal feeds back into stake
  coin pricing. Described in [Risk Market Ecosystem](../contract/risk_market_ecosystem.md).
- **Insurance coverage:** Underwriters cover PN/BB default risk, priced
  according to the issuer's observable track record. Integrated with the
  [Insurance Market](../contract/insurance_market.md) contract.

### The User's Toolkit

What an individual holder can do today, with existing infrastructure:

1. **Verify coverage freshness before buying.** Check the block height of the
   latest ProveCoverageV1 for the series. If it's older than you're comfortable
   with, treat the coverage ratio as unknown.
2. **Check RedeemV1 usage for the token.** If a token type has zero RedeemV1
   calls in its entire history, the full bearer-instrument lifecycle has never
   been exercised for that token. The "promise" is untested.
3. **Cross-check total supply.** Scan MintV1 events for the token type and
   compare the total minted to what the issuer claims as `total_outstanding` in
   their coverage reports.
4. **Diversify across issuers.** No single issuer should be "too big to fail"
   in your portfolio. The composability cascade (Section C.4) means correlated
   exposures amplify risk.
5. **Prefer issuers with on-chain verifiable reserves.** If an issuer can't
   point to specific on-chain coins as proof of reserves, the reserves are
   unverifiable by construction.
6. **Use OTC price discovery.** Price discovery happens through the OTC swap
   market, not through on-chain oracles. Get multiple quotes. If a token trades
   at a persistent discount to its stated redemption value, the market is pricing
   in default risk — take that signal seriously.

---

## "Use at Your Own Risk" — The Contract Between Protocol and User

### What the Protocol Guarantees (ZK-Verified Properties)

These are the properties enforced by ZK circuits and entrypoint logic. They hold
regardless of issuer behavior:

| Property | Mechanism |
|----------|-----------|
| No double-spend | Nullifier uniqueness enforced by SMT-backed nullifier set |
| Value conservation on transfers | Pedersen homomorphic commitment sums: $\sum\text{input} = \sum\text{output}$ per token type |
| Well-formed coins | BlindOutput_V1 ZK proof verifies coin commitment is correctly constructed |
| Only mint_secret holder can mint | ZK proof compares `mint_public` against stored `token_auth_parent` |
| Redemption receipt is real | Redeem_V1 circuit constrains output coin to `value = 0` via boolean gate |
| Coverage ratio arithmetic | `base_div` ZK proof verifies $\text{ratio} = \frac{\text{reserve}}{\text{outstanding}} \times 10000$ |
| No duplicate coverage reports | One report per `(series_token_id, report_block)` enforced by entrypoint |

### What the Protocol Does NOT Guarantee (Trust-Assumed Properties)

These properties are assumed by the market but not enforced by the protocol.
They depend entirely on issuer honesty:

| Property | Why It's Not Guaranteed |
|----------|------------------------|
| Reserves actually exist | ProveCoverageV1 verifies arithmetic on self-reported numbers. Off-chain reserves are invisible. |
| Profits correspond to real revenue | DeclareProfitsV1 has no ZK proof. Issuer self-reports. |
| Coverage is current | No report freshness requirement. A report filed at block 1000 is treated as valid at block 50000. |
| Anyone will redeem your notes | RedeemV1 exists but is unused by most token types. Redemption is purely voluntary. |
| Stake coins will pay yield or hold principal | Profits can be zero. Coverage can be fraudulent. |
| Maturity locks prevent early unstaking | Enforced on-chain at entrypoint level (`entrypoint/mod.rs:878`) | N/A (protocol-enforced) |
| Supply doesn't explode | No supply cap at MintV1. `mint_secret` compromise = unlimited minting. |

### Summary Table

| Property | Protocol Guarantee | Market Must Trust |
|----------|--------------------|-------------------|
| Coin not double-spent | ZK nullifier check | — |
| Transfer value conserved | Pedersen homomorphic sum | — |
| Coverage math correct | `base_div` ZK proof | — |
| Reserves actually exist | **None** | Issuer's word + off-chain audit |
| Profits actually earned | **None** | Issuer's word + business inspection |
| Coverage is current | **None** (no expiry) | Tracking `report_block` vs current height |
| Tokens can be redeemed | **None** (RedeemV1 unused by most) | Issuer's promise to honor redemption |
| Maturity lock enforced | **None** (wallet-level only) | Trusting holders not to bypass wallet |
| Supply won't explode | **None** (no supply cap) | `mint_secret` is not compromised |

---

## Conclusion — The Market as Arbiter

The protocol cannot save you from bad counterparties. That is not a bug — it is
the Conder token model, where the bearer instrument is infrastructure and the
market is the regulator.

DarkWow gives you the tools to verify what CAN be verified:
- That a transfer didn't inflate the money supply
- That a coin wasn't double-spent
- That a coverage report's arithmetic is internally consistent
- That a redemption receipt is a real ZK proof of redemption

For everything else — whether the issuer actually has the reserves they claim,
whether the profits they declare are real, whether they'll honor redemption
when you ask — the market decides. The Conder tokens worked because reputation
was the only capital that mattered. A dishonest issuer could fool the market
once. They couldn't do it twice.

Let a thousand tokens bloom — and let a thousand tokens fail. The protocol
provides the lego bricks. Whether the cathedral stands is up to the builders.

---

## Why This Risk Model Inverts the Industry

The fee signalling system (see [fee-spec.md §12.12.6](consensus/fee-spec.md))
is the first major case study from genesis demonstrating why DarkWow's o-cap
architecture exists. It inverts a structural problem that token-weighted
governance cannot solve.

### The Problem: Governance Tokens Incentivize Risk Extraction

In token-weighted governance systems, whales control the parameters. They set
gas costs, fee structures, and execution limits. They are structurally
incentivized to push risk onto users because they profit from user extraction
while bearing none of the downside. Users bid gas in plaintext auctions,
overpay to ensure inclusion, and pay for execution whether it succeeds or
fails. Fee/gas patterns are visible to everyone, enabling MEV and traffic
analysis. The governance token is the control surface; the whale is the
controller; the user is the victim.

### The Inversion: O-Cap Primitives Enforce Risk Distribution

DarkWow's genesis block contains specific o-cap primitives — not because they
are general-purpose infrastructure, but because they are the necessary and
sufficient components for decentralized self-governance without token voting.
Each genesis contract has a specific role in the risk architecture:

- **`native_token`**: Fee payment is Pedersen-committed — private, bounded to
  threshold. No traffic analysis of fee patterns is possible.

- **`manifest`**: Cost profiles are self-declared by deployers and
  cryptographically bound to contracts. The deployer stakes reputation on
  accuracy.

- **`identity` + `attestation`**: Third parties vouch for contract safety,
  lowering the risk factor applied to that contract's fees.

- **`endowment` + `escrow`**: Deployers underwrite their cost declarations
  with slashable economic stake. A contract that lies about its costs can
  lose its endowment.

- **`deployooor`**: Contract deployment binds the manifest to the contract
  at birth — the cost declaration is inseparable from the contract itself.

- **Fee window system**: Miners track observed-vs-declared cost accuracy
  across windows. Contracts that systematically under-declare see their risk
  factor rise until they are priced out of the mempool.

The adjustment is mechanical, not political. Miners observe, risk factors
adjust, deployers respond. No governance token, no DAO, no whale vote. The
architecture itself enforces the risk distribution.

### What This Means for Users

Users pay a threshold fee and either get included or they don't. They cannot
fat-finger away their native token. They cannot be front-run through fee
analysis — their fee is hidden behind a Pedersen commitment. They do not pay
for failed or resource-exhausting execution. Infrastructure builders and
deployers absorb that risk. Users are protected by the architecture, not by
the goodwill of token holders.

### What This Means for Deployers

Deploying a contract is not free. The deployer must declare costs accurately,
have the contract attested, and potentially stake capital in endowment/escrow
contracts to underwrite the risk. A contract with a 2.0× risk factor (unknown,
unattested) pays twice the base fee — the market prices the risk. A contract
that causes block exhaustion gets blacklisted. The burden of proof rests on
the deployer, not the user.

This is caveat emptor applied to infrastructure: the market decides which
contracts are trustworthy. The protocol provides the lego bricks — manifest
declarations, attestation pathways, slashing mechanisms — but does not
guarantee that any particular contract is safe. Builders who prove their
contracts are safe earn lower risk factors. Builders who don't, pay the price.

### Why Upstream Cannot Do This

Token-weighted governance cannot achieve this inversion because the same
whales who set gas parameters also profit from user extraction. There is no
structural separation between the governors and the governed. DarkWow's o-cap
primitives provide that separation: miners enforce risk mechanically, deployers
stake reputation and capital, users are protected by the architecture. No
vote can change it.

The fee model is the case study that proves this architecture works — straight
from genesis. It is not a future roadmap item. It is the first demonstration
that decentralized self-governance through o-cap primitives is operational,
not theoretical.

---

## References

- George Selgin, *Good Money: Birmingham Button Makers, the Royal Mint, and
  the Beginnings of Modern Coinage* (University of Michigan Press, 2008)
- [Promissory Note Contract](../contract/promissory_note.md)
- [Bearer Bond Contract](../contract/bearer_bond.md)
- [Intermediary Contract Audit](../contract/promissory_note_intermediaries.md)
- [Consensus & Coinbase](consensus-coinbase.md)
- [Risk Market Ecosystem](../contract/risk_market_ecosystem.md)
- [Slashing & Economic Security](slashing.md)
- [Relayer Economics](../relayer/relayer_economics.md)
- [Composability](../contract/composability.md)
- [Safety Patterns](../dev/contracts/safety.md)
