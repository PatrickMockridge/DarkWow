# Risk & Governance Specification

This document is the normative specification of DarkWow's risk and governance model. It is the
**hub** that consolidates — in one place — the risk/governance architecture that is otherwise
specified across the fee, manifest, genesis, o-cap, and philosophy documents. Those documents are
the spokes; this document states the invariants they jointly define and points to each.

The key words "MUST", "MUST NOT", "SHALL", "SHALL NOT", "SHOULD", and "MAY" in this document are to
be interpreted as described in RFC 2119.

## 1. Scope

DarkWow partitions on-chain computation into two domains with different authority requirements:

- **Consensus-critical operations** — enforced as block-validity rules at every `accept_block`.
- **Risk and governance operations** — expressed as *views* that nodes form by observation, priced
  into admission, and never enforced as block-validity rules.

This document specifies the boundary between the two, and the risk/governance model that operates on
the non-consensus side.

## 2. The Consensus-Critical Boundary

**RG-1.** The **only** consensus-critical proof SHALL be the per-block **Pedersen mass balance** of
the native token. This comprises two meters, both in `[domain: mass_balance]`:

- **Supply audit** — the cumulative supply commitment chain `S_H = S_{H-1} + C_H`, verified by
  additive Pedersen homomorphism without ZK proofs. See [Consensus: Supply
  Audit](consensus/consensus.md) and [Genesis](genesis.md).
- **Fee totalization** — the `FeeCollectV1` plaintext `fees_db[height]` accumulator: each fee call
  adds its plaintext fee to the per-height pot, and `FeeCollectV1` claims the sum. See [Fee
  Specification](consensus/fee-spec.md) FI-COLLECT-1..5.

**RG-2.** Every other signal — contract execution risk, `BlockCharge` accuracy, attestation,
functionality verification, governance — SHALL be a non-consensus *view* priced into admission, and
SHALL NOT be a block-validity rule. A misconfigured view degrades UX or pricing; it cannot create or
destroy money.

## 3. Domain Separation

The split is `[domain: mass_balance]` (consensus-critical) versus `[domain: fee_signalling]`
(non-consensus coordination), defined in [Fee Specification §0](consensus/fee-spec.md). The
genesis block makes it concrete: only **Deployooor** and **NativeToken** are consensus-critical; the
remaining seven contracts are ecosystem infrastructure that play no role in block validation
([Genesis](genesis.md)).

## 4. The Risk Factor

**RG-3.** A contract's risk factor SHALL be a per-contract, **dynamic, observed** value — a *view*
nodes form by comparing the declared `BlockCharge` against *observed* execution cost, updated by the
`ContractRiskTracker` at fee-window boundaries. It is NOT a static attestation classification. The
`BlockCharge` is a declarative *nameplate*, not gas ([Fee Specification
§12.4.5](consensus/fee-spec.md)); a deployer who under-declares earns a higher risk factor.

**RG-4.** The risk factor SHALL feed the admission fee via `compute_fee_v3()`, which implements the
multiplicative FeeV3 formula `fee = gas × CF × tier × risk` ([Fee
Specification](consensus/fee-spec.md) §12.4.1). `risk` is the `ContractRiskTracker` factor
(1.0× → 2.0×, FI-RISK-1).

**RG-5.** The risk factor SHALL be the dynamic value stored per `contract_id` in the `contract_risk`
sled tree by `ContractRiskTracker`:

| Observation | Risk factor |
|---|---|
| Accurate declaration (observed ≈ declared) | 1.0× (baseline) |
| Sustained under-declaration | escalates toward 2.0× (cap) |
| Sustained accuracy | de-escalates toward 1.0× |

Risk factors SHALL be per-contract, chain-state values with no global classification table; a contract
with no entry SHALL be assigned baseline (1.0×), and any node SHALL be able to read a contract's risk
factor and derive the same value the miner uses ([Fee
Specification](consensus/fee-spec.md) FI-RISK-3, FI-RISK-4, FI-RISK-5).

## 5. The Risk Architecture

The risk model SHALL invert the gas model's risk placement ([Fee Specification
§12.12.6](consensus/fee-spec.md)):

- **RG-6.** Users SHALL have bounded, predictable risk: a plaintext fee at a chosen three-tier
  priority (low/medium/high), never increased on failed or resource-exhausting execution.
- **RG-7.** Deployers SHALL bear the burden of proving `BlockCharge` accuracy — by attestation and
  slashable endowment — or be priced out of the mempool as nodes raise the risk factor.

The adjustment SHALL be mechanical (nodes observe, risk factors adjust, deployers respond); no
token-weighted governance SHALL be required to decide who bears risk.

## 6. Governance Primitives

**RG-8.** Governance SHALL be composed from the genesis o-cap primitives, not a monolithic DAO. The
primitives and their risk/governance roles are:

| Primitive | Role |
|---|---|
| `native_token` | The one consensus-critical meter — Pedersen supply audit + plaintext fee totalization |
| `deployooor` | Binds the manifest to the contract at birth |
| `manifest` | Self-declared cost profiles — deployer stakes reputation on accuracy |
| `identity` + `attestation` | Vouching — third parties verify safety, lowering the risk factor |
| `oracle` | Feeds — price, randomness, attestation data (informational, non-consensus) |
| `purse` + `box` | Economic underwriting + capability delegation (slashable stake, linear consumption) |
| `multisig` | Threshold governance with zero-knowledge ballots — no token weighting |

See [Genesis](genesis.md), [Philosophy](../philosophy/philosophy.md), and [Differences from
Upstream](../about/differences_from_upstream.md).

## 7. The ZK Paradigm

The risk/governance model is inseparable from DarkWow's ZK o-cap model ([O-Cap](ocap.md),
[Type System](type-system.md)):

- A **capability** is a name whose possession is the authority to act; in **ZK mode** the capability
  *is* a secret whose knowledge is proven in zero-knowledge, and the type is the ZK circuit that
  verifies the predicate ([O-Cap](ocap.md) §5).
- **Barbs** are the observable actions of a type: `SecretKey` `↓spend`/`↓derive`, `PublicKey`
  `↓verify`/`↓encrypt`, `Nullifier` `↓nullify`, `Commitment` `↓commit`, `ContractId` `↓dispatch`,
  `FuncId` `↓gate`, `AssetId` `↓denominate`, `MerkleNode` `↓prove-inclusion`.
- **Fees are capabilities, not gas**: a three-tier priority selector (low/medium/high) is a capability
  — the user's choice of tier grants admission priority ([Fee Specification §12.5](consensus/fee-spec.md)).
  Because the fee is plaintext and risk is a soft view, users converge on three uniform tiers — no
  idiosyncratic gas-price traffic analysis is possible.

## 8. Hub — Spokes

This document consolidates; the following are the authoritative specifications it points to:

- **Consensus & the hardwired proof** — [Consensus: Supply Audit](consensus/consensus.md),
  [Fee Specification](consensus/fee-spec.md), [Consensus Safety](consensus/safety.md).
- **Genesis primitives** — [Genesis](genesis.md).
- **Manifest & attestation** — [Contract Manifest](manifest.md), [Contract Trust
  Model](contract-trust-model.md).
- **Capabilities & privacy** — [O-Cap](ocap.md), [Type System](type-system.md),
  [Privacy Model](privacy.md), [Anonymous Assets](anonymous_assets.md).
- **Economics & philosophy** — [Caveat Emptor](economics-caveat-emptor.md),
  [Philosophy](../philosophy/philosophy.md), [Differences from Upstream](../about/differences_from_upstream.md).
