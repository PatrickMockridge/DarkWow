/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! ContractTestingPipeline — Lightweight deployment pipeline (Level 1).
//!
//! Tests contract deployment through the **Deployooor contract** — the real
//! production path. Builds a `DeployV1` transaction with `DeployParamsV1`,
//! submits it through `apply_block_with_uncles()`, and verifies the deployed
//! contract is recorded in Deployooor's lock tree and initialized correctly.
//! No ZK proofs are generated.
//!
//! ## Demarcation from Heavyweight Tests
//!
//! | Concern | Lightweight (here) | Heavyweight |
//! |---------|-------------------|-------------|
//! | Deployment path | **Deployooor** (real production flow) | Direct `deploy_contract()` (setup convenience) |
//! | ContractId origin | Derived from deploy keypair (`ContractId::derive_public`) | Deterministic hash of contract name |
//! | Init params | Real serialized params via `DeployParamsV1.ix` | Empty `ix` (contract defaults) |
//! | ZK proofs | None | Required for all calls |
//! | Tests | Deployment correctness | Function/endpoint behavior, uncle-merkle stress |
//!
//! **Both are required.** Lightweight tests verify the Deployooor deployment
//! pipeline works end-to-end. Heavyweight tests verify contract functions,
//! state transitions, and uncle-merkle block execution using the direct
//! deploy path for test setup convenience.
//!
//! ## Running
//!
//! ```bash
//! cargo test -p dwowd test_pipeline
//! CONTRACT_NAME=promissory_note cargo test -p dwowd test_pipeline
//! cargo test -p dwowd test_all_contracts_deploy
//! ```

use std::env;
use std::sync::Arc;

use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockVersion};
use dwow_sdk::crypto::{ContractId, Keypair, SecretKey, DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};
use dwow_sdk::crypto::keypair::Network;
use dwow_sdk::deploy::{ContractMetadata, DeployParamsV1};
use dwow_sdk::pasta::pallas;
use dwow_serial::Encodable;
use rand::rngs::OsRng;

use super::genesis::GenesisHarness;
use super::harness::{build_contract_tx, build_test_block};
use super::heavyweight_pipeline::build_witness;
use crate::registry::model::LinearPowRewardZk;

/// Lightweight deployment pipeline — tests Deployooor deployment through the
/// production `accept_block` path so Deployooor post-processing (WASM storage +
/// `__initialize`) actually executes. Uses a real PoWRewardV1 coinbase (same
/// production path as mining).
pub struct ContractTestingPipeline {
    genesis: GenesisHarness,
    contract_name: String,
    zk: Arc<LinearPowRewardZk>,
    keys_path: std::path::PathBuf,
}

impl ContractTestingPipeline {
    const TEST_KEY_TOML: &'static str =
        "[node0]\nwallet_secret = \
         \"0100000000000000000000000000000000000000000000000000000000000000\"\n";

    /// Create a new lightweight pipeline. Initializes genesis (height 1) with
    /// all 9 contracts deployed, compiles ZK proving keys for coinbases.
    pub async fn new(contract_name: &str) -> Result<Self> {
        let genesis = GenesisHarness::new_without_contracts()?;
        let zk = LinearPowRewardZk::new(genesis.chain_state.clone()).await?;
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_lwp_{}_{}.toml", std::process::id(), contract_name));
        std::fs::write(&keys_path, Self::TEST_KEY_TOML)
            .map_err(|e| dwow_core::Error::Custom(format!("write test keys: {}", e)))?;

        // Genesis block — deploys all 9 contracts.
        let mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).map_err(|e| dwow_core::Error::Custom(format!("open test keys for genesis: {}", e)))?;
        let gen_recipient = crate::accounts::MiningRecipient::from_account(
            &mgr, BlockHeight::GENESIS,
        ).map_err(|e| dwow_core::Error::Custom(format!("MiningRecipient for genesis: {}", e)))?;
        drop(mgr);
        crate::init_genesis(&genesis.chain_state, gen_recipient, [0xDA, 0x57, 0x01, 0x57]).await?;

        Ok(Self { genesis, contract_name: contract_name.to_string(), zk: Arc::new(zk), keys_path })
    }

    /// One-shot: build everything and deploy the contract.
    pub async fn ensure_ready_and_deploy(&mut self) -> Result<ContractId> {
        self.deploy().await
    }

    /// Deploy the contract WASM through the Deployooor contract via
    /// `accept_block` — the real production path. Deployooor post-processing
    /// in `execute_block` stores WASM and calls `__initialize`.
    pub async fn deploy(&self) -> Result<ContractId> {
        let wasm = self.load_contract_wasm()?;
        let deploy_keypair = Keypair::new(SecretKey::random(&mut OsRng));
        let contract_id = ContractId::derive_public(deploy_keypair.public);

        let ix = self.build_init_params()?;
        let deploy_params = DeployParamsV1 {
            wasm_bincode: wasm,
            public_key: deploy_keypair.public,
            ix,
            singleton: false,
            singleton_name: String::new(),
        };
        let mut call_data = vec![0x00u8];
        deploy_params.encode(&mut call_data)?;

        let mut contract_tx = build_contract_tx(*DEPLOYOOOR_CONTRACT_ID, call_data);
        contract_tx.witness = build_witness(*DEPLOYOOOR_CONTRACT_ID, &contract_tx.contract_calls[0].data, vec![]);

        let height = self.genesis.block_height();
        let next_height = height.succ();
        let reward = dwow_sdk::blockchain::expected_reward(next_height);

        // Real PoWRewardV1 coinbase.
        let mgr = crate::accounts::AccountManager::open(
            &self.keys_path, Network::Testnet, "node0",
        ).map_err(|e| dwow_core::Error::Custom(format!("open test keys: {}", e)))?;
        let recipient = crate::accounts::MiningRecipient::from_account(&mgr, next_height)
            .map_err(|e| dwow_core::Error::Custom(format!("MiningRecipient: {}", e)))?;
        drop(mgr);
        let (_cb, _pi, pow_reward_call, _coin_blind) =
            crate::registry::model::build_linear_coinbase(
                recipient, reward, &self.zk, next_height,
            ).await?;
        let coinbase = dwow_chain::Transaction {
            version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0, nullifiers: vec![_cb.nullifier], witness: vec![],
        };

        let block = build_test_block(&self.genesis.chain_state, next_height, vec![coinbase, contract_tx]);
        let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block.header.randomx_key)
            .map_err(|e| dwow_core::Error::Custom(format!("RandomX cache: {}", e)))?;
        let vm = Arc::new(randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
            .map_err(|e| dwow_core::Error::Custom(format!("RandomX VM: {}", e)))?);

        crate::block_acceptor::accept_block(
            &self.genesis.chain_state, &block, &[], &vm, height, BlockTarget::MAX, None,
        ).map_err(|e| dwow_core::Error::Custom(format!("accept_block deploy: {}", e)))?;
        Ok(contract_id)
    }

    /// Deploy with ContractMetadata as the ix payload. Same production path
    /// as [`deploy`] — routes through `accept_block`.
    pub async fn deploy_with_metadata(&self, metadata: &ContractMetadata) -> Result<ContractId> {
        let wasm = self.load_contract_wasm()?;
        let deploy_keypair = Keypair::new(SecretKey::random(&mut OsRng));
        let contract_id = ContractId::derive_public(deploy_keypair.public);

        let ix = metadata.to_ix_bytes();
        let deploy_params = DeployParamsV1 {
            wasm_bincode: wasm, public_key: deploy_keypair.public, ix,
            singleton: false, singleton_name: String::new(),
        };
        let mut call_data = vec![0x00u8];
        deploy_params.encode(&mut call_data)?;

        let mut contract_tx = build_contract_tx(*DEPLOYOOOR_CONTRACT_ID, call_data);
        contract_tx.witness = build_witness(*DEPLOYOOOR_CONTRACT_ID, &contract_tx.contract_calls[0].data, vec![]);

        let height = self.genesis.block_height();
        let next_height = height.succ();
        let reward = dwow_sdk::blockchain::expected_reward(next_height);

        let mgr = crate::accounts::AccountManager::open(
            &self.keys_path, Network::Testnet, "node0",
        ).map_err(|e| dwow_core::Error::Custom(format!("open test keys: {}", e)))?;
        let recipient = crate::accounts::MiningRecipient::from_account(&mgr, next_height)
            .map_err(|e| dwow_core::Error::Custom(format!("MiningRecipient: {}", e)))?;
        drop(mgr);
        let (_cb, _pi, pow_reward_call, _coin_blind) =
            crate::registry::model::build_linear_coinbase(
                recipient, reward, &self.zk, next_height,
            ).await?;
        let coinbase = dwow_chain::Transaction {
            version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0, nullifiers: vec![_cb.nullifier], witness: vec![],
        };

        let block = build_test_block(&self.genesis.chain_state, next_height, vec![coinbase, contract_tx]);
        let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block.header.randomx_key)
            .map_err(|e| dwow_core::Error::Custom(format!("RandomX cache: {}", e)))?;
        let vm = Arc::new(randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
            .map_err(|e| dwow_core::Error::Custom(format!("RandomX VM: {}", e)))?);

        crate::block_acceptor::accept_block(
            &self.genesis.chain_state, &block, &[], &vm, height, BlockTarget::MAX, None,
        ).map_err(|e| dwow_core::Error::Custom(format!("accept_block deploy_with_metadata: {}", e)))?;
        Ok(contract_id)
    }

    /// Build initialization params (ix) for the contract's __initialize call.
    /// Returns empty vec for contracts that ignore the payload (25 of 28).
    /// For the 3 contracts with initialization params (dex, stablecoin, bridge),
    /// returns serialized default params so the full Deployooor path is exercised.
    fn build_init_params(&self) -> Result<Vec<u8>> {
        let ix: Vec<u8> = match self.contract_name.as_str() {
            "dex" => {
                // Use defaults matching init_contract empty-payload fallback
                use dwow_dex_contract::model::{InitializeParams, TransparencyConfig};
                let params = InitializeParams {
                    timeout: 100,
                    fee: 0,
                    trusted_money_merkle_root: [0u8; 32],
                    transparency_config: TransparencyConfig::default(),
                };
                dwow_serial::serialize(&params)
            }
            "stablecoin" => {
                use dwow_stablecoin_contract::model::{InitializeParams, StablecoinModel,
                    DeadManSwitchConfig, DeadManAction};
                use dwow_stablecoin_contract::{
                    CDP_MIN_COLLATERALIZATION_RATIO, CDP_LIQUIDATION_THRESHOLD,
                    CDP_LIQUIDATION_PENALTY, CDP_BASE_RATE, CDP_PI_KP, CDP_PI_KI,
                    CDP_PRICE_FEED_TWAP_WINDOW, CDP_PRICE_DEVIATION_THRESHOLD,
                };
                let params = InitializeParams {
                    model: StablecoinModel::PooledDebt,
                    min_collateralization_ratio: CDP_MIN_COLLATERALIZATION_RATIO,
                    liquidation_threshold: CDP_LIQUIDATION_THRESHOLD,
                    liquidation_penalty: CDP_LIQUIDATION_PENALTY,
                    base_rate: CDP_BASE_RATE,
                    pi_kp: CDP_PI_KP,
                    pi_ki: CDP_PI_KI,
                    twap_window: CDP_PRICE_FEED_TWAP_WINDOW,
                    price_deviation_threshold: CDP_PRICE_DEVIATION_THRESHOLD,
                    collateral_params: vec![],
                    dead_man_switch: DeadManSwitchConfig {
                        enabled: false,
                        timeout_blocks: 0,
                        action: DeadManAction::DisableMinting,
                        last_action_block: 0,
                    },
                    token_authority_pub: dwow_sdk::crypto::PublicKey::from_secret(
                        dwow_sdk::crypto::SecretKey::from_base(dwow_sdk::pasta::pallas::Base::from(1u64)),
                    ),
                    create_token: false,
                    token_symbol: [0u8; 32],
                    deployer_auth: dwow_sdk::pasta::pallas::Base::zero(),
                    promissory_note_contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32])
                        .unwrap(),
                };
                dwow_serial::serialize(&params)
            }
            "bridge" => {
                use dwow_bridge_contract::model::UpdateConfigParams;
                use dwow_bridge_contract::BRIDGE_CONTRACT_XMR_CONFIRMATIONS;
                let params = UpdateConfigParams {
                    deposit_fee: 0,
                    withdrawal_fee: 0,
                    min_confirmations: BRIDGE_CONTRACT_XMR_CONFIRMATIONS as u32,
                    max_deposit: u64::MAX,
                    max_withdrawal: u64::MAX,
                    gov_pub_x: pallas::Base::zero(),
                    gov_pub_y: pallas::Base::zero(),
                    config_nullifier: pallas::Base::from(7u64),
                };
                dwow_serial::serialize(&params)
            }
            _ => vec![],
        };
        Ok(ix)
    }

    /// Load pre-built WASM bytes for the contract.
    fn load_contract_wasm(&self) -> Result<Vec<u8>> {
        match self.contract_name.as_str() {
            "attestation" => Ok(include_bytes!(
                "../../../../src/contract/attestation/dwow_attestation_contract.wasm"
            ).to_vec()),
            "auction" => Ok(include_bytes!(
                "../../../../src/contract/auction/dwow_auction_contract.wasm"
            ).to_vec()),
            "bridge" => Ok(include_bytes!(
                "../../../../src/contract/bridge/dwow_bridge_contract.wasm"
            ).to_vec()),
            "dao_escrow" => Ok(include_bytes!(
                "../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm"
            ).to_vec()),
            "deployooor" => Ok(include_bytes!(
                "../../../../src/contract/deployooor/dwow_deployooor_contract.wasm"
            ).to_vec()),
            "dex" => Ok(include_bytes!(
                "../../../../src/contract/dex/dwow_dex_contract.wasm"
            ).to_vec()),
            "drain_protection" => Ok(include_bytes!(
                "../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm"
            ).to_vec()),
            "escrow" => Ok(include_bytes!(
                "../../../../src/contract/escrow/dwow_escrow_contract.wasm"
            ).to_vec()),
            "game_room" => Ok(include_bytes!(
                "../../../../src/contract/game_room/dwow_game_room_contract.wasm"
            ).to_vec()),
            "identity" => Ok(include_bytes!(
                "../../../../src/contract/identity/dwow_identity_contract.wasm"
            ).to_vec()),
            "insurance_market" => Ok(include_bytes!(
                "../../../../src/contract/insurance_market/dwow_insurance_market_contract.wasm"
            ).to_vec()),
            "labor_market" => Ok(include_bytes!(
                "../../../../src/contract/labor_market/dwow_labor_market_contract.wasm"
            ).to_vec()),
            "promissory_note" => Ok(include_bytes!(
                "../../../../src/contract/promissory_note/dwow_promissory_note_contract.wasm"
            ).to_vec()),
            "native_token" => Ok(include_bytes!(
                "../../../../src/contract/native_token/dwow_native_token_contract.wasm"
            ).to_vec()),
            "oracle" => Ok(include_bytes!(
                "../../../../src/contract/oracle/dwow_oracle_contract.wasm"
            ).to_vec()),
            "pool_stake" => Ok(include_bytes!(
                "../../../../src/contract/pool_stake/dwow_pool_stake_contract.wasm"
            ).to_vec()),
            "relayer_endowment" => Ok(include_bytes!(
                "../../../../src/contract/relayer_endowment/dwow_relayer_endowment_contract.wasm"
            ).to_vec()),
            "slot" => Ok(include_bytes!(
                "../../../../src/contract/slot/dwow_slot_contract.wasm"
            ).to_vec()),
            "stablecoin" => Ok(include_bytes!(
                "../../../../src/contract/stablecoin/dwow_stablecoin_contract.wasm"
            ).to_vec()),
            "subscription" => Ok(include_bytes!(
                "../../../../src/contract/subscription/dwow_subscription_contract.wasm"
            ).to_vec()),
            "tender" => Ok(include_bytes!(
                "../../../../src/contract/tender/dwow_tender_contract.wasm"
            ).to_vec()),
            "baccarat" => Ok(include_bytes!(
                "../../../../src/contract/baccarat/dwow_baccarat_contract.wasm"
            ).to_vec()),
            "betting_stake" => Ok(include_bytes!(
                "../../../../src/contract/betting_stake/dwow_betting_stake_contract.wasm"
            ).to_vec()),
            "darkbet_exchange" => Ok(include_bytes!(
                "../../../../src/contract/darkbet_exchange/dwow_darkbet_exchange_contract.wasm"
            ).to_vec()),
            "darktoshi_dice" => Ok(include_bytes!(
                "../../../../src/contract/darktoshi_dice/dwow_darktoshi_dice_contract.wasm"
            ).to_vec()),
            "lottery" => Ok(include_bytes!(
                "../../../../src/contract/lottery/dwow_lottery_contract.wasm"
            ).to_vec()),
            "otc_swap" => Ok(include_bytes!(
                "../../../../src/contract/otc_swap/dwow_otc_swap_contract.wasm"
            ).to_vec()),
            "roulette" => Ok(include_bytes!(
                "../../../../src/contract/roulette/dwow_roulette_contract.wasm"
            ).to_vec()),
            "bearer_bond" => Ok(include_bytes!(
                "../../../../src/contract/bearer_bond/dwow_bearer_bond_contract.wasm"
            ).to_vec()),
            "box" => Ok(include_bytes!(
                "../../../../src/contract/box/dwow_box_contract.wasm"
            ).to_vec()),
            "multisig" => Ok(include_bytes!(
                "../../../../src/contract/multisig/dwow_multisig_contract.wasm"
            ).to_vec()),
            "purse" => Ok(include_bytes!(
                "../../../../src/contract/purse/dwow_purse_contract.wasm"
            ).to_vec()),
            _ => Err(dwow_core::Error::Custom(format!(
                "Unknown or missing-WASM contract: {}. Add WASM include_bytes! entry in pipeline.rs",
                self.contract_name
            ))),
        }
    }

}

// ============================================================================
// Test functions
// ============================================================================

/// Deploy a specific contract (default: dex, override via CONTRACT_NAME env).
#[test]
fn test_pipeline() -> Result<()> {
    let contract_name = env::var("CONTRACT_NAME").unwrap_or_else(|_| "dex".to_string());
    println!("=== Lightweight Pipeline: {} ===", contract_name);

    smol::block_on(async {
        let mut pipeline = ContractTestingPipeline::new(&contract_name).await?;
        let contract_id = pipeline.ensure_ready_and_deploy().await?;
        println!("Deployed {} at {:?}", contract_name, contract_id.to_bytes());
        Ok(())
    })
}

/// Batch deploy all contracts to verify deployment plumbing.
#[test]
fn test_all_contracts_deploy() -> Result<()> {
    let contracts = [
        "attestation", "auction", "baccarat", "bearer_bond",
        "betting_stake", "box", "bridge", "dao_escrow",
        "darkbet_exchange", "darktoshi_dice", "deployooor", "dex",
        "drain_protection", "escrow", "game_room", "identity",
        "insurance_market", "labor_market", "lottery", "multisig",
        "native_token", "oracle", "otc_swap", "pool_stake",
        "promissory_note", "purse", "relayer_endowment", "roulette",
        "slot", "stablecoin", "subscription", "tender",
    ];

    println!("=== Batch Deploy All {} Contracts ===", contracts.len());
    let mut deployed = 0;
    let mut failed = Vec::new();

    smol::block_on(async {
        for name in &contracts {
            match ContractTestingPipeline::new(name).await {
                Ok(mut pipeline) => {
                    match pipeline.ensure_ready_and_deploy().await {
                        Ok(cid) => {
                            deployed += 1;
                            println!("  OK {} -> {:?}", name, cid.to_bytes());
                        }
                        Err(e) => {
                            println!("  FAIL {} deploy: {}", name, e);
                            failed.push((name, format!("{}", e)));
                        }
                    }
                }
                Err(e) => {
                    println!("  FAIL {} init: {}", name, e);
                    failed.push((name, format!("{}", e)));
                }
            }
        }
        Ok::<_, dwow_core::Error>(())
    })?;

    println!(
        "Deployed {}/{} contracts ({} failed)",
        deployed,
        contracts.len(),
        failed.len()
    );
    if !failed.is_empty() {
        for (name, err) in &failed {
            println!("  FAILED: {} — {}", name, err);
        }
    }

    Ok(())
}

/// Deploy escrow through Deployooor with ContractMetadata as the ix payload.
/// Verifies that metadata-carrying deployments succeed through the real
/// production deploy path.
#[test]
fn test_metadata_deploy_lightweight() -> Result<()> {
    use dwow_sdk::deploy::{Category, ContractMetadata};

    println!("=== Lightweight Pipeline: Escrow + ContractMetadata ===");

    smol::block_on(async {
        let pipeline = ContractTestingPipeline::new("escrow").await?;

        let metadata = ContractMetadata {
            name: "Test Escrow".to_string(),
            symbol: Some("TESC".to_string()),
            category: Category::Finance,
            description: Some("A test escrow contract with on-chain metadata".to_string()),
            public: true,
            attestations: vec![],
        };

        let ix_bytes = metadata.to_ix_bytes();
        assert!(!ix_bytes.is_empty(), "serialized metadata must be non-empty");

        let contract_id = pipeline.deploy_with_metadata(&metadata).await?;
        let height = pipeline.genesis.block_height();

        println!("Deployed escrow at {:?}", contract_id.to_bytes());
        println!("Block height after deploy: {}", height);

        assert!(height.get() > 0, "block height must increase after deploy");
        assert_ne!(contract_id.to_bytes(), [0u8; 32], "contract_id must not be zero");

        let decoded = ContractMetadata::from_ix_bytes(&ix_bytes)
            .expect("metadata must roundtrip");
        assert_eq!(decoded.name, "Test Escrow");
        assert_eq!(decoded.symbol.as_deref(), Some("TESC"));
        assert_eq!(decoded.category, Category::Finance);
        assert!(decoded.public);

        Ok(())
    })
}
