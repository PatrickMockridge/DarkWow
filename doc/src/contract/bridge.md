# Bridge Core — Composable Cross-Chain Wrapping

> **USE AT YOUR OWN RISK.** Cross-chain bridges carry inherent risk of loss. This
> contract has not been independently audited.

The DarkWow bridge is no longer a monolithic contract. It is a thin, object-capability-native
**bridge-core** that only locks an external-chain asset and issues a wrapped promissory note
against it 1:1. Every surrounding concern — relayer registry, coverage/slashing, governance,
atomic-swap coordination — lives in separate composable contracts ("legos").

---

## 1. The Idea

A promissory note is only as good as what backs it **and the proof thereof** — exactly like
paper money. bridge-core's job is narrow:

1. **Bridge** an external-chain asset into DarkWow as a wrapped PN.
2. **Lock** the external asset (deposited to the bridge's deterministic external address).
3. **Issue** the wrapped PN 1:1 against that locked asset.

The user owns the wrapped PN (their capability) at all times. There is no custodian, no
threshold, no VSS. Backing is enforced by proof — external-deposit verification plus
anti-double-claim — not by secret custody.

## 2. Function Surface

| Opcode | Function | Purpose |
|--------|----------|---------|
| `0x00` | `InitializeV1` | Init trees; store PN contract id |
| `0x01` | `DepositV1` | Verify external deposit → issue wrapped PN (child `PN::IssueV1`) |
| `0x02` | `WithdrawV1` | Redeem wrapped PN (child `PN::RedeemV1`) → record external-release signal |

### 2.1 Deposit (`0x01`)

Transaction = `[Bridge::DepositV1, PN::IssueV1 (child)]`.

1. Verify the external-chain deposit proof (`verify_chain_proof`, feature-gated `bridge-verify`).
2. Anti-double-claim on the deposit commitment and the external event (`chain_events`).
3. Validate the child `IssueV1`: `spend_hook == bridge`, `token_id == derive_wrapped_token_id(cid, chain)`.
4. Record `deposits[commitment]`. The wrapped PN is minted by the child `IssueV1` into PN's coin
   tree — the single source of truth. There is no bridge-side deposit Merkle tree.

### 2.2 Withdraw (`0x02`)

Transaction = `[Bridge::WithdrawV1, PN::RedeemV1 (child)]`.

1. The user burns the wrapped PN via child `RedeemV1` (zero-value receipt; `spend_hook` routes
   the burn through the bridge).
2. Anti-double-spend on the nullifier.
3. Record `withdrawals[nullifier]` as the external-release signal.

The relayer watches `withdrawals` and executes the release on the external chain. There is no
on-chain claim/accept/reassign/cancel machinery — those were over-engineered coordination and
were removed.

## 3. Mint Authority (Deterministic, Public)

The wrapped token's mint authority is a **deterministic public secret** — no custodian:

```
issue_secret     = H(bridge_cid, chain, "brid")
token_auth_parent = H(7, issue_secret)                      # matches PN IssueV2 issue_public
token_blind      = H(chain, "blnd")
token_id         = H(2, token_auth_parent, 0, token_blind)  # matches PN RegisterTypeV2
```

Anyone can derive it; 1:1 backing is enforced by the bridge (external proof + anti-double-claim),
not by secrecy. An unbacked wrapped PN can only be minted with a matching locked deposit, and it
carries `spend_hook = bridge`, so it cannot be redeemed through the bridge without that backing.

## 4. Trees

| Tree | Purpose |
|------|---------|
| `info` | PN contract id, state |
| `deposits` | deposit commitment → record (anti-double-claim) |
| `withdrawals` | nullifier → external-release signal |
| `nullifiers` | spent nullifiers (anti-double-spend) |
| `chain_events` | external-event uniqueness (anti-double-deposit) |

## 5. Circuits

| Circuit | Proves |
|---------|--------|
| `deposit.zk` | depositor's commitment binds `(secret, amount, bridge_address)` |
| `withdraw.zk` | withdrawal nullifier binds `(secret, recipient_hash)` |

No Sinsemilla deposit-tree membership proof — the PN coin tree is the wrapped-asset ledger.

## 6. Lego Inventory

bridge-core composes with existing contracts rather than re-implementing their concerns:

| Lego | Concern | Interaction |
|------|---------|-------------|
| `promissory_note` | bearer instrument (issue/transfer/redeem) | child `IssueV1`/`RedeemV1`, `spend_hook = bridge` |
| `relayer_endowment` | relayer registry (register/reputation/fee-schedule) | relayer registers before executing withdrawals |
| `pool_stake` | coverage + slashing (`AllocateCoverageV1`/`SlashCoverageV1`) | relayer covers a withdrawal, slashed on failure |
| `dao_escrow` | DAO governance (config, treasury) | governs the *ecosystem*, not bridge operational config |
| `otc_swap` | DarkWow-internal OTC swaps | unrelated to cross-chain atomic swaps |

## 7. Composition Recipes

### 7.1 Full lifecycle

```
deposit:    external chain → bridge-core::DepositV1 (+ child PN::IssueV1) → user holds wrapped PN
withdraw:   user → bridge-core::WithdrawV1 (+ child PN::RedeemV1) → withdrawals[nullifier]
relayer:    relayer_endowment::RegisterRelayerV1 → watches withdrawals → executes on external chain
coverage:   pool_stake::AllocateCoverageV1 → relayer covers the withdrawal; slash on failure
governance: dao_escrow::SetGovernanceConfigV1 → DAO governs the ecosystem, not bridge ops
```

### 7.2 Cross-chain atomic swap (composition, not a contract)

The old `CreateHtlcV1`/`ClaimHtlcV1`/`RefundHtlcV1` orchestration is gone. A cross-chain atomic
swap is now: bridge-core deposit/withdraw on the DarkWow side + the external chain's native HTLC
on the other side. No single contract coordinates it.

## 8. Trust Model

A wrapped PN is a bearer instrument: holding it is the capability to redeem. Its value is exactly
the proof that backs it — reputation, endowment, attestation, and governance-verified backing —
mapped to ZK object-capabilities. The bridge never sees the user's secret; it only verifies
proofs.
