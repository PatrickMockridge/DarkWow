# Bearer Bond

Fixed-interest staking instruments. The canonical specification is
[`doc/src/contract/bearer_bond.md`](../../../doc/src/contract/bearer_bond.md); the manifest is
[`manifest.toml`](manifest.toml) and the circuits are under [`proof/`](proof/).

**Status (2026-10-25): the issuer-authority arms are refused.** `IssueStakeV1` (0x00),
`RequestInterestV1` (0x02) and `ProveCoverageV1` (0x06) return
`AuthorityModelUnavailable` before any host call or state read. A series stores an
`issuer_contract`, but those arms took it from the caller's params and compared it with the
stored value — anyone who reads the public series record could pass it, so issuance and
coverage reports were unauthenticated (`OBL-C248`, `OBL-C275`, `OBL-C276`). `TransferStakeV1`,
`EmergencyUnstakeV1`, `UnstakeV1`, `BurnStakeV1`, `VerifyCoverageV1`, `PayInterestV1` and
`RegisterSeriesV1` are reachable. A key-based or contract-composition authority model is the
redesign.

Tests: `tests/`; heavyweight control: `test_heavyweight_bearer_bond`.
