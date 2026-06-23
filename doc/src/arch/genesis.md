# Genesis Contracts

Nine contracts are deployed at genesis, each at a deterministic ContractId. This
page is the **single source of truth** for the genesis contract set. Every other
document that references genesis contracts links here rather than repeating the list.

## Contract List

| Counter | Name | Crate | Consensus | Role |
|---------|------|-------|-----------|------|
| 2 | **Deployooor** | `dwow_deployooor_contract` | Yes (infrastructure) | WASM contract deployment, singleton enforcement, manifest storage |
| 3 | **Promissory Note** | `dwow_promissory_note_contract` | No | Universal DeFi primitive — tokens, transfers, swaps, redemption |
| 4 | **NativeToken** | `dwow_native_token_contract` | Yes | Block rewards, fee payment, supply audit |
| 5 | **Identity** | `dwow_identity_contract` | No | Credential issuance, selective disclosure, capability proofs |
| 6 | **Oracle** | `dwow_oracle_contract` | No | External data feeds — price, randomness, attestation data |
| 7 | **Attestation** | `dwow_attestation_contract` | No | Trust verification — on-chain attestations from trusted issuers |
| 8 | **Purse** | `dwow_purse_contract` | No | Fungible capability container — hidden balances via Pedersen commitments |
| 9 | **Box** | `dwow_box_contract` | No | Capability delegation — Put/Take with linear consumption via nullifier |
| 10 | **MultiSig** | `dwow_multisig_contract` | No | Private threshold voting — N-of-M groups, zero-knowledge ballots |

## ContractId Derivation

Every genesis contract ID is derived deterministically:

```
ContractId = poseidon_hash([42, 0, counter])
```

Where `42` is the `CONTRACT_ID_PREFIX` constant and `0` is the x-coordinate
(`pallas::Base::zero()`). The x-coordinate is zero because 0 is not a valid
x-coordinate for any Pallas curve point — this means a signature can never be
produced for these IDs, preventing anyone from claiming to be the deployer of
a genesis contract.

Counter starts at 2. Counters 0 and 1 are unused. The constants are defined in
`src/sdk/src/crypto/contract_id.rs` as `lazy_static!` values.

## Consensus-Critical vs. Ecosystem

Only two contracts are **consensus-critical**: Deployooor (counter 2) and
NativeToken (counter 4). The chain cannot function without them — Deployooor
provides the deployment infrastructure that every contract depends on, and
NativeToken handles block rewards and fee payment.

The remaining seven contracts are **ecosystem infrastructure**. They are deployed
at genesis to provide canonical well-known ContractIds for composable O-Cap
primitives. Any contract can reference `PURSE_CONTRACT_ID` for balance tracking
or `MULTISIG_CONTRACT_ID` for threshold voting without worrying about
fragmentation from replica deployments. They play zero role in block validation,
fee payment, or coinbase rewards — they are genesis-deployed purely for
ecosystem convenience, not consensus necessity.

## Bootstrap Sequence

During `dwowd` startup, `init_linear()` embeds each contract's WASM binary at
compile time via `include_bytes!()` and stores it via `set_contract_data()`.
Manifests are stored under `_manifest`-suffixed keys for manifest-based
capability resolution. The full sequence is:

1. Store Deployooor WASM (infrastructure — no manifest needed)
2. Store NativeToken WASM (consensus-critical — no manifest needed)
3. Store PromissoryNote WASM + manifest
4. Store Identity WASM + manifest
5. Store Oracle WASM + manifest
6. Store Attestation WASM + manifest
7. Store Purse WASM + manifest
8. Store Box WASM + manifest
9. Store MultiSig WASM + manifest
10. Create genesis block at height 1 with coinbase reward

## Adding a New Genesis Contract

When adding a new contract to genesis (counter 11 and beyond), these files must
be updated:

| File | Change |
|------|--------|
| `src/sdk/src/crypto/contract_id.rs` | Add `lazy_static!` for new ContractId, update `GENESIS_CONTRACT_IDS_BYTES` array size |
| `src/sdk/src/crypto/mod.rs` | Add new ContractId to `pub use` re-exports |
| `bin/dwowd/src/lib.rs` | Add `include_bytes!` + `set_contract_data` block in `init_linear()` |
| `bin/dwowd/src/tests/genesis.rs` | Add to `GenesisHarness::new()` |
| `contrib/docker/darkwow-testnet/Dockerfile` | Add `zkas rebuild` + WASM `cargo build` + `cp` lines |
| `Cargo.toml` | Add contract to workspace members |
| **This page** | Add row to the contract table |

That's it. No other documentation needs updating — every other page references
this one rather than repeating the list.

## See Also

- [Formal Specification](formal-specification.md) — One-page architecture reference
- [Contract Trust Model](contract-trust-model.md) — How genesis trust tier works
- [O-Cap Model](ocap.md) — How genesis primitives compose
- [Wallet Architecture](wallet.md) — How the wallet discovers genesis contracts
- Source: `src/sdk/src/crypto/contract_id.rs`, `bin/dwowd/src/lib.rs`
