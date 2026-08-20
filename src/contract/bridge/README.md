# DarkWow Bridge Core

> **USE AT YOUR OWN RISK.** Cross-chain bridges carry inherent risk of loss. This contract has
> not been independently audited.

`bridge-core` is a thin, object-capability-native contract that locks an external-chain asset and
issues a wrapped [promissory note](../promissory_note/README.md) 1:1 against it. The user owns the
wrapped PN (their capability) at all times — no custodian, no VSS, no threshold.

Relayer registry, coverage/slashing, governance, and atomic-swap coordination are **separate
legos**. See [the composition spec](../../../doc/src/contract/bridge.md) for how they compose.

## Functions

| Opcode | Function | Purpose |
|--------|----------|---------|
| `0x00` | `InitializeV1` | Init trees; store PN contract id |
| `0x01` | `DepositV1` | Verify external deposit → issue wrapped PN (child `PN::IssueV1`) |
| `0x02` | `WithdrawV1` | Redeem wrapped PN (child `PN::RedeemV1`) → record external-release signal |

## How it works

**Deposit** — `[Bridge::DepositV1, PN::IssueV1 (child)]`. The bridge verifies the external-chain
deposit (feature-gated `bridge-verify`), enforces anti-double-claim (`deposits` + `chain_events`),
and validates the child `IssueV1` (`spend_hook == bridge`, deterministic `asset_id`). The wrapped
PN lands in PN's coin tree — the single source of truth.

**Withdraw** — `[Bridge::WithdrawV1, PN::RedeemV1 (child)]`. The user burns the wrapped PN
(zero-value receipt routed through the bridge via `spend_hook`), the bridge enforces
anti-double-spend, and records `withdrawals[nullifier]` as the external-release signal the relayer
watches.

## Mint authority

Deterministic and public — no custodian:

```
issue_secret      = H(bridge_cid, chain, "brid")
token_auth_parent = H(7, issue_secret)
token_blind       = H(chain, "blnd")
asset_id          = H(2, token_auth_parent, 0, token_blind)
```

1:1 backing is enforced by the bridge (external proof + anti-double-claim), not by secret custody.

## Lego composition

| Lego | Concern |
|------|---------|
| `promissory_note` | bearer instrument (issue/transfer/redeem) |
| `relayer_endowment` | relayer registry (register/reputation/fee-schedule) |
| `pool_stake` | coverage + slashing |
| `dao_escrow` | DAO governance |
| `otc_swap` | DarkWow-internal OTC swaps |

## Building

```bash
make            # build WASM + compile circuits
make proof      # compile deposit.zk / withdraw.zk
cargo test      # integration tests
```

## Feature gates

- `bridge-verify` — external-chain cryptographic verification (default off; structural checks only).
- `deterministic-zk` — deterministic proof generation for tests.

## Security model

A wrapped PN is a bearer instrument: holding it is the capability to redeem. Its value is the
proof that backs it — reputation, endowment, attestation, governance-verified backing — mapped to
ZK object-capabilities. The bridge never sees the user's secret; it only verifies proofs.
