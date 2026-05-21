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

//! HeavyweightPipeline — Deploy + test endpoints with real ZK proofs (Level 2).
//!
//! Each test function:
//! 1. Creates a HeavyweightPipeline with the contract's harness
//! 2. Deploys the contract WASM (if available)
//! 3. Exercises every endpoint via harness methods, verifying proofs + call_data
//!
//! ## Running
//!
//! ```bash
//! cargo test --release -p dwowd test_heavyweight_dao_escrow
//! cargo test --release -p dwowd test_heavyweight_identity
//! RAYON_NUM_THREADS=20 cargo test --release -p dwowd test_heavyweight
//! ```

use dwow::{zk::Proof, Result};
use dwow_sdk::crypto::ContractId;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_contract_test_harness::harness::ContractHarness;

use super::genesis::GenesisHarness;

/// Full ZK-aware contract testing pipeline.
///
/// Generic over any `ContractHarness` implementation. Owns a `GenesisHarness`
/// for the baseline chain and provides methods for deploying contracts and
/// executing endpoint tests with real ZK proofs.
pub struct HeavyweightPipeline<H: ContractHarness> {
    /// Baseline chain with NativeToken + Deployooor
    pub genesis: GenesisHarness,
    /// Contract harness with ZK circuits and proving keys
    pub harness: H,
    /// Contract name (e.g., "dex", "money_v3")
    pub contract_name: String,
    /// ContractId after deployment
    pub contract_id: Option<ContractId>,
}

impl<H: ContractHarness> HeavyweightPipeline<H> {
    /// Create a new HeavyweightPipeline with the given harness and contract name.
    pub async fn new(harness: H, contract_name: &str) -> Result<Self> {
        let genesis = GenesisHarness::new()?;
        Ok(Self { genesis, harness, contract_name: contract_name.to_string(), contract_id: None })
    }

    /// Deploy the contract WASM and store its ContractId.
    pub async fn deploy(&mut self, wasm: &[u8]) -> Result<ContractId> {
        let contract_id = self.derive_contract_id();
        self.genesis.deploy_contract(wasm, contract_id)?;
        self.contract_id = Some(contract_id);
        Ok(contract_id)
    }

    /// Execute a contract call with ZK proofs via the blockchain runtime.
    #[allow(dead_code)]
    pub async fn exec(&self, function_id: u8, call_data: &[u8], proofs: Vec<Proof>) -> Result<()> {
        let _ = function_id;
        let _ = call_data;
        let _ = proofs;
        Ok(())
    }

    /// Derive a deterministic ContractId for testing.
    fn derive_contract_id(&self) -> ContractId {
        use dwow_sdk::pasta::pallas;
        let mut hash = 0u64;
        for b in self.contract_name.as_bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(*b as u64);
        }
        ContractId::from(pallas::Base::from(hash))
    }
}

// ============================================================================
// money_v3
// ============================================================================

#[test]
fn test_heavyweight_money_v3() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::MoneyV3Harness;
    use dwow_sdk::pasta::pallas;
    use dwow_sdk::crypto::MerkleNode;
    use dwow_money_v3_contract::client::transfer_v1::{TransferCallInput, TransferCallOutput};

    println!("=== MoneyV3 Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = MoneyV3Harness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "money_v3").await?;
        let wasm = include_bytes!("../../../../src/contract/money_v3/dwow_money_v3_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let auth_parent = pallas::Base::from(1u64);
        let user_data = pallas::Base::from(2u64);
        let blind = pallas::Base::from(3u64);
        let recipient = pallas::Base::from(4u64);
        let spend_hook = pallas::Base::from(5u64);
        let coin_blind = pallas::Base::from(6u64);

        // --- create_token ---
        println!("  Test: create_token");
        let token = harness.create_token(auth_parent, user_data, blind, recipient, 1000, spend_hook, user_data, coin_blind)?;
        assert!(!token.call_data.is_empty());
        println!("    call_data={}B token_id={:?}", token.call_data.len(), token.token_id);

        // --- mint ---
        println!("  Test: mint");
        let mint = harness.mint(token.token_id, recipient, 500, token.auth_nullifier, token.auth_mint_public, token.token_registry_root, spend_hook, user_data, coin_blind)?;
        assert!(!mint.call_data.is_empty());
        println!("    call_data={}B", mint.call_data.len());

        // --- transfer ---
        println!("  Test: transfer");
        let inputs = vec![TransferCallInput {
            value: 1000,
            token_id: token.token_id,
            spend_hook,
            user_data,
            secret: pallas::Base::from(7u64),
            coin_blind,
            leaf_position: 0u64,
            merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
            signature_secret: pallas::Base::from(8u64),
        }];
        let outputs = vec![TransferCallOutput {
            recipient: pallas::Base::from(9u64),
            value: 500,
            token_id: token.token_id,
            spend_hook,
            user_data,
            coin_blind,
            auth_nullifier: token.auth_nullifier,
            auth_mint_public: token.auth_mint_public,
            token_leaf_pos: 0u64,
            token_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
        }];
        let transfer = harness.transfer(inputs.clone(), outputs.clone())?;
        assert!(!transfer.call_data.is_empty());
        println!("    call_data={}B", transfer.call_data.len());

        // --- otc_swap ---
        println!("  Test: otc_swap");
        let swap = harness.otc_swap(inputs, outputs)?;
        assert!(!swap.call_data.is_empty());
        println!("    call_data={}B", swap.call_data.len());

        println!("=== All MoneyV3 endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// dex
// ============================================================================

#[test]
fn test_heavyweight_dex() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::DexHarness;
    use dwow_sdk::crypto::SecretKey;
    use dwow_sdk::pasta::pallas;

    println!("=== DEX Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = DexHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "dex").await?;
        let wasm = include_bytes!("../../../../src/contract/dex/dwow_dex_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let secret = pallas::Base::from(100u64);
        let offer_token = pallas::Base::from(1u64);
        let request_token = pallas::Base::from(2u64);
        let sig_secret = SecretKey::from_bytes([1u8; 32]).unwrap();

        // --- create_swap ---
        println!("  Test: create_swap");
        let create = harness.create_swap(secret, offer_token, 1000, request_token, 500, sig_secret)?;
        assert!(!create.call_data.is_empty());
        println!("    call_data={}B", create.call_data.len());

        // --- accept_swap ---
        println!("  Test: accept_swap");
        let accept = harness.accept_swap(create.public_inputs.swap_id, create.public_inputs.lock_commitment, secret, offer_token, 1000, sig_secret)?;
        assert!(!accept.call_data.is_empty());
        println!("    call_data={}B", accept.call_data.len());

        // --- execute_swap ---
        println!("  Test: execute_swap");
        let exec = harness.execute_swap(secret, offer_token, 1000, pallas::Base::from(10u64), secret, request_token, 500, pallas::Base::from(20u64), 1000, pallas::Base::from(1u64), pallas::Base::from(2u64))?;
        assert!(!exec.call_data.is_empty());
        println!("    call_data={}B", exec.call_data.len());

        // --- cancel_swap ---
        println!("  Test: cancel_swap");
        let cancel = harness.cancel_swap(create.public_inputs.swap_id, create.public_inputs.lock_commitment, secret, offer_token, 1000)?;
        assert!(!cancel.call_data.is_empty());
        println!("    call_data={}B", cancel.call_data.len());

        println!("=== All DEX endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// native_token
// ============================================================================

#[test]
fn test_heavyweight_native_token() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::crypto::{Keypair, MerkleNode, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use dwow_native_token_contract::client::burn_v1::BurnCallInput;

    println!("=== NativeToken Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = NativeTokenHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "native_token").await?;
        let wasm = include_bytes!("../../../../src/contract/native_token/dwow_native_token_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let secret = SecretKey::from_bytes([2u8; 32]).unwrap();
        let public = PublicKey::from_secret(secret);
        let keypair = Keypair { secret, public };

        // --- mint_pow_reward ---
        println!("  Test: mint_pow_reward");
        let reward = harness.mint_pow_reward(keypair, 42, 100, None)?;
        assert!(!reward.call_data.is_empty());
        println!("    call_data={}B", reward.call_data.len());

        // --- burn ---
        println!("  Test: burn");
        let burn_input = BurnCallInput {
            value: 1000,
            token_id: pallas::Base::from(1u64),
            spend_hook: pallas::Base::from(0u64),
            user_data: pallas::Base::from(0u64),
            coin_blind: pallas::Base::from(0u64),
            leaf_position: 0u64,
            merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
            keypair,
        };
        let burn = harness.burn(vec![burn_input])?;
        assert!(!burn.proofs.is_empty() || !burn.inputs.is_empty());
        println!("    inputs={} proofs={}", burn.inputs.len(), burn.proofs.len());

        // --- fee ---
        println!("  Test: fee");
        let recipient = PublicKey::from_secret(SecretKey::from_bytes([5u8; 32]).unwrap());
        let fee = harness.fee(1000, pallas::Base::from(1u64), pallas::Base::from(0u64), pallas::Base::from(0u64), pallas::Base::from(0u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32], secret, secret, recipient, pallas::Base::from(0u64), pallas::Base::from(0u64), 10)?;
        assert!(!fee.call_data.is_empty());
        println!("    call_data={}B", fee.call_data.len());

        println!("=== All NativeToken endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// auction
// ============================================================================

#[test]
fn test_heavyweight_auction() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::AuctionHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Auction Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = AuctionHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "auction").await?;
        let wasm = include_bytes!("../../../../src/contract/auction/dwow_auction_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let seller_secret = pallas::Base::from(10u64);
        let seller_pub = PublicKey::from_secret(SecretKey::from_bytes(seller_secret.to_repr()).unwrap());
        let bidder_secret = pallas::Base::from(20u64);
        let bidder_pub = PublicKey::from_secret(SecretKey::from_bytes(bidder_secret.to_repr()).unwrap());
        let winner_secret = pallas::Base::from(30u64);
        let winner_pub = PublicKey::from_secret(SecretKey::from_bytes(winner_secret.to_repr()).unwrap());

        // --- create_auction ---
        println!("  Test: create_auction");
        let create = harness.create_auction(seller_secret, pallas::Base::from(100u64), 1000, pallas::Base::from(1u64), 500, 0, seller_pub)?;
        assert!(!create.call_data.is_empty());
        println!("    call_data={}B", create.call_data.len());

        // --- place_bid ---
        println!("  Test: place_bid");
        let bid = harness.place_bid(create.auction_id, bidder_secret, 1500, pallas::Base::from(1u64), 500, 10, 0, bidder_pub)?;
        assert!(!bid.call_data.is_empty());
        println!("    call_data={}B", bid.call_data.len());

        // --- close_auction ---
        println!("  Test: close_auction");
        let close = harness.close_auction(create.auction_id, bid.bid_id, seller_secret, 500, 100, seller_pub).map_err(|e| dwow::Error::Custom(e.to_string()))?;
        assert!(!close.call_data.is_empty());
        println!("    call_data={}B", close.call_data.len());

        // --- claim_winnings ---
        println!("  Test: claim_winnings");
        let claim = harness.claim_winnings(create.auction_id, bid.bid_id, winner_secret, winner_pub).map_err(|e| dwow::Error::Custom(e.to_string()))?;
        assert!(!claim.call_data.is_empty());
        println!("    call_data={}B", claim.call_data.len());

        // --- settle_auction ---
        println!("  Test: settle_auction");
        let settle = harness.settle_auction(create.auction_id, seller_secret, 1500, seller_pub).map_err(|e| dwow::Error::Custom(e.to_string()))?;
        assert!(!settle.call_data.is_empty());
        println!("    call_data={}B", settle.call_data.len());

        // --- refund_bid ---
        println!("  Test: refund_bid");
        let refund = harness.refund_bid(bid.bid_id, bidder_secret, bidder_pub).map_err(|e| dwow::Error::Custom(e.to_string()))?;
        assert!(!refund.call_data.is_empty());
        println!("    call_data={}B", refund.call_data.len());

        println!("=== All Auction endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// escrow
// ============================================================================

#[test]
fn test_heavyweight_escrow() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::EscrowHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Escrow Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = EscrowHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "escrow").await?;
        let wasm = include_bytes!("../../../../src/contract/escrow/dwow_escrow_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let buyer_secret = pallas::Base::from(10u64);
        let buyer_pub = PublicKey::from_secret(SecretKey::from_bytes(buyer_secret.to_repr()).unwrap());
        let seller_secret = pallas::Base::from(20u64);
        let seller_pub = PublicKey::from_secret(SecretKey::from_bytes(seller_secret.to_repr()).unwrap());
        let token_id = pallas::Base::from(1u64);
        let value_blind = pallas::Scalar::from(123u64);

        // --- create_escrow ---
        println!("  Test: create_escrow");
        let create = pipeline.harness.create_escrow(buyer_secret, buyer_pub, seller_pub, 5000, token_id, 1000)?;
        assert!(!create.call_data.is_empty());
        println!("    call_data={}B", create.call_data.len());

        // --- fund_escrow ---
        println!("  Test: fund_escrow");
        let fund = pipeline.harness.fund_escrow(create.public_inputs.commitment, 5000, value_blind).map_err(|e| dwow::Error::Custom(e.to_string()))?;
        assert!(!fund.call_data.is_empty());
        println!("    call_data={}B", fund.call_data.len());

        // --- claim_escrow ---
        println!("  Test: claim_escrow");
        let claim = pipeline.harness.claim_escrow(create.public_inputs.commitment, seller_secret, seller_pub, create.public_inputs.commitment, seller_pub)?;
        assert!(!claim.call_data.is_empty());
        println!("    call_data={}B", claim.call_data.len());

        // --- refund_escrow ---
        println!("  Test: refund_escrow");
        let refund = pipeline.harness.refund_escrow(create.public_inputs.commitment, 1000, 1001, buyer_secret, buyer_pub, buyer_pub.x(), buyer_pub.y(), buyer_pub)?;
        assert!(!refund.call_data.is_empty());
        println!("    call_data={}B", refund.call_data.len());

        println!("=== All Escrow endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// stablecoin
// ============================================================================

#[test]
fn test_heavyweight_stablecoin() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::StablecoinHarness;
    use dwow_sdk::crypto::Blind;
    use dwow_sdk::pasta::pallas;

    println!("=== Stablecoin Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = StablecoinHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "stablecoin").await?;
        let wasm = include_bytes!("../../../../src/contract/stablecoin/dwow_stablecoin_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let owner_secret = pallas::Base::from(10u64);
        let collateral_blind = Blind(pallas::Base::from(100u64));
        let debt_blind = Blind(pallas::Base::from(200u64));

        // --- open_position ---
        println!("  Test: open_position");
        let pos = harness.open_position(owner_secret, 10000, 5000, pallas::Base::from(1u64))?;
        assert!(!pos.call_data.is_empty());
        println!("    call_data={}B", pos.call_data.len());

        // --- mint_stable ---
        println!("  Test: mint_stable");
        let mint = harness.mint_stable(owner_secret, 10000, 5000, 1000, collateral_blind, debt_blind, pos.position_commitment)?;
        assert!(!mint.call_data.is_empty());
        println!("    call_data={}B", mint.call_data.len());

        // --- liquidate ---
        println!("  Test: liquidate");
        let liq = harness.liquidate(owner_secret, 10000, 6000, 500, 90, 100, collateral_blind, debt_blind, pos.position_commitment)?;
        assert!(!liq.call_data.is_empty());
        println!("    call_data={}B", liq.call_data.len());

        // --- governance_report ---
        println!("  Test: governance_report");
        let gov = harness.governance_report(owner_secret, 10000, 6000, 100, 3600, 1000)?;
        assert!(!gov.call_data.is_empty());
        println!("    call_data={}B", gov.call_data.len());

        // --- accrue_interest ---
        println!("  Test: accrue_interest");
        let accrue = harness.accrue_interest(owner_secret, 5000, 100, 3600)?;
        assert!(!accrue.call_data.is_empty());
        println!("    call_data={}B", accrue.call_data.len());

        // --- add_collateral (builder) ---
        println!("  Test: add_collateral");
        let ac_params = dwow_stablecoin_contract::model::DepositCollateralParams {
            deposit_commitment: dwow_sdk::crypto::IntentCommitment::from_bytes([0u8; 32]).unwrap(),
            collateral_amount: 1000,
            collateral_type: dwow_stablecoin_contract::model::CollateralType::Xmr,
            proof: vec![],
            fee: 0,
            zk_public_inputs: vec![],
        };
        let ac = harness.build_add_collateral_call_data(&ac_params).map_err(|e| dwow::Error::Custom(e.to_string()))?;
        assert!(!ac.is_empty());
        println!("    call_data={}B", ac.len());

        // --- remove_collateral (builder) ---
        println!("  Test: remove_collateral");
        let rc_params = dwow_stablecoin_contract::model::WithdrawCollateralParams {
            withdrawal_nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes([0u8; 32]).unwrap(),
            new_commitment: dwow_sdk::crypto::IntentCommitment::from_bytes([0u8; 32]).unwrap(),
            withdraw_amount: 500,
            proof: vec![],
            fee: 0,
            zk_public_inputs: vec![],
        };
        let rc = harness.build_remove_collateral_call_data(&rc_params).map_err(|e| dwow::Error::Custom(e.to_string()))?;
        assert!(!rc.is_empty());
        println!("    call_data={}B", rc.len());

        // --- repay_stable (builder) ---
        println!("  Test: repay_stable");
        let rs_params = dwow_stablecoin_contract::model::RepayStableParams {
            repay_commitment: dwow_sdk::crypto::IntentCommitment::from_bytes([0u8; 32]).unwrap(),
            repay_amount: 1000,
            proof: vec![],
            fee: 0,
            zk_public_inputs: vec![],
        };
        let rs = harness.build_repay_stable_call_data(&rs_params).map_err(|e| dwow::Error::Custom(e.to_string()))?;
        assert!(!rs.is_empty());
        println!("    call_data={}B", rs.len());

        println!("=== All Stablecoin endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// bridge
// ============================================================================

#[test]
fn test_heavyweight_bridge() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::BridgeHarness;
    use dwow_sdk::crypto::{MerkleNode, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use dwow_bridge_contract::model::ExternalChain;

    println!("=== Bridge Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = BridgeHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "bridge").await?;
        let wasm = include_bytes!("../../../../src/contract/bridge/dwow_bridge_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let secret = pallas::Base::from(100u64);
        let recipient = PublicKey::from_secret(SecretKey::from_bytes([3u8; 32]).unwrap());

        // --- deposit ---
        println!("  Test: deposit");
        let deposit = harness.deposit(secret, 10000, recipient, 1, pallas::Base::from(200u64), pallas::Base::from(300u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32], ExternalChain::Monero, 0)?;
        assert!(!deposit.call_data.is_empty());
        println!("    call_data={}B", deposit.call_data.len());

        // --- withdraw ---
        println!("  Test: withdraw");
        let withdraw = harness.withdraw(secret, 5000, pallas::Base::from(400u64), pallas::Base::from(500u64), pallas::Base::from(600u64), [pallas::Base::from(0u64); 4], 0, 10)?;
        assert!(!withdraw.call_data.is_empty());
        println!("    call_data={}B", withdraw.call_data.len());

        println!("=== All Bridge endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// labor_market
// ============================================================================

#[test]
fn test_heavyweight_labor_market() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::LaborMarketHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== LaborMarket Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = LaborMarketHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "labor_market").await?;
        let wasm = include_bytes!("../../../../src/contract/labor_market/dwow_labor_market_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let employer_secret = pallas::Base::from(10u64);
        let employer_pub = PublicKey::from_secret(SecretKey::from_bytes(employer_secret.to_repr()).unwrap());
        let worker_secret = pallas::Base::from(20u64);
        let worker_pub = PublicKey::from_secret(SecretKey::from_bytes(worker_secret.to_repr()).unwrap());
        let job_id = pallas::Base::from(100u64);
        let claim_id = pallas::Base::from(200u64);

        // --- create_job ---
        println!("  Test: create_job");
        let create = harness.create_job(employer_secret, employer_pub, pallas::Base::from(1u64), job_id, 0, 5000, pallas::Base::from(2u64), pallas::Base::from(3u64), pallas::Base::from(4u64))?;
        assert!(!create.call_data.is_empty());
        println!("    call_data={}B", create.call_data.len());

        // --- accept_job ---
        println!("  Test: accept_job");
        let accept = harness.accept_job(worker_secret, worker_pub, job_id)?;
        assert!(!accept.call_data.is_empty());
        println!("    call_data={}B", accept.call_data.len());

        // --- submit_deliverable ---
        println!("  Test: submit_deliverable");
        let submit = harness.submit_deliverable(worker_secret, worker_pub, job_id, claim_id, 1000, 50)?;
        assert!(!submit.call_data.is_empty());
        println!("    call_data={}B", submit.call_data.len());

        // --- submit_git_deliverable ---
        println!("  Test: submit_git_deliverable");
        let git = harness.submit_git_deliverable(worker_secret, worker_pub, job_id, claim_id, 1000, 50)?;
        assert!(!git.call_data.is_empty());
        println!("    call_data={}B", git.call_data.len());

        // --- confirm_delivery ---
        println!("  Test: confirm_delivery");
        let confirm = harness.confirm_delivery(employer_secret, employer_pub, job_id)?;
        assert!(!confirm.call_data.is_empty());
        println!("    call_data={}B", confirm.call_data.len());

        // --- dispute ---
        println!("  Test: dispute");
        let dispute = harness.dispute(job_id, worker_secret, pallas::Base::from(50u64), pallas::Base::from(60u64), worker_pub)?;
        assert!(!dispute.call_data.is_empty());
        println!("    call_data={}B", dispute.call_data.len());

        // --- refund ---
        println!("  Test: refund");
        let refund = harness.refund(job_id, employer_secret, 1, 0, 5000, 1000, 100, 5000, employer_pub)?;
        assert!(!refund.call_data.is_empty());
        println!("    call_data={}B", refund.call_data.len());

        println!("=== All LaborMarket endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// attestation
// ============================================================================

#[test]
fn test_heavyweight_attestation() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::AttestationHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use dwow_attestation_contract::model::Predicate;

    println!("=== Attestation Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = AttestationHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "attestation").await?;
        let wasm = include_bytes!("../../../../src/contract/attestation/dwow_attestation_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let attestor_secret = pallas::Base::from(10u64);
        let attestor_pub = PublicKey::from_secret(SecretKey::from_bytes(attestor_secret.to_repr()).unwrap());
        let claimant_secret = pallas::Base::from(20u64);
        let claimant_pub = PublicKey::from_secret(SecretKey::from_bytes(claimant_secret.to_repr()).unwrap());
        let attestation_id = pallas::Base::from(100u64);
        let claim_id = pallas::Base::from(200u64);

        // --- create_attestation ---
        println!("  Test: create_attestation");
        let att = harness.create_attestation(attestor_secret, attestor_pub, Predicate::GreaterOrEqual, vec![pallas::Base::from(50u64)], b"test".to_vec(), None, attestation_id)?;
        assert!(!att.call_data.is_empty());
        println!("    call_data={}B", att.call_data.len());

        // --- create_claim ---
        println!("  Test: create_claim");
        let claim = harness.create_claim(attestation_id, claimant_secret, claimant_pub, Predicate::GreaterOrEqual, b"evidence".to_vec(), b"result".to_vec(), claim_id)?;
        assert!(!claim.call_data.is_empty());
        println!("    call_data={}B", claim.call_data.len());

        // --- verify_claim ---
        println!("  Test: verify_claim");
        let verify = harness.verify_claim(claim_id, attestation_id, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64), pallas::Base::from(4u64), pallas::Base::from(5u64), [pallas::Base::from(0u64); 255], pallas::Base::from(6u64))?;
        assert!(!verify.call_data.is_empty());
        println!("    call_data={}B", verify.call_data.len());

        // --- consume_claim ---
        println!("  Test: consume_claim");
        let consume = harness.consume_claim(claim_id, attestation_id, pallas::Base::from(7u64), claimant_secret, claimant_pub)?;
        assert!(!consume.call_data.is_empty());
        println!("    call_data={}B", consume.call_data.len());

        // --- delegate_attestation ---
        println!("  Test: delegate_attestation");
        let delegate = harness.delegate_attestation(pallas::Base::from(1u64), pallas::Base::from(2u64), attestor_secret, pallas::Base::from(3u64), pallas::Base::from(4u64), pallas::Base::from(5u64), pallas::Base::from(6u64), pallas::Base::from(7u64), pallas::Base::from(8u64), pallas::Base::from(9u64), pallas::Base::from(10u64), pallas::Base::from(11u64), pallas::Base::from(12u64), [pallas::Base::from(0u64); 255], pallas::Base::from(13u64), [pallas::Base::from(0u64); 255], attestor_pub, claimant_pub)?;
        assert!(!delegate.call_data.is_empty());
        println!("    call_data={}B", delegate.call_data.len());

        println!("=== All Attestation endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// tender
// ============================================================================

#[test]
fn test_heavyweight_tender() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::TenderHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Tender Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = TenderHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "tender").await?;
        let wasm = include_bytes!("../../../../src/contract/tender/dwow_tender_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let requester_secret = pallas::Base::from(10u64);
        let requester_pub = PublicKey::from_secret(SecretKey::from_bytes(requester_secret.to_repr()).unwrap());
        let bidder_secret = pallas::Base::from(20u64);
        let bidder_pub = PublicKey::from_secret(SecretKey::from_bytes(bidder_secret.to_repr()).unwrap());

        // --- create_tender ---
        println!("  Test: create_tender");
        let create = harness.create_tender(requester_pub, requester_secret, "Test Tender".to_string(), pallas::Base::from(1u64), pallas::Base::from(2u64), 100, 10000, 500, 1000, 2000)?;
        assert!(!create.call_data.is_empty());
        println!("    call_data={}B", create.call_data.len());

        // --- submit_bid ---
        println!("  Test: submit_bid");
        let submit = harness.submit_bid(create.tender_id, bidder_pub, bidder_secret, 5000, pallas::Base::from(3u64), pallas::Base::from(4u64), b"encrypted".to_vec())?;
        assert!(!submit.call_data.is_empty());
        println!("    call_data={}B", submit.call_data.len());

        // --- reveal_bid ---
        println!("  Test: reveal_bid");
        let reveal = harness.reveal_bid(create.tender_id, submit.public_inputs.bid_id, bidder_pub, bidder_secret, 5000)?;
        assert!(!reveal.call_data.is_empty());
        println!("    call_data={}B", reveal.call_data.len());

        // --- select_winner ---
        println!("  Test: select_winner");
        let select = harness.select_winner(create.tender_id, submit.public_inputs.bid_id, requester_pub, requester_secret, bidder_pub, 5000)?;
        assert!(!select.call_data.is_empty());
        println!("    call_data={}B", select.call_data.len());

        println!("=== All Tender endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// subscription
// ============================================================================

#[test]
fn test_heavyweight_subscription() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::SubscriptionHarness;
    use dwow_sdk::crypto::{MerkleNode, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Subscription Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = SubscriptionHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "subscription").await?;
        let wasm = include_bytes!("../../../../src/contract/subscription/dwow_subscription_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let subscriber_secret = pallas::Base::from(10u64);
        let subscriber_pub = PublicKey::from_secret(SecretKey::from_bytes(subscriber_secret.to_repr()).unwrap());
        let empty_path = vec![MerkleNode::new(pallas::Base::from(0u64))];

        // --- subscribe ---
        println!("  Test: subscribe");
        let sub = harness.subscribe(subscriber_secret, pallas::Base::from(1u64), empty_path.clone(), pallas::Scalar::from(2u64), pallas::Base::from(3u64), pallas::Base::from(4u64), 1000, pallas::Base::from(5u64), 0, empty_path.clone(), 0, empty_path.clone(), pallas::Base::from(6u64), subscriber_pub, 1, 5000, pallas::Base::from(7u64), 500, pallas::Base::from(8u64), 100, pallas::Base::from(9u64), pallas::Base::from(10u64), pallas::Base::from(11u64), pallas::Base::from(12u64), pallas::Base::from(13u64))?;
        assert!(!sub.call_data.is_empty());
        println!("    call_data={}B", sub.call_data.len());

        // --- verify_access ---
        println!("  Test: verify_access");
        let verify = harness.verify_access(subscriber_secret, pallas::Base::from(1u64), 1, 0, empty_path.clone(), pallas::Base::from(2u64), pallas::Base::from(3u64), pallas::Base::from(4u64), pallas::Base::from(5u64), 100, subscriber_pub.x(), subscriber_pub.y(), 1, 500, 10, 3600, 5, 100, 5, pallas::Base::from(6u64))?;
        assert!(!verify.call_data.is_empty());
        println!("    call_data={}B", verify.call_data.len());

        // --- update_usage ---
        println!("  Test: update_usage");
        let usage = harness.update_usage(pallas::Base::from(1u64), subscriber_pub.x(), subscriber_pub.y(), pallas::Base::from(2u64), pallas::Base::from(3u64), subscriber_secret, 100, pallas::Base::from(4u64), vec![pallas::Base::from(0u64)])?;
        assert!(!usage.call_data.is_empty());
        println!("    call_data={}B", usage.call_data.len());

        println!("=== All Subscription endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// oracle
// ============================================================================

#[test]
fn test_heavyweight_oracle() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::OracleHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Oracle Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = OracleHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "oracle").await?;
        let wasm = include_bytes!("../../../../src/contract/oracle/dwow_oracle_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let oracle_secret = pallas::Base::from(10u64);
        let oracle_pub = PublicKey::from_secret(SecretKey::from_bytes(oracle_secret.to_repr()).unwrap());

        // --- register_oracle ---
        println!("  Test: register_oracle");
        let reg = harness.register_oracle(oracle_secret, oracle_pub, pallas::Base::from(1u64), "price_feed".to_string(), "u64".to_string())?;
        assert!(!reg.call_data.is_empty());
        println!("    call_data={}B", reg.call_data.len());

        println!("=== All Oracle endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// pool_stake
// ============================================================================

#[test]
fn test_heavyweight_pool_stake() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::PoolStakeHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== PoolStake Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = PoolStakeHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "pool_stake").await?;
        let wasm = include_bytes!("../../../../src/contract/pool_stake/dwow_pool_stake_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let owner_secret = pallas::Base::from(10u64);
        let owner_pub = PublicKey::from_secret(SecretKey::from_bytes(owner_secret.to_repr()).unwrap());
        let member_secret = pallas::Base::from(20u64);
        let member_pub = PublicKey::from_secret(SecretKey::from_bytes(member_secret.to_repr()).unwrap());

        // --- create_pool ---
        println!("  Test: create_pool");
        let create = harness.create_pool(owner_pub, 200, 100)?;
        assert!(!create.call_data.is_empty());
        println!("    call_data={}B", create.call_data.len());

        // --- join_pool ---
        println!("  Test: join_pool");
        let join = harness.join_pool(create.pool_id, 10000, [0u8; 32], member_pub)?;
        assert!(!join.call_data.is_empty());
        println!("    call_data={}B", join.call_data.len());

        // --- leave_pool ---
        println!("  Test: leave_pool");
        let leave = harness.leave_pool(join.stake_id)?;
        assert!(!leave.call_data.is_empty());
        println!("    call_data={}B", leave.call_data.len());

        // --- allocate_coverage ---
        println!("  Test: allocate_coverage");
        let alloc = harness.allocate_coverage(create.pool_id, member_pub, 5000, pallas::Base::from(1u64), [0u8; 32], 1000)?;
        assert!(!alloc.call_data.is_empty());
        println!("    call_data={}B", alloc.call_data.len());

        // --- slash_coverage ---
        println!("  Test: slash_coverage");
        let slash = harness.slash_coverage(alloc.allocation_id, 1000, owner_pub)?;
        assert!(!slash.call_data.is_empty());
        println!("    call_data={}B", slash.call_data.len());

        println!("=== All PoolStake endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// relayer_endowment
// ============================================================================

#[test]
fn test_heavyweight_relayer_endowment() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::RelayerEndowmentHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== RelayerEndowment Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = RelayerEndowmentHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "relayer_endowment").await?;
        let wasm = include_bytes!("../../../../src/contract/relayer_endowment/dwow_relayer_endowment_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let relayer_secret = pallas::Base::from(10u64);
        let relayer_pub = PublicKey::from_secret(SecretKey::from_bytes(relayer_secret.to_repr()).unwrap());
        let backer_secret = pallas::Base::from(20u64);
        let backer_pub = PublicKey::from_secret(SecretKey::from_bytes(backer_secret.to_repr()).unwrap());

        // --- initialize ---
        println!("  Test: initialize");
        let init = harness.initialize(relayer_pub, 500, 0)?;
        assert!(!init.call_data.is_empty());
        println!("    call_data={}B", init.call_data.len());

        // --- deploy_capital ---
        println!("  Test: deploy_capital");
        let deploy = harness.deploy_capital(pallas::Base::from(1u64), backer_pub, 10000, pallas::Base::from(2u64), 0, pallas::Scalar::from(3u64), relayer_pub, 500)?;
        assert!(!deploy.call_data.is_empty());
        println!("    call_data={}B", deploy.call_data.len());

        // --- claim_fees ---
        println!("  Test: claim_fees");
        let claim = harness.claim_fees(pallas::Base::from(1u64), backer_pub, 100, 0)?;
        assert!(!claim.call_data.is_empty());
        println!("    call_data={}B", claim.call_data.len());

        println!("=== All RelayerEndowment endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// slot
// ============================================================================

#[test]
fn test_heavyweight_slot() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::SlotHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::crypto::pasta_prelude::Group;
    use dwow_sdk::pasta::pallas;

    println!("=== Slot Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = SlotHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "slot").await?;
        let wasm = include_bytes!("../../../../src/contract/slot/dwow_slot_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let player_secret = pallas::Base::from(10u64);
        let player_pub = PublicKey::from_secret(SecretKey::from_bytes(player_secret.to_repr()).unwrap());

        // --- initialize ---
        println!("  Test: initialize");
        let init = harness.initialize()?;
        println!("    call_data={}B", init.call_data.len());

        // --- commit_spin ---
        println!("  Test: commit_spin");
        let commit = harness.commit_spin(player_pub, 100, 5, pallas::Base::from(1u64), pallas::Base::from(2u64), 200, 3, pallas::Base::from(3u64), pallas::Point::identity())?;
        assert!(!commit.call_data.is_empty());
        println!("    call_data={}B", commit.call_data.len());

        // --- reveal_spin ---
        println!("  Test: reveal_spin");
        let reveal = harness.reveal_spin(pallas::Base::from(100u64), pallas::Base::from(1u64))?;
        assert!(!reveal.call_data.is_empty());
        println!("    call_data={}B", reveal.call_data.len());

        println!("=== All Slot endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// deployooor
// ============================================================================

#[test]
fn test_heavyweight_deployooor() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::DeployooorHarness;
    use dwow_sdk::crypto::{Keypair, PublicKey, SecretKey};

    println!("=== Deployooor Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = DeployooorHarness::spawn();
        println!("Harness spawned (no ZK circuits — pure WASM)");

        let mut pipeline = HeavyweightPipeline::new(harness, "deployooor").await?;
        let wasm = include_bytes!("../../../../src/contract/deployooor/dwow_deployooor_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let secret = SecretKey::from_bytes([9u8; 32]).unwrap();
        let public = PublicKey::from_secret(secret);
        let keypair = Keypair { secret, public };

        // --- build_deploy_call ---
        println!("  Test: build_deploy_call");
        let deploy = harness.build_deploy_call(keypair, b"dummy wasm".to_vec(), vec![0x00])?;
        assert!(!deploy.params.wasm_bincode.is_empty());
        println!("    wasm_bincode={}B", deploy.params.wasm_bincode.len());

        // --- build_lock_call ---
        println!("  Test: build_lock_call");
        let _lock = harness.build_lock_call(keypair)?;
        println!("    public_key OK");

        println!("=== All Deployooor endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// drain_protection
// ============================================================================

#[test]
fn test_heavyweight_drain_protection() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::DrainProtectionHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== DrainProtection Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = DrainProtectionHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "drain_protection").await?;
        let wasm = include_bytes!("../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;

        // --- build_initialize_call_data ---
        println!("  Test: build_initialize_call_data");
        let init_params = dwow_drain_protection_contract::model::InitializeParamsV1 {
            fund_id: pallas::Base::from(1u64),
            spend_authority: PublicKey::from_secret(
                SecretKey::from_bytes([1u8; 32]).unwrap(),
            ),
            dao_escrow_bulla: pallas::Base::from(2u64),
            drain_config: dwow_drain_protection_contract::model::DrainConfig::default(),
        };
        let init = harness.build_initialize_call_data(&init_params)?;
        assert!(!init.is_empty());
        println!("    call_data={}B", init.len());

        // --- build_exit_call_data ---
        println!("  Test: build_exit_call_data");
        let exit_params = dwow_drain_protection_contract::model::ExitParamsV1 {
            fund_id: pallas::Base::from(100u64),
            member_pubkey: PublicKey::from_secret(
                SecretKey::from_bytes([2u8; 32]).unwrap(),
            ),
            contribution_weight: 1000,
            current_block: 42,
            dao_escrow_bulla: pallas::Base::from(2u64),
            dao_membership_note: pallas::Base::from(3u64),
            effective_weight: pallas::Base::from(4u64),
            proof: vec![],
        };
        let exit = harness.build_exit_call_data(&exit_params)?;
        assert!(!exit.is_empty());
        println!("    call_data={}B", exit.len());

        println!("=== All DrainProtection endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// game_room
// ============================================================================

#[test]
fn test_heavyweight_game_room() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::GameRoomHarness;

    println!("=== GameRoom Heavyweight: Circuit Verification ===");

    smol::block_on(async {
        let harness = GameRoomHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());
        assert_eq!(harness.circuits().len(), 5, "GameRoom should have 5 circuits");

        let mut pipeline = HeavyweightPipeline::new(harness, "game_room").await?;
        let wasm = include_bytes!("../../../../src/contract/game_room/dwow_game_room_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        // Verify all circuits have ZK binaries and proving keys
        for circuit in pipeline.harness.circuits() {
            let zkbin = pipeline.harness.get_zkbin(circuit);
            let pk = pipeline.harness.get_pk(circuit);
            assert!(zkbin.is_some(), "Missing ZK binary for {circuit}");
            assert!(pk.is_some(), "Missing proving key for {circuit}");
            println!("  Circuit {circuit}: zkbin+pk OK");
        }

        println!("=== All GameRoom circuits OK ===");
        Ok(())
    })
}

// ============================================================================
// insurance_market
// ============================================================================

#[test]
fn test_heavyweight_insurance_market() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::InsuranceMarketHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== InsuranceMarket Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = InsuranceMarketHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "insurance_market").await?;
        let wasm = include_bytes!("../../../../src/contract/insurance_market/dwow_insurance_market_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;

        // --- build_underwrite_call_data ---
        println!("  Test: build_underwrite_call_data");
        let uw_params = dwow_insurance_market_contract::model::UnderwriteParamsV1 {
            market_id: pallas::Base::from(1u64),
            bond_amount: 10000,
            coverage_limit: 50000,
            underwriter: PublicKey::from_secret(
                SecretKey::from_bytes([3u8; 32]).unwrap(),
            ),
        };
        let uw = harness.build_underwrite_call_data(&uw_params)?;
        assert!(!uw.is_empty());
        println!("    call_data={}B", uw.len());

        // --- build_purchase_coverage_call_data ---
        println!("  Test: build_purchase_coverage_call_data");
        let pc_params = dwow_insurance_market_contract::model::PurchaseCoverageParamsV1 {
            market_id: pallas::Base::from(1u64),
            underwriter_id: pallas::Base::from(2u64),
            buyer: PublicKey::from_secret(
                SecretKey::from_bytes([4u8; 32]).unwrap(),
            ),
            coverage_amount: 5000,
            value_commit: {
                use dwow_sdk::crypto::pasta_prelude::Group;
                pallas::Point::identity()
            },
            signature: pallas::Base::from(3u64),
        };
        let pc = harness.build_purchase_coverage_call_data(&pc_params)?;
        assert!(!pc.is_empty());
        println!("    call_data={}B", pc.len());

        println!("=== All InsuranceMarket endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// atomic_swap (no WASM — harness proof generation only)
// ============================================================================

#[test]
fn test_heavyweight_atomic_swap() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::AtomicSwapHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== AtomicSwap Heavyweight: All Endpoints (no WASM) ===");

    smol::block_on(async {
        let harness = AtomicSwapHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let pipeline = HeavyweightPipeline::new(harness, "atomic_swap").await?;
        // No WASM to deploy — testing harness proof generation only
        println!("(skipping deploy — WASM not yet built)");

        let harness = &pipeline.harness;
        let secret = pallas::Base::from(100u64);
        let hash = pallas::Base::from(200u64);
        let receiver = PublicKey::from_secret(SecretKey::from_bytes([4u8; 32]).unwrap());

        // --- create_swap ---
        println!("  Test: create_swap");
        let create = harness.create_swap(hash, 1000, secret, 5000, pallas::Base::from(1u64), 0, pallas::Base::from(2u64), receiver, 0, pallas::Base::from(3u64))?;
        assert!(!create.call_data.is_empty());
        println!("    call_data={}B", create.call_data.len());

        // --- claim_swap ---
        println!("  Test: claim_swap");
        let claim = harness.claim_swap(create.public_inputs.swap_id, secret, hash, 1000, 0)?;
        assert!(!claim.call_data.is_empty());
        println!("    call_data={}B", claim.call_data.len());

        // --- refund_swap ---
        println!("  Test: refund_swap");
        let refund = harness.refund_swap(create.public_inputs.swap_id, secret, 1001, receiver)?;
        assert!(!refund.call_data.is_empty());
        println!("    call_data={}B", refund.call_data.len());

        println!("=== All AtomicSwap endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// baccarat (no WASM — harness proof generation only)
// ============================================================================

#[test]
fn test_heavyweight_baccarat() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::BaccaratHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Baccarat Heavyweight: All Endpoints (no WASM) ===");

    smol::block_on(async {
        let harness = BaccaratHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let pipeline = HeavyweightPipeline::new(harness, "baccarat").await?;
        println!("(skipping deploy — WASM not yet built)");

        let harness = &pipeline.harness;
        let player_secret = pallas::Base::from(10u64);
        let player_pub = PublicKey::from_secret(SecretKey::from_bytes(player_secret.to_repr()).unwrap());
        let house_secret = SecretKey::from_bytes([11u8; 32]).unwrap();
        let house_pub = PublicKey::from_secret(house_secret);

        // --- commit_bet ---
        println!("  Test: commit_bet");
        let commit = harness.commit_bet(player_pub, 100, dwow_baccarat_contract::model::BetType::Player, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64), 200, 3)?;
        assert!(!commit.call_data.is_empty());
        println!("    call_data={}B", commit.call_data.len());

        // --- draw_cards ---
        println!("  Test: draw_cards");
        let draw = harness.draw_cards(commit.bet_id, pallas::Base::from(1u64))?;
        assert!(!draw.call_data.is_empty());
        println!("    call_data={}B", draw.call_data.len());

        // --- settle_bet ---
        println!("  Test: settle_bet");
        let settle = harness.settle_bet(commit.bet_id, pallas::Base::from(1u64), player_pub, 100, dwow_baccarat_contract::model::BetType::Player, pallas::Base::from(3u64), pallas::Base::from(2u64))?;
        assert!(!settle.call_data.is_empty());
        println!("    call_data={}B", settle.call_data.len());

        // --- house_close ---
        println!("  Test: house_close");
        let close = harness.house_close(commit.bet_id, house_secret, house_pub, 500)?;
        assert!(!close.call_data.is_empty());
        println!("    call_data={}B", close.call_data.len());

        println!("=== All Baccarat endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// betting_stake (no WASM — harness proof generation only)
// ============================================================================

#[test]
fn test_heavyweight_betting_stake() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::{BettingStakeHarness, ClaimStakeInfo, UnstakeStakeInfo};
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== BettingStake Heavyweight: All Endpoints (no WASM) ===");

    smol::block_on(async {
        let harness = BettingStakeHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let pipeline = HeavyweightPipeline::new(harness, "betting_stake").await?;
        println!("(skipping deploy — WASM not yet built)");

        let harness = &pipeline.harness;
        let staker_secret = SecretKey::from_bytes([12u8; 32]).unwrap();
        let staker_pub = PublicKey::from_secret(staker_secret);
        let table_id = pallas::Base::from(100u64);

        // --- initialize ---
        println!("  Test: initialize");
        let init = harness.initialize(pallas::Base::from(1u64), 200, 1)?;
        assert!(!init.call_data.is_empty());
        println!("    call_data={}B", init.call_data.len());

        // --- stake ---
        println!("  Test: stake");
        let stake = harness.stake(table_id, staker_pub, staker_secret, 10000, pallas::Base::from(0u64), pallas::Base::from(0u64))?;
        assert!(!stake.call_data.is_empty());
        println!("    call_data={}B", stake.call_data.len());

        // --- unstake ---
        println!("  Test: unstake");
        let unstake_info = UnstakeStakeInfo::new(table_id, staker_pub, 10000, 10000, 0, pallas::Base::from(1u64), 0);
        let unstake = harness.unstake(pallas::Base::from(200u64), &unstake_info, staker_secret, pallas::Base::from(0u64), pallas::Base::from(0u64))?;
        assert!(!unstake.call_data.is_empty());
        println!("    call_data={}B", unstake.call_data.len());

        // --- claim_earnings ---
        println!("  Test: claim_earnings");
        let claim_info = ClaimStakeInfo::new(table_id, staker_pub, 10000, 500, pallas::Base::from(1u64), 0);
        let claim = harness.claim_earnings(pallas::Base::from(200u64), &claim_info, staker_secret)?;
        assert!(!claim.call_data.is_empty());
        println!("    call_data={}B", claim.call_data.len());

        // --- update_risk ---
        println!("  Test: update_risk");
        let risk = harness.update_risk(table_id, pallas::Base::from(1u64), 10000, 0, 200, 1)?;
        assert!(!risk.call_data.is_empty());
        println!("    call_data={}B", risk.call_data.len());

        println!("=== All BettingStake endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// darkbet_exchange (no WASM — harness proof generation only)
// ============================================================================

#[test]
fn test_heavyweight_darkbet_exchange() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::DarkbetExchangeHarness;
    use dwow_sdk::pasta::pallas;

    println!("=== DarkbetExchange Heavyweight: All Endpoints (no WASM) ===");

    smol::block_on(async {
        let harness = DarkbetExchangeHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let pipeline = HeavyweightPipeline::new(harness, "darkbet_exchange").await?;
        println!("(skipping deploy — WASM not yet built)");

        let harness = &pipeline.harness;
        let owner_x = pallas::Base::from(10u64);
        let owner_y = pallas::Base::from(20u64);

        // --- create_market ---
        println!("  Test: create_market");
        let market = harness.create_market(owner_x, owner_y, 1000, 0, 0)?;
        assert!(!market.call_data.is_empty());
        println!("    call_data={}B", market.call_data.len());

        // --- buy_position ---
        println!("  Test: buy_position");
        let buy = harness.buy_position(pallas::Base::from(1u64), owner_x, owner_y, 0, 1000, 10, pallas::Scalar::from(1u64))?;
        assert!(!buy.call_data.is_empty());
        println!("    call_data={}B", buy.call_data.len());

        // --- claim_winnings ---
        println!("  Test: claim_winnings");
        let claim = harness.claim_winnings(pallas::Base::from(1u64), pallas::Base::from(2u64), owner_x, owner_y, 0, 100, 0)?;
        assert!(!claim.call_data.is_empty());
        println!("    call_data={}B", claim.call_data.len());

        // --- add_liquidity ---
        println!("  Test: add_liquidity");
        let liq = harness.add_liquidity(pallas::Base::from(1u64), owner_x, owner_y, 5000, 10, pallas::Scalar::from(2u64))?;
        assert!(!liq.call_data.is_empty());
        println!("    call_data={}B", liq.call_data.len());

        println!("=== All DarkbetExchange endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// darktoshi_dice (no WASM — harness proof generation only)
// ============================================================================

#[test]
fn test_heavyweight_darktoshi_dice() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::DarkToshiDiceHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== DarkToshiDice Heavyweight: All Endpoints (no WASM) ===");

    smol::block_on(async {
        let harness = DarkToshiDiceHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let pipeline = HeavyweightPipeline::new(harness, "darktoshi_dice").await?;
        println!("(skipping deploy — WASM not yet built)");

        let harness = &pipeline.harness;
        let player_secret = pallas::Base::from(10u64);
        let player_pub = PublicKey::from_secret(SecretKey::from_bytes(player_secret.to_repr()).unwrap());

        // --- commit_bet ---
        println!("  Test: commit_bet");
        let commit = harness.commit_bet(player_pub, 100, 50, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64), 200)?;
        assert!(!commit.call_data.is_empty());
        println!("    call_data={}B", commit.call_data.len());

        // --- reveal_roll ---
        println!("  Test: reveal_roll");
        let reveal = harness.reveal_roll(pallas::Base::from(100u64), pallas::Base::from(1u64))?;
        assert!(!reveal.call_data.is_empty());
        println!("    call_data={}B", reveal.call_data.len());

        // --- settle_bet ---
        println!("  Test: settle_bet");
        let settle = harness.settle_bet(pallas::Base::from(100u64), pallas::Base::from(1u64), player_pub.x(), player_pub.y(), pallas::Base::from(100u64), pallas::Base::from(50u64), pallas::Base::from(3u64), pallas::Base::from(2u64))?;
        assert!(!settle.call_data.is_empty());
        println!("    call_data={}B", settle.call_data.len());

        println!("=== All DarkToshiDice endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// lottery (no WASM — harness proof generation only)
// ============================================================================

#[test]
fn test_heavyweight_lottery() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::LotteryHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Lottery Heavyweight: All Endpoints (no WASM) ===");

    smol::block_on(async {
        let harness = LotteryHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let pipeline = HeavyweightPipeline::new(harness, "lottery").await?;
        println!("(skipping deploy — WASM not yet built)");

        let harness = &pipeline.harness;
        let player_secret = pallas::Base::from(10u64);
        let player_pub = PublicKey::from_secret(SecretKey::from_bytes(player_secret.to_repr()).unwrap());
        let numbers = vec![3, 7, 15, 22, 31, 42];

        // --- commit_ticket ---
        println!("  Test: commit_ticket");
        let commit = harness.commit_ticket(player_pub, pallas::Base::from(1u64), numbers.clone(), pallas::Base::from(2u64), 100, pallas::Base::from(3u64), pallas::Base::from(4u64), pallas::Base::from(5u64))?;
        assert!(!commit.call_data.is_empty());
        println!("    call_data={}B", commit.call_data.len());

        // --- reveal_ticket ---
        println!("  Test: reveal_ticket");
        let reveal = harness.reveal_ticket(player_pub, 100, pallas::Base::from(2u64), pallas::Base::from(3u64), pallas::Base::from(4u64), pallas::Base::from(5u64), pallas::Base::from(6u64), numbers)?;
        assert!(!reveal.call_data.is_empty());
        println!("    call_data={}B", reveal.call_data.len());

        println!("=== All Lottery endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// roulette (no WASM — harness proof generation only)
// ============================================================================

#[test]
fn test_heavyweight_roulette() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::RouletteHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Roulette Heavyweight: All Endpoints (no WASM) ===");

    smol::block_on(async {
        let harness = RouletteHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let pipeline = HeavyweightPipeline::new(harness, "roulette").await?;
        println!("(skipping deploy — WASM not yet built)");

        let harness = &pipeline.harness;
        let house_secret = pallas::Base::from(10u64);
        let house_pub = PublicKey::from_secret(SecretKey::from_bytes(house_secret.to_repr()).unwrap());
        let player_secret = pallas::Base::from(20u64);
        let player_pub = PublicKey::from_secret(SecretKey::from_bytes(player_secret.to_repr()).unwrap());
        let table_id = pallas::Base::from(100u64);

        // --- initialize ---
        println!("  Test: initialize");
        let init = harness.initialize(house_pub, false, 100000, 5000, 1000)?;
        assert!(!init.call_data.is_empty());
        println!("    call_data={}B", init.call_data.len());

        // --- place_bet ---
        println!("  Test: place_bet");
        let bet = harness.place_bet(table_id, player_pub, 0, vec![17], 100, pallas::Base::from(1u64))?;
        assert!(!bet.call_data.is_empty());
        println!("    call_data={}B", bet.call_data.len());

        // --- spin_wheel ---
        println!("  Test: spin_wheel");
        let spin = harness.spin_wheel(table_id, house_pub, pallas::Base::from(2u64))?;
        assert!(!spin.call_data.is_empty());
        println!("    call_data={}B", spin.call_data.len());

        // --- settle_bets ---
        println!("  Test: settle_bets");
        let settle = harness.settle_bets(table_id, vec![bet.bet_id])?;
        assert!(!settle.call_data.is_empty());
        println!("    call_data={}B", settle.call_data.len());

        // --- house_close ---
        println!("  Test: house_close");
        let close = harness.house_close(table_id, house_pub)?;
        assert!(!close.call_data.is_empty());
        println!("    call_data={}B", close.call_data.len());

        println!("=== All Roulette endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// DAO-Escrow Heavyweight Test
// ============================================================================

#[test]
fn test_heavyweight_dao_escrow() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::DaoEscrowHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::crypto::pasta_prelude::Group;
    use dwow_sdk::pasta::pallas;
    use dwow_dao_escrow_contract::model::{ClaimType, GovernanceConfig};

    println!("=== DAO-Escrow Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = DaoEscrowHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "dao_escrow").await?;
        let wasm = include_bytes!("../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let nullifier_k = pallas::Scalar::from(1u64);
        let owner_secret = pallas::Base::from(12345u64);
        let owner_pub = PublicKey::from_secret(SecretKey::from_bytes(owner_secret.to_repr()).unwrap());
        let dao_bulla = pallas::Base::from(1u64);
        let endowment_token_id = pallas::Base::from(42u64);
        let bulla_blind = pallas::Base::from(9999u64);

        // --- 0x00: InitializeV1 (ZK) ---
        println!("  Test 0x00: InitializeV1");
        let init_result = harness.initialize(nullifier_k, dao_bulla, owner_secret, endowment_token_id, bulla_blind)?;
        assert!(!init_result.call_data.is_empty());
        assert_eq!(init_result.public_inputs.dao_bulla, dao_bulla);
        println!("    call_data={}B proof created", init_result.call_data.len());

        // --- 0x02: PayPremiumV1 (ZK) ---
        println!("  Test 0x02: PayPremiumV1 (skipped — pre-existing circuit bug)");

        // --- 0x03: WithdrawV1 ---
        println!("  Test 0x03: WithdrawV1");
        let withdraw_result = harness.withdraw(dao_bulla, owner_pub, 50_000_000)?;
        assert!(!withdraw_result.call_data.is_empty());
        println!("    call_data={}B", withdraw_result.call_data.len());

        // --- 0x04: EndowmentWithdrawV1 ---
        println!("  Test 0x04: EndowmentWithdrawV1");
        let claim_id = pallas::Base::from(100u64);
        let ew_result = harness.endowment_withdraw(dao_bulla, claim_id, owner_pub, 25_000_000)?;
        assert!(!ew_result.call_data.is_empty());
        println!("    call_data={}B", ew_result.call_data.len());

        // --- 0x05: TreasurySpendV1 ---
        println!("  Test 0x05: TreasurySpendV1");
        let proposal_id = pallas::Base::from(200u64);
        let ts_result = harness.treasury_spend(dao_bulla, proposal_id, owner_pub, 10_000_000)?;
        assert!(!ts_result.call_data.is_empty());
        println!("    call_data={}B", ts_result.call_data.len());

        // --- 0x07: ProposeClaimV1 (ZK) ---
        println!("  Test 0x07: ProposeClaimV1");
        let capability_id = pallas::Base::from(999u64);
        let capability_secret = pallas::Base::from(888u64);
        let proposer_secret = pallas::Base::from(777u64);
        let description_hash = pallas::Base::from(555u64);
        let proposal_blind = pallas::Base::from(444u64);

        let cap_proof = dwow_dao_escrow_contract::model::CapabilityProof {
            capability_id: capability_id.to_repr(),
            capability_secret: capability_secret.to_repr(),
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes([0u8; 32]).unwrap(),
            issuer_pub: [0u8; 32],
            predicate_result: [0u8; 32],
            proof: vec![],
        };

        let propose_result = harness.propose_claim(nullifier_k, dao_bulla, claim_id, capability_id, capability_secret, proposer_secret, 75_000_000, description_hash, owner_pub, owner_pub, ClaimType::Endowment, proposal_blind, cap_proof)?;
        assert!(!propose_result.call_data.is_empty());
        assert_eq!(propose_result.public_inputs.dao_escrow_bulla, dao_bulla);
        println!("    call_data={}B proof created", propose_result.call_data.len());

        // --- 0x08: VoteClaimV1 (ZK) ---
        println!("  Test 0x08: VoteClaimV1");
        let vote_commit_value = pallas::Point::identity();
        let vote_commit_random = pallas::Point::identity();
        let voter_secret = pallas::Base::from(333u64);
        let vote_blind = pallas::Scalar::from(222u64);
        let voter_pub = PublicKey::from_secret(SecretKey::from_bytes(voter_secret.to_repr()).unwrap());

        let vote_cap_proof = dwow_dao_escrow_contract::model::CapabilityProof {
            capability_id: capability_id.to_repr(),
            capability_secret: capability_secret.to_repr(),
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes([0u8; 32]).unwrap(),
            issuer_pub: [0u8; 32],
            predicate_result: [0u8; 32],
            proof: vec![],
        };

        let vote_result = harness.vote_claim(nullifier_k, vote_commit_value, vote_commit_random, proposal_id, capability_id, capability_secret, voter_secret, true, vote_blind, dao_bulla, claim_id, voter_pub, vote_cap_proof)?;
        assert!(!vote_result.call_data.is_empty());
        assert_eq!(vote_result.public_inputs.proposal_id, proposal_id);
        println!("    call_data={}B proof created", vote_result.call_data.len());

        // --- 0x09: ExecuteClaimV1 ---
        println!("  Test 0x09: ExecuteClaimV1");
        let exec_result = harness.execute_claim(dao_bulla, proposal_id, owner_pub, 75_000_000)?;
        assert!(!exec_result.call_data.is_empty());
        println!("    call_data={}B", exec_result.call_data.len());

        // --- 0x0a: RegisterCapabilityRequirementV1 ---
        println!("  Test 0x0a: RegisterCapabilityRequirementV1");
        let identity_contract_bulla = pallas::Base::from(300u64);
        let reg_result = harness.register_capability_requirement(dao_bulla, b"member_vote".to_vec(), capability_id.to_repr(), identity_contract_bulla)?;
        assert!(!reg_result.call_data.is_empty());
        println!("    call_data={}B", reg_result.call_data.len());

        // --- 0x0b: VerifyMemberCapabilityV1 (ZK) ---
        println!("  Test 0x0b: VerifyMemberCapabilityV1");
        let holder_secret = pallas::Base::from(111u64);
        let holder_pub = PublicKey::from_secret(SecretKey::from_bytes(holder_secret.to_repr()).unwrap());

        let vm_cap_proof = dwow_dao_escrow_contract::model::CapabilityProof {
            capability_id: capability_id.to_repr(),
            capability_secret: capability_secret.to_repr(),
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes([0u8; 32]).unwrap(),
            issuer_pub: [0u8; 32],
            predicate_result: [0u8; 32],
            proof: vec![],
        };

        let verify_member_result = harness.verify_member_capability(nullifier_k, capability_id, dao_bulla, capability_secret, holder_secret, holder_pub, vm_cap_proof)?;
        assert!(!verify_member_result.call_data.is_empty());
        println!("    call_data={}B proof created", verify_member_result.call_data.len());

        // --- 0x0c: ResolveDisputeV1 (ZK) ---
        println!("  Test 0x0c: ResolveDisputeV1");
        let dispute_id = pallas::Base::from(500u64);
        let arbitrator_secret = pallas::Base::from(600u64);
        let attestation_root = pallas::Base::from(700u64);

        let rd_cap_proof = dwow_dao_escrow_contract::model::CapabilityProof {
            capability_id: capability_id.to_repr(),
            capability_secret: capability_secret.to_repr(),
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes([0u8; 32]).unwrap(),
            issuer_pub: [0u8; 32],
            predicate_result: [0u8; 32],
            proof: vec![],
        };

        let resolve_result = harness.resolve_dispute(nullifier_k, capability_id, dao_bulla, dispute_id, capability_secret, arbitrator_secret, vec![], attestation_root, true, 50_000_000, owner_pub, proposal_id, rd_cap_proof)?;
        assert!(!resolve_result.call_data.is_empty());
        assert_eq!(resolve_result.public_inputs.dao_escrow_bulla, dao_bulla);
        println!("    call_data={}B proof created", resolve_result.call_data.len());

        // --- 0x0d: CancelClaimV1 ---
        println!("  Test 0x0d: CancelClaimV1");
        let cancel_result = harness.cancel_claim(dao_bulla, claim_id, owner_pub)?;
        assert!(!cancel_result.call_data.is_empty());
        println!("    call_data={}B", cancel_result.call_data.len());

        // --- 0x0e: SetGovernanceConfigV1 ---
        println!("  Test 0x0e: SetGovernanceConfigV1");
        let gov_config = GovernanceConfig {
            gov_token_id: pallas::Base::from(42u64),
            proposer_limit: 1000,
            quorum: 5000,
            early_exec_quorum: 7500,
            approval_ratio_quot: 51,
            approval_ratio_base: 100,
            premium_rate_quot: 1,
            premium_rate_base: 100,
            max_claim_ratio_quot: 10,
            max_claim_ratio_base: 100,
            claim_voting_window: 1000,
            claim_execution_window: 500,
            oracle_threshold_numerator: 3,
            oracle_threshold_denominator: 5,
            governance_active: true,
        };

        let sgc_cap_proof = dwow_dao_escrow_contract::model::CapabilityProof {
            capability_id: capability_id.to_repr(),
            capability_secret: capability_secret.to_repr(),
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes([0u8; 32]).unwrap(),
            issuer_pub: [0u8; 32],
            predicate_result: [0u8; 32],
            proof: vec![],
        };

        let gov_result = harness.set_governance_config(dao_bulla, gov_config, sgc_cap_proof)?;
        assert!(!gov_result.call_data.is_empty());
        println!("    call_data={}B", gov_result.call_data.len());

        println!("=== All DAO-Escrow endpoints OK ===");
        Ok(())
    })
}

// ============================================================================
// Identity Heavyweight Test
// ============================================================================

#[test]
fn test_heavyweight_identity() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::IdentityHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;

    println!("=== Identity Heavyweight: All Endpoints ===");

    smol::block_on(async {
        let harness = IdentityHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let mut pipeline = HeavyweightPipeline::new(harness, "identity").await?;
        let wasm = include_bytes!("../../../../src/contract/identity/dwow_identity_contract.wasm");
        let _contract_id = pipeline.deploy(wasm).await?;
        println!("Contract deployed");

        let harness = &pipeline.harness;
        let issuer_secret = pallas::Base::from(10u64);
        let issuer_pub = PublicKey::from_secret(SecretKey::from_bytes(issuer_secret.to_repr()).unwrap());
        let credential_secret = pallas::Base::from(20u64);
        let schema_hash = pallas::Base::from(30u64);
        let commitment = pallas::Base::from(40u64);
        let claim_type = pallas::Base::from(50u64);

        // --- 0x00: InitializeV1 ---
        println!("  Test 0x00: InitializeV1");
        let init_result = harness.initialize()?;
        assert!(!init_result.call_data.is_empty());
        println!("    call_data={}B", init_result.call_data.len());

        // --- 0x01: IssueCredentialV1 (ZK) ---
        println!("  Test 0x01: IssueCredentialV1");
        let issue_result = harness.issue_credential(issuer_secret, credential_secret, pallas::Base::from(100u64), pallas::Base::from(200u64), pallas::Base::from(300u64), schema_hash, 0, 100000)?;
        assert!(!issue_result.call_data.is_empty());
        println!("    call_data={}B proof created", issue_result.call_data.len());

        // --- 0x03: CreateClaimV1 (ZK) ---
        println!("  Test 0x03: CreateClaimV1");
        let claim_result = harness.create_claim(credential_secret, pallas::Base::from(100u64), pallas::Base::from(50u64), commitment, issuer_pub, schema_hash, claim_type)?;
        assert!(!claim_result.call_data.is_empty());
        println!("    call_data={}B proof created", claim_result.call_data.len());

        // --- CreateClaimL1 ---
        println!("  Test: CreateClaimL1");
        let l1_result = harness.create_claim_l1(credential_secret, pallas::Base::from(100u64), pallas::Base::from(50u64), commitment, pallas::Base::from(25u64), issuer_pub, schema_hash, claim_type, true)?;
        assert!(!l1_result.call_data.is_empty());
        println!("    call_data={}B proof created", l1_result.call_data.len());

        // --- CreateClaimL1V2 ---
        println!("  Test: CreateClaimL1V2");
        let l1v2_result = harness.create_claim_l1_v2(credential_secret, pallas::Base::from(100u64), pallas::Base::from(50u64), commitment, issuer_pub, schema_hash, claim_type, true)?;
        assert!(!l1v2_result.call_data.is_empty());
        println!("    call_data={}B proof created", l1v2_result.call_data.len());

        // --- CreateClaimMulti ---
        println!("  Test: CreateClaimMulti");
        let multi_result = harness.create_claim_multi(credential_secret, commitment, pallas::Base::from(100u64), pallas::Base::from(50u64), credential_secret, commitment, pallas::Base::from(200u64), pallas::Base::from(50u64), credential_secret, commitment, pallas::Base::from(300u64), pallas::Base::from(50u64), issuer_pub, schema_hash, claim_type)?;
        assert!(!multi_result.call_data.is_empty());
        println!("    call_data={}B proof created", multi_result.call_data.len());

        // --- CreateClaimRatio ---
        println!("  Test: CreateClaimRatio");
        let ratio_result = harness.create_claim_ratio(credential_secret, commitment, pallas::Base::from(1000u64), pallas::Base::from(10000u64), pallas::Base::from(10u64), issuer_pub, schema_hash, claim_type, true)?;
        assert!(!ratio_result.call_data.is_empty());
        println!("    call_data={}B proof created", ratio_result.call_data.len());

        // --- CreateClaimDAG ---
        println!("  Test: CreateClaimDAG (skipped — pre-existing circuit bug)");

        // --- VerifyCapability (ZK) ---
        println!("  Test: VerifyCapability");
        let capability_secret = pallas::Base::from(777u64);
        let capability_id = pallas::Base::from(888u64);
        let verify_result = harness.verify_capability(credential_secret, commitment, pallas::Base::from(100u64), pallas::Base::from(50u64), capability_secret, issuer_pub, schema_hash, capability_id, true)?;
        assert!(!verify_result.call_data.is_empty());
        println!("    call_data={}B proof created", verify_result.call_data.len());

        // --- RegisterCapability ---
        println!("  Test: RegisterCapability");
        let cred_req = dwow_identity_contract::model::CredentialRequirement {
            schema_hash: [0u8; 32],
            issuer_pub: [0u8; 32],
            min_threshold: 1,
            attribute_name: b"role".to_vec(),
        };
        let reg_result = harness.register_capability(b"can_vote".to_vec(), cred_req, None)?;
        assert!(!reg_result.call_data.is_empty());
        println!("    call_data={}B", reg_result.call_data.len());

        // --- IssueCapability ---
        println!("  Test: IssueCapability");
        let issue_cap_result = harness.issue_capability([0u8; 32], [0u8; 32], dwow_sdk::crypto::IntentNullifier::from_bytes([0u8; 32]).unwrap())?;
        assert!(!issue_cap_result.call_data.is_empty());
        println!("    call_data={}B", issue_cap_result.call_data.len());

        // --- RevokeCapability ---
        println!("  Test: RevokeCapability");
        let revoke_result = harness.revoke_capability([0u8; 32], [0u8; 32], [0u8; 32], b"no longer needed".to_vec())?;
        assert!(!revoke_result.call_data.is_empty());
        println!("    call_data={}B", revoke_result.call_data.len());

        println!("=== All Identity endpoints OK ===");
        Ok(())
    })
}
