# Deployooor

## Abstract

This contract enables deployment of custom smart contracts on chain.
A user deploys their custom `WASM` bincode via `DeployV1`, optionally with the
singleton rule, and can then `LockV1` the contract so its code is final and
can't be modified further.

There is **no update function**: `Deployooor` exposes exactly `DeployV1` (0x00) and
`LockV1` (0x01). A `DeployUpdateV1` struct exists in `model.rs` but no dispatch path
applies it, so "updating deployed code" is not a capability this contract provides —
the only path from deployed to changed is a re-deployment under a different
`ContractId`.

**Note**: The `dwow_wallet` wallet integrates with Deployooor via `apply_tx_deploy_data()` for scanning deployments and `deploy_contract()` for creating new deployments. Contract deployment requires fee payment infrastructure (NativeToken::FeeV3 integration).

- [Concepts](concepts.md)
- [Scheme](scheme.md)

