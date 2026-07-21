# Contracts

This page is the **single source of truth** for every contract in DarkWow.
Every other document that references the contract catalog links here rather
than repeating the list.

DarkWow has **32 deployable contracts**: 9 deployed at genesis (counters 2–10)
and 23 deployed post-genesis via Deployooor. There is also a `test-harness`
crate that is a test utility, not a deployable contract.

### Maturity Labels

| Label | Meaning |
|-------|---------|
| `✅ Proven` | Genesis-deployed, consensus-tested, Lean4-verified circuits |
| `⚠️ Experimental` | Post-genesis, code + circuits exist, basic tests pass |
| `🔬 Spec` | Design document only, no deployable contract crate |

---

## Genesis Contracts (9)

Deployed at block 1 with deterministic ContractIds derived from
`poseidon_hash([42, 0, counter])`. See [Genesis Contracts](arch/genesis.md)
for the ContractId derivation, consensus vs. ecosystem distinction, and
the complete bootstrap sequence.

| Counter | Contract | Crate | Consensus | Maturity | Role |
|---------|----------|-------|-----------|----------|------|
| 2 | **Deployooor** | `dwow_deployooor_contract` | Yes | ✅ Proven | WASM contract deployment, singleton enforcement, manifest storage |
| 3 | **Promissory Note** | `dwow_promissory_note_contract` | No | ✅ Proven | Universal DeFi primitive — tokens, transfers, swaps, redemption |
| 4 | **NativeToken** | `dwow_native_token_contract` | Yes | ✅ Proven | Block rewards, fee payment, supply audit |
| 5 | **Identity** | `dwow_identity_contract` | No | ✅ Proven | Credential issuance, selective disclosure, capability proofs |
| 6 | **Oracle** | `dwow_oracle_contract` | No | ✅ Proven | External data feeds — price, randomness, attestation data |
| 7 | **Attestation** | `dwow_attestation_contract` | No | ✅ Proven | Trust verification — on-chain attestations from trusted issuers |
| 8 | **Purse** | `dwow_purse_contract` | No | ✅ Proven | Fungible capability container — hidden balances via Pedersen commitments |
| 9 | **Box** | `dwow_box_contract` | No | ✅ Proven | Capability delegation — Put/Take with linear consumption via nullifier |
| 10 | **MultiSig** | `dwow_multisig_contract` | No | ✅ Proven | Private threshold voting — N-of-M groups, zero-knowledge ballots |

---

## Post-Genesis Contracts (23)

Deployed via [Deployooor](contract/deployooor.md) with ContractIds derived
from the deployer's public key.

### DeFi (8)

| Contract | Crate | Maturity | Description |
|----------|-------|----------|-------------|
| [Auction](contract/auction.md) | `dwow_auction_contract` | ⚠️ Experimental | Sealed-bid auctions with escrow integration |
| [Bearer Bond](contract/bearer_bond.md) | `dwow_bearer_bond_contract` | ⚠️ Experimental | Fixed-interest staking instruments |
| [DEX](contract/dex.md) | `dwow_dex_contract` | ⚠️ Experimental | Atomic swap DAO — bilateral token swaps |
| [Escrow](contract/escrow.md) | `dwow_escrow_contract` | ⚠️ Experimental | Timelock-based conditional payments |
| [OTC Swap](contract/otc_swap.md) | `dwow_otc_swap_contract` | ⚠️ Experimental | Peer-to-peer OTC token swaps |
| [Stablecoin](contract/stablecoin.md) | `dwow_stablecoin_contract` | ⚠️ Experimental | Collateralized debt position (CDP) stablecoin |
| [Subscription](contract/subscription.md) | `dwow_subscription_contract` | ⚠️ Experimental | Recurring payments and time-based billing |
| [Tender](contract/tender.md) | `dwow_tender_contract` | ⚠️ Experimental | Sealed-bid tendering with O-Cap gating |

### Gaming (8)

| Contract | Crate | Maturity | Description |
|----------|-------|----------|-------------|
| [Baccarat](contract/baccarat.md) | `dwow_baccarat_contract` | ⚠️ Experimental | Baccarat (Punto Banco) |
| [Betting Stake](contract/betting_stake.md) | `dwow_betting_stake_contract` | ⚠️ Experimental | Composable capital staking for betting contracts |
| [Darkbet Exchange](contract/darkbet_exchange.md) | `dwow_darkbet_exchange_contract` | ⚠️ Experimental | Decentralized betting exchange |
| [Darktoshi Dice](contract/darktoshi_dice.md) | `dwow_darktoshi_dice_contract` | ⚠️ Experimental | Satoshi Dice clone |
| [Game Room](contract/game_room.md) | `dwow_game_room_contract` | ⚠️ Experimental | Multi-game lobby and pot management |
| [Lottery](contract/lottery.md) | `dwow_lottery_contract` | ⚠️ Experimental | Privacy-preserving pooled lottery |
| [Roulette](contract/roulette.md) | `dwow_roulette_contract` | ⚠️ Experimental | European/American roulette |
| [Slot](contract/slot.md) | `dwow_slot_contract` | ⚠️ Experimental | Modular slot machine |

### DAO & Governance (2)

| Contract | Crate | Maturity | Description |
|----------|-------|----------|-------------|
| [DAO Escrow](contract/dao_escrow.md) | `dwow_dao_escrow_contract` | ⚠️ Experimental | DAO-governed endowment with three modes |
| [Drain Protection](contract/drain_protection.md) | `dwow_drain_protection_contract` | ⚠️ Experimental | Gradual withdrawal limits and rate limiting |

### Infrastructure (3)

| Contract | Crate | Maturity | Description |
|----------|-------|----------|-------------|
| [Bridge](contract/bridge.md) | `dwow_bridge_contract` | ⚠️ Experimental | Cross-chain transfers via O-Cap security |
| [Pool Stake](contract/pool_stake.md) | `dwow_pool_stake_contract` | ⚠️ Experimental | Pooled staking for relayer coverage |
| [Relayer Endowment](contract/relayer_endowment.md) | `dwow_relayer_endowment_contract` | ⚠️ Experimental | Capital deployment to relayers |

### Markets (2)

| Contract | Crate | Maturity | Description |
|----------|-------|----------|-------------|
| [Insurance Market](contract/insurance_market.md) | `dwow_insurance_market_contract` | ⚠️ Experimental | Decentralized risk marketplace |
| [Labor Market](contract/labor_market.md) | `dwow_labor_market_contract` | ⚠️ Experimental | Service marketplace with milestone payments |

---

## Test Utility (not a deployable contract)

| Crate | Description |
|-------|-------------|
| `dwow_contract_test_harness` | Isolated test harness for contract testing — not deployed on-chain |

---

## Summary

| Category | Count |
|----------|-------|
| Genesis (consensus-critical) | 2 (Deployooor, NativeToken) |
| Genesis (ecosystem infrastructure) | 7 (PromissoryNote, Identity, Oracle, Attestation, Purse, Box, MultiSig) |
| Post-genesis DeFi | 8 |
| Post-genesis Gaming | 8 |
| Post-genesis DAO & Governance | 2 |
| Post-genesis Infrastructure | 3 |
| Post-genesis Markets | 2 |
| **Total deployable contracts** | **32** |
| Test utilities (not deployable) | 1 (test-harness) |

## See Also

- [Genesis Contracts](arch/genesis.md) — ContractId derivation, consensus vs. ecosystem, bootstrap sequence
- [Contract Manifest](arch/manifest.md) — On-chain ABI and capability declarations
- [Contract Trust Model](arch/contract-trust-model.md) — Genesis → SelfDeployed → Attested → Unverified
- [Promissory Note Intermediaries](contract/promissory_note_intermediaries.md) — How 22 contracts interact with PN
- Source: `src/sdk/src/crypto/contract_id.rs`, `bin/dwowd/src/lib.rs`, `Cargo.toml`
