# Deployooor

## Abstract

This contract enables deployment and management of custom smart
contracts on chain. Users can create an authority to control the smart
contract and deploy their custom `WASM` bincodes. Additionally, they
can update their code or lock the contract so its code is final and
can't be modified further.

**Note**: The `dwow_wallet` wallet integrates with Deployooor via `apply_tx_deploy_data()` for scanning deployments and `deploy_contract()` for creating new deployments. Contract deployment requires fee payment infrastructure (NativeToken::FeeV1 integration).

- [Concepts](concepts.md)
- [Scheme](scheme.md)

