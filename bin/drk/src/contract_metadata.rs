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

//! Contract Metadata Registry
//!
//! Static definitions of all known contract functions for universal contract interaction.
//! WASM contracts don't support runtime introspection, so we use static metadata.
//!
//! ## Design Rationale
//!
//! - **Static metadata**: WASM contracts don't expose function enumeration at runtime
//! - **Binary serialization**: Params use `SerialEncodable`, not JSON - we deserialize from JSON
//!   into the native type, then serialize to binary for the contract call
//! - **ZK proof generation**: Contract-specific, handled per-function in `invoke_contract`

use std::collections::HashMap;

/// Represents a single contract function signature
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Human-readable function name (e.g., "initialize", "transfer")
    pub name: &'static str,
    /// Function code byte used in contract call data
    pub code: u8,
    /// Whether this function requires ZK proof generation
    pub requires_proof: bool,
    /// Name of the proof circuit for ZK proof generation (e.g., "init_v1", "transfer_v1")
    pub proof_circuit: Option<&'static str>,
}

/// Metadata for a single contract containing all its functions
#[derive(Debug, Clone)]
pub struct ContractMetadata {
    /// Human-readable contract name (e.g., "dao_escrow", "money_v3")
    pub name: &'static str,
    /// List of all functions this contract supports
    pub functions: Vec<FunctionSignature>,
}

impl ContractMetadata {
    /// Look up a function by name
    pub fn get_function(&self, name: &str) -> Option<&FunctionSignature> {
        self.functions.iter().find(|f| f.name == name)
    }
}

/// Registry of all known contracts and their functions
pub struct ContractMetadataRegistry {
    /// Map from contract name to contract metadata
    contracts: HashMap<&'static str, ContractMetadata>,
}

impl ContractMetadataRegistry {
    /// Create a new registry with all known contracts pre-registered
    pub fn new() -> Self {
        let mut registry = Self { contracts: HashMap::new() };
        registry.register_known_contracts();
        registry
    }

    /// Register all known DarkWow contracts
	fn register_known_contracts(&mut self) {
		// Money V3 Contract (DeFi tokens / ERC-20 style)
		let money_v3 = ContractMetadata {
			name: "money_v3",
			functions: vec![
				FunctionSignature { name: "token_mint", code: 0x00, requires_proof: true, proof_circuit: Some("token_mint_v1") },
				FunctionSignature { name: "mint", code: 0x01, requires_proof: true, proof_circuit: Some("mint_v1") },
				FunctionSignature { name: "burn", code: 0x02, requires_proof: true, proof_circuit: Some("burn_v1") },
				FunctionSignature { name: "transfer", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "otc_swap", code: 0x04, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("money_v3", money_v3);

		// Native Token Contract (DARK token - fees and native operations)
		let native_token = ContractMetadata {
			name: "native_token",
			functions: vec![
				FunctionSignature { name: "fee", code: 0x00, requires_proof: true, proof_circuit: Some("fee_v1") },
				FunctionSignature { name: "mint", code: 0x01, requires_proof: true, proof_circuit: Some("mint_v1") },
				FunctionSignature { name: "burn", code: 0x02, requires_proof: true, proof_circuit: Some("burn_v1") },
				FunctionSignature { name: "transfer", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "spend", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "pow_reward", code: 0x05, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("native_token", native_token);

		// DAO-Escrow Contract
		let dao_escrow = ContractMetadata {
			name: "dao_escrow",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: true, proof_circuit: Some("init_v1") },
				FunctionSignature { name: "update", code: 0x01, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "pay_premium", code: 0x02, requires_proof: true, proof_circuit: Some("pay_premium_v1") },
				FunctionSignature { name: "withdraw", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "endowment_withdraw", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "treasury_spend", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "enable_drain_protection", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "propose_claim", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "vote_claim", code: 0x08, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("dao_escrow", dao_escrow);

		// Deployooor Contract
		let deployooor = ContractMetadata {
			name: "deployooor",
			functions: vec![
				FunctionSignature { name: "deploy", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "lock", code: 0x01, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("deployooor", deployooor);

		// DEX Contract (token swaps — privacy is user-selectable via set_transparency_level)
		let dex = ContractMetadata {
			name: "dex",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_swap", code: 0x01, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "accept_swap", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "execute_swap", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "cancel_swap", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "update_config", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "set_transparency_level", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "execute_swap_fee", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "execute_swap_slippage", code: 0x08, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("dex", dex);

		// Auction Contract
		let auction = ContractMetadata {
			name: "auction",
			functions: vec![
				FunctionSignature { name: "create_auction", code: 0x00, requires_proof: true, proof_circuit: Some("create_auction_v1") },
				FunctionSignature { name: "place_bid", code: 0x01, requires_proof: true, proof_circuit: Some("place_bid_v1") },
				FunctionSignature { name: "close_auction", code: 0x02, requires_proof: true, proof_circuit: Some("close_auction_v1") },
				FunctionSignature { name: "claim_winnings", code: 0x03, requires_proof: true, proof_circuit: Some("claim_winnings_v1") },
				FunctionSignature { name: "settle_auction", code: 0x04, requires_proof: true, proof_circuit: Some("settle_auction_v1") },
				FunctionSignature { name: "refund_bid", code: 0x05, requires_proof: true, proof_circuit: Some("refund_bid_v1") },
			],
		};
		self.contracts.insert("auction", auction);

		// Stablecoin Contract (CDP collateralized debt position)
		let stablecoin = ContractMetadata {
			name: "stablecoin",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: true, proof_circuit: Some("init_v1") },
				FunctionSignature { name: "open_position", code: 0x01, requires_proof: true, proof_circuit: Some("open_position_v1") },
				FunctionSignature { name: "add_collateral", code: 0x02, requires_proof: true, proof_circuit: Some("add_collateral_v1") },
				FunctionSignature { name: "remove_collateral", code: 0x03, requires_proof: true, proof_circuit: Some("remove_collateral_v1") },
				FunctionSignature { name: "mint_stable", code: 0x04, requires_proof: true, proof_circuit: Some("mint_stable_v1") },
				FunctionSignature { name: "repay_stable", code: 0x05, requires_proof: true, proof_circuit: Some("repay_stable_v1") },
				FunctionSignature { name: "liquidate", code: 0x06, requires_proof: true, proof_circuit: Some("liquidate_v1") },
				FunctionSignature { name: "update_config", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "governance_report", code: 0x08, requires_proof: true, proof_circuit: Some("governance_report_v1") },
				FunctionSignature { name: "accrue_interest", code: 0x09, requires_proof: true, proof_circuit: Some("accrue_interest_v1") },
			],
		};
		self.contracts.insert("stablecoin", stablecoin);

		// DrainProtection Contract
		let drain_protection = ContractMetadata {
			name: "drain_protection",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "propose", code: 0x01, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "vote", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "execute", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "exit", code: 0x04, requires_proof: true, proof_circuit: Some("exit_v1") },
				FunctionSignature { name: "transfer", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "lock", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "unlock", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "update_config", code: 0x08, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("drain_protection", drain_protection);

		// Attestation Contract
		let attestation = ContractMetadata {
			name: "attestation",
			functions: vec![
				FunctionSignature { name: "create_attestation", code: 0x00, requires_proof: true, proof_circuit: Some("create_attestation_v1") },
				FunctionSignature { name: "revoke_attestation", code: 0x01, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "expire_attestation", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_claim", code: 0x03, requires_proof: true, proof_circuit: Some("create_claim_v1") },
				FunctionSignature { name: "verify_claim", code: 0x04, requires_proof: true, proof_circuit: Some("verify_claim_v1") },
				FunctionSignature { name: "consume_claim", code: 0x05, requires_proof: true, proof_circuit: Some("consume_claim_v1") },
				FunctionSignature { name: "validate_claim", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "check_not_revoked", code: 0x07, requires_proof: true, proof_circuit: Some("check_not_revoked_v1") },
				FunctionSignature { name: "delegate_attestation", code: 0x08, requires_proof: true, proof_circuit: Some("delegate_attestation_v1") },
				FunctionSignature { name: "verify_chain", code: 0x09, requires_proof: true, proof_circuit: Some("verify_chain_v1") },
				FunctionSignature { name: "update_delegation", code: 0x0a, requires_proof: true, proof_circuit: Some("update_delegation_v1") },
			],
		};
		self.contracts.insert("attestation", attestation);

		// Baccarat Contract (provably fair baccarat game)
		let baccarat = ContractMetadata {
			name: "baccarat",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "commit_bet", code: 0x01, requires_proof: true, proof_circuit: Some("commit_bet_v1") },
				FunctionSignature { name: "draw_cards", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "settle_bet", code: 0x03, requires_proof: true, proof_circuit: Some("settle_bet_v1") },
				FunctionSignature { name: "house_close", code: 0x04, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("baccarat", baccarat);

		// BettingStake Contract
		let betting_stake = ContractMetadata {
			name: "betting_stake",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: true, proof_circuit: Some("init_v1") },
				FunctionSignature { name: "stake", code: 0x01, requires_proof: true, proof_circuit: Some("stake_v1") },
				FunctionSignature { name: "unstake", code: 0x02, requires_proof: true, proof_circuit: Some("unstake_v1") },
				FunctionSignature { name: "claim_earnings", code: 0x03, requires_proof: true, proof_circuit: Some("claim_v1") },
				FunctionSignature { name: "update_risk", code: 0x04, requires_proof: true, proof_circuit: Some("update_risk_v1") },
			],
		};
		self.contracts.insert("betting_stake", betting_stake);

		// Bridge Contract (cross-chain bridge)
		let bridge = ContractMetadata {
			name: "bridge",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "deposit", code: 0x01, requires_proof: true, proof_circuit: Some("deposit_v1") },
				FunctionSignature { name: "withdraw", code: 0x02, requires_proof: true, proof_circuit: Some("withdraw_v1") },
				FunctionSignature { name: "update_config", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "cancel_withdraw", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "execute_guaranteed_withdraw", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_htlc", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "claim_htlc", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "refund_htlc", code: 0x08, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("bridge", bridge);

		// Darkbet Exchange Contract (prediction market)
		let darkbet_exchange = ContractMetadata {
			name: "darkbet_exchange",
			functions: vec![
				FunctionSignature { name: "create_market", code: 0x00, requires_proof: true, proof_circuit: Some("create_market_v1") },
				FunctionSignature { name: "place_back", code: 0x01, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "place_lay", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "match_orders", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "resolve_market", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "settle_market", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "cancel_order", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "buy_position", code: 0x07, requires_proof: true, proof_circuit: Some("buy_position_v1") },
				FunctionSignature { name: "add_liquidity", code: 0x08, requires_proof: true, proof_circuit: Some("add_liquidity_v1") },
				FunctionSignature { name: "remove_liquidity", code: 0x09, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "claim_winnings", code: 0x0A, requires_proof: true, proof_circuit: Some("claim_winnings_v1") },
			],
		};
		self.contracts.insert("darkbet_exchange", darkbet_exchange);

		// Darktoshi Dice Contract (provably fair dice game)
		let darktoshi_dice = ContractMetadata {
			name: "darktoshi_dice",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "commit_bet", code: 0x01, requires_proof: true, proof_circuit: Some("commit_bet_v1") },
				FunctionSignature { name: "reveal_roll", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "settle_bet", code: 0x03, requires_proof: true, proof_circuit: Some("settle_bet_v1") },
				FunctionSignature { name: "house_close", code: 0x04, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("darktoshi_dice", darktoshi_dice);

		// Escrow Contract
		let escrow = ContractMetadata {
			name: "escrow",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_escrow", code: 0x01, requires_proof: true, proof_circuit: Some("create_escrow_v1") },
				FunctionSignature { name: "fund", code: 0x02, requires_proof: true, proof_circuit: Some("fund_v1") },
				FunctionSignature { name: "claim", code: 0x03, requires_proof: true, proof_circuit: Some("claim_v1") },
				FunctionSignature { name: "refund", code: 0x04, requires_proof: true, proof_circuit: Some("refund_v1") },
				FunctionSignature { name: "cancel", code: 0x05, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("escrow", escrow);

		// GameRoom Contract (multi-player game room)
		let game_room = ContractMetadata {
			name: "game_room",
			functions: vec![
				FunctionSignature { name: "create_room", code: 0x00, requires_proof: true, proof_circuit: Some("create_room_v1") },
				FunctionSignature { name: "deposit", code: 0x01, requires_proof: true, proof_circuit: Some("deposit_v1") },
				FunctionSignature { name: "withdraw", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "place_bet", code: 0x03, requires_proof: true, proof_circuit: Some("place_bet_v1") },
				FunctionSignature { name: "raise", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "call", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "fold", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "close_pot", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "settle_pot", code: 0x08, requires_proof: true, proof_circuit: Some("settle_pot_v1") },
				FunctionSignature { name: "contribute_entropy", code: 0x09, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "claim", code: 0x0A, requires_proof: true, proof_circuit: Some("claim_v1") },
			],
		};
		self.contracts.insert("game_room", game_room);

		// Identity Contract (credentials and capabilities)
		let identity = ContractMetadata {
			name: "identity",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "issue_credential", code: 0x01, requires_proof: true, proof_circuit: Some("issue_credential_v1") },
				FunctionSignature { name: "revoke_credential", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_claim", code: 0x03, requires_proof: true, proof_circuit: Some("create_claim_v1") },
				FunctionSignature { name: "verify_claim", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_claim_l1", code: 0x05, requires_proof: true, proof_circuit: Some("create_claim_v1_l1") },
				FunctionSignature { name: "create_claim_l1_v2", code: 0x06, requires_proof: true, proof_circuit: Some("create_claim_v1_l1_v2") },
				FunctionSignature { name: "create_claim_multi", code: 0x07, requires_proof: true, proof_circuit: Some("create_claim_v1_multi") },
				FunctionSignature { name: "create_claim_ratio", code: 0x08, requires_proof: true, proof_circuit: Some("create_claim_v1_ratio") },
				FunctionSignature { name: "register_capability", code: 0x09, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "issue_capability", code: 0x0a, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "verify_capability", code: 0x0b, requires_proof: true, proof_circuit: Some("verify_capability_v1") },
				FunctionSignature { name: "revoke_capability", code: 0x0c, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_claim_dag", code: 0x0d, requires_proof: true, proof_circuit: Some("create_claim_v1_dag") },
			],
		};
		self.contracts.insert("identity", identity);

		// InsuranceMarket Contract
		let insurance_market = ContractMetadata {
			name: "insurance_market",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "register_risk_type", code: 0x01, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_market", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "underwrite", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "purchase_coverage", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "file_claim", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "resolve_claim", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "withdraw_premium", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "update_premium", code: 0x08, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "underwrite_with_capability", code: 0x09, requires_proof: true, proof_circuit: Some("underwrite_with_capability_v1") },
				FunctionSignature { name: "purchase_coverage_with_capability", code: 0x0a, requires_proof: true, proof_circuit: Some("purchase_coverage_with_capability_v1") },
				FunctionSignature { name: "purchase_coverage_with_dag", code: 0x0b, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "resolve_claim_with_capability", code: 0x0c, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("insurance_market", insurance_market);

		// LaborMarket Contract
		let labor_market = ContractMetadata {
			name: "labor_market",
			functions: vec![
				FunctionSignature { name: "create_job", code: 0x00, requires_proof: true, proof_circuit: Some("create_job_v1") },
				FunctionSignature { name: "accept_job", code: 0x01, requires_proof: true, proof_circuit: Some("accept_job_v1") },
				FunctionSignature { name: "submit_deliverable", code: 0x02, requires_proof: true, proof_circuit: Some("submit_deliverable_v1") },
				FunctionSignature { name: "submit_git_deliverable", code: 0x03, requires_proof: true, proof_circuit: Some("submit_git_deliverable_v1") },
				FunctionSignature { name: "confirm_delivery", code: 0x04, requires_proof: true, proof_circuit: Some("confirm_delivery_v1") },
				FunctionSignature { name: "dispute", code: 0x05, requires_proof: true, proof_circuit: Some("dispute_v1") },
				FunctionSignature { name: "refund", code: 0x06, requires_proof: true, proof_circuit: Some("refund_v1") },
				FunctionSignature { name: "cancel", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_job_with_milestones", code: 0x08, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "submit_milestone", code: 0x09, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "confirm_milestone", code: 0x0a, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "initiate_dispute", code: 0x0b, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_job_with_capability", code: 0x0c, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "accept_job_with_capability", code: 0x0d, requires_proof: true, proof_circuit: Some("accept_job_with_capability_v1") },
				FunctionSignature { name: "create_job_with_milestones_and_capability", code: 0x0e, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("labor_market", labor_market);

		// Lottery Contract (provably fair lottery)
		let lottery = ContractMetadata {
			name: "lottery",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "buy_ticket", code: 0x01, requires_proof: true, proof_circuit: Some("commit_ticket_v1") },
				FunctionSignature { name: "draw_winners", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "reveal_ticket", code: 0x03, requires_proof: true, proof_circuit: Some("reveal_ticket_v1") },
				FunctionSignature { name: "claim_prize", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "expire_lottery", code: 0x05, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("lottery", lottery);

		// Oracle Contract
		let oracle = ContractMetadata {
			name: "oracle",
			functions: vec![
				FunctionSignature { name: "register_oracle", code: 0x00, requires_proof: true, proof_circuit: Some("register_oracle_v1") },
				FunctionSignature { name: "push_value", code: 0x01, requires_proof: true, proof_circuit: Some("push_value_v1") },
				FunctionSignature { name: "attest_value", code: 0x02, requires_proof: true, proof_circuit: Some("attest_value_v1") },
				FunctionSignature { name: "push_value_commitment", code: 0x03, requires_proof: true, proof_circuit: Some("push_value_commitment_v1") },
				FunctionSignature { name: "aggregate", code: 0x04, requires_proof: true, proof_circuit: Some("aggregate_v1") },
			],
		};
		self.contracts.insert("oracle", oracle);

		// PoolStake Contract
		let pool_stake = ContractMetadata {
			name: "pool_stake",
			functions: vec![
				FunctionSignature { name: "create_pool", code: 0x00, requires_proof: true, proof_circuit: Some("create_pool_v1") },
				FunctionSignature { name: "join_pool", code: 0x01, requires_proof: true, proof_circuit: Some("join_pool_v1") },
				FunctionSignature { name: "leave_pool", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "allocate_coverage", code: 0x03, requires_proof: true, proof_circuit: Some("allocate_coverage_v1") },
				FunctionSignature { name: "release_coverage", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "slash_coverage", code: 0x05, requires_proof: true, proof_circuit: Some("slash_coverage_v1") },
				FunctionSignature { name: "claim_fees", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "update_pool_config", code: 0x07, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("pool_stake", pool_stake);

		// RelayerEndowment Contract
		let relayer_endowment = ContractMetadata {
			name: "relayer_endowment",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: true, proof_circuit: Some("initialize_v1") },
				FunctionSignature { name: "deploy_capital", code: 0x01, requires_proof: true, proof_circuit: Some("deploy_capital_v1") },
				FunctionSignature { name: "withdraw_deployment", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "claim_relayer_fees", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "settle_fees", code: 0x04, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "update_config", code: 0x05, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("relayer_endowment", relayer_endowment);

		// Roulette Contract (provably fair roulette)
		let roulette = ContractMetadata {
			name: "roulette",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "place_bet", code: 0x01, requires_proof: true, proof_circuit: Some("place_bet_v1") },
				FunctionSignature { name: "spin_wheel", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "settle_bets", code: 0x03, requires_proof: true, proof_circuit: Some("settle_bet_v1") },
				FunctionSignature { name: "house_close", code: 0x04, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("roulette", roulette);

		// Slot Contract (provably fair slot machine)
		let slot = ContractMetadata {
			name: "slot",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "commit_spin", code: 0x01, requires_proof: true, proof_circuit: Some("commit_bet_v1") },
				FunctionSignature { name: "reveal_spin", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "settle_spin", code: 0x03, requires_proof: true, proof_circuit: Some("settle_bet_v1") },
				FunctionSignature { name: "cancel_spin", code: 0x04, requires_proof: false, proof_circuit: None },
			],
		};
		self.contracts.insert("slot", slot);

		// Subscription Contract
		let subscription = ContractMetadata {
			name: "subscription",
			functions: vec![
				FunctionSignature { name: "initialize", code: 0x00, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "subscribe", code: 0x01, requires_proof: true, proof_circuit: Some("subscribe_v1") },
				FunctionSignature { name: "cancel", code: 0x02, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "renew", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "verify_access", code: 0x04, requires_proof: true, proof_circuit: Some("verify_access_v1") },
				FunctionSignature { name: "dao_control", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "update_usage", code: 0x06, requires_proof: true, proof_circuit: Some("update_usage_v1") },
			],
		};
		self.contracts.insert("subscription", subscription);

		// Tender Contract (sealed-bid procurement)
		let tender = ContractMetadata {
			name: "tender",
			functions: vec![
				FunctionSignature { name: "create_tender", code: 0x00, requires_proof: true, proof_circuit: Some("create_tender_v1") },
				FunctionSignature { name: "submit_bid", code: 0x01, requires_proof: true, proof_circuit: Some("submit_bid_v1") },
				FunctionSignature { name: "reveal_bid", code: 0x02, requires_proof: true, proof_circuit: Some("reveal_bid_v1") },
				FunctionSignature { name: "close_tender", code: 0x03, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "select_winner", code: 0x04, requires_proof: true, proof_circuit: Some("select_winner_v1") },
				FunctionSignature { name: "cancel_tender", code: 0x05, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "reject_bid", code: 0x06, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "create_tender_with_capability", code: 0x07, requires_proof: false, proof_circuit: None },
				FunctionSignature { name: "submit_bid_with_capability", code: 0x08, requires_proof: true, proof_circuit: Some("submit_bid_with_capability_v1") },
			],
		};
		self.contracts.insert("tender", tender);
	}

    /// Look up contract metadata by contract name
    pub fn get(&self, name: &str) -> Option<&ContractMetadata> {
        self.contracts.get(name)
    }

    /// Look up a specific function within a contract
    pub fn get_function(&self, contract_name: &str, function_name: &str) -> Option<&FunctionSignature> {
        self.get(contract_name).and_then(|c| c.get_function(function_name))
    }

    /// List all registered contract names
    pub fn contract_names(&self) -> Vec<&'static str> {
        self.contracts.keys().copied().collect()
    }
}

impl Default for ContractMetadataRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global contract metadata registry singleton using lazy initialization
pub static CONTRACT_METADATA_REGISTRY: std::sync::LazyLock<ContractMetadataRegistry> =
    std::sync::LazyLock::new(ContractMetadataRegistry::new);
