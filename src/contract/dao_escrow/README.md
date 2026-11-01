# DAO Escrow

One endowment pool, governed by one MultiSig group. The canonical specification is
[`doc/src/contract/dao_escrow.md`](../../../doc/src/contract/dao_escrow.md); the manifest is
[`manifest.toml`](manifest.toml) and the circuits are under [`proof/`](proof/).

The owner installs a MultiSig group with `UpdateV1`; from then on every spend is authorised by
the group's `multisig::FinalizeV1` over a message naming the action. The group's threshold
**is** the quorum — this contract re-counts nothing, so there is no second source of truth that
could disagree with the first. The claim lifecycle is `ProposeClaimV1` (0x07) →
`VoteClaimV1` (0x08) → `ExecuteClaimV1` (0x09), with `CancelClaimV1` (0x0d) alongside.
Membership notes are issued by `PayPremiumV1` (0x02); members do not vote.

`EnableDrainProtectionV1` (0x06) and the capability-requirement/config endpoints (0x0a–0x0c,
0x0e–0x10) were retired and are not in the enum.

Tests: `tests/`; heavyweight control: `test_heavyweight_dao_escrow`.
