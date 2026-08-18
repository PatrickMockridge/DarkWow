# Bridge Core — Developer Guide

> **Developer integration guide.** For the contract specification, see [Bridge Core](../../contract/bridge.md).

bridge-core is a thin, object-capability-native issuer that locks an external-chain asset and
issues a wrapped promissory note 1:1 against it. Relayer registry, coverage/slashing, governance,
and atomic-swap coordination are **separate legos** — compose them, don't re-monolithize.

## What bridge-core does (and doesn't)

**Does:**

- `DepositV1` — verify an external deposit, issue a wrapped PN via a child `PN::IssueV1`.
- `WithdrawV1` — redeem a wrapped PN via a child `PN::RedeemV1`, record the external-release signal.

**Does NOT:**

- Register relayers, accept/reassign/cancel withdrawals, or track reputation → `relayer_endowment`.
- Cover or slash → `pool_stake`.
- Govern or report → `dao_escrow`.
- Coordinate cross-chain HTLC atomic swaps → compose deposit/withdraw with external-chain HTLC.

## Building

```bash
make -C src/contract/bridge all      # WASM + circuits
make -C src/contract/bridge proof    # compile deposit.zk / withdraw.zk
```

## Child-call construction

bridge-core requires one promissory-note child call per endpoint:

| Opcode | Child call | Builder (PN harness) |
|--------|-----------|----------------------|
| `DepositV1` (0x01) | `PN::IssueV1` (0x02) | `PromissoryNoteHarness::issue` |
| `WithdrawV1` (0x02) | `PN::RedeemV1` (0x01) | `PromissoryNoteHarness::redeem` |

The wrapped token id is deterministic (see spec §3). Reproduce it client-side with the same
derivation so the bridge's `derive_wrapped_token_id` check passes.

## Composition recipe

```
deposit:    [Bridge::DepositV1, PN::IssueV1(child)]      → user holds wrapped PN
withdraw:   [Bridge::WithdrawV1, PN::RedeemV1(child)]   → withdrawals[nullifier]
relayer:    relayer_endowment::RegisterRelayerV1 (0x08)  → then watches withdrawals
coverage:   pool_stake::AllocateCoverageV1 / SlashCoverageV1
governance: dao_escrow::SetGovernanceConfigV1
```

## Feature gates

- `bridge-verify` — external-chain cryptographic verification (default off; structural checks only).
- `deterministic-zk` — deterministic proof generation for tests.
- `relayer` (on `relayer_endowment`) — enables the relayer-registry submodule.
