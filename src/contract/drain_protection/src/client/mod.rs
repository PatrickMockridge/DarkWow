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

//! DrainProtection contract client API
//!
//! Builder structs for constructing DrainProtection contract calls.
//!
//! ## Usage
//!
//! ```ignore
//! // 1. Initialize a protected fund
//! let init = InitializeBuilder::new()
//!     .fund_id(fund_id)
//!     .spend_authority(owner_key)
//!     .dao_escrow_bulla(bulla)
//!     .build()?;
//!
//! // 2. Propose a large withdrawal
//! let propose = ProposeBuilder::new()
//!     .action(VoteAction::LargeWithdrawal { amount: 1000, recipient: dest })
//!     .prover_pubkey(proposer_key)
//!     .vote_period_blocks(1000)
//!     .build()?;
//!
//! // 3. Vote on a proposal
//! let vote = VoteBuilder::new()
//!     .proposal_id(proposal_id)
//!     .voter_pubkey(voter_key)
//!     .vote(true)
//!     .build()?;
//!
//! // 4. Execute a concluded proposal
//! let exec = ExecuteBuilder::new()
//!     .proposal_id(proposal_id)
//!     .build()?;
//!
//! // 5. Exit with haircut
//! let exit = ExitBuilder::new()
//!     .member_pubkey(member_key)
//!     .contribution_weight(1000)
//!     .current_block(block_height)
//!     .build()?;
//!
//! // 6. Transfer funds (rate-limited)
//! let transfer = TransferBuilder::new()
//!     .amount(500)
//!     .recipient(dest_key)
//!     .exceeds_rate_limit(false)
//!     .build()?;
//!
//! // 7. Lock funds (emergency)
//! let lock = LockBuilder::new()
//!     .duration_blocks(6000)
//!     .build()?;
//!
//! // 8. Unlock funds
//! let unlock = UnlockBuilder::new()
//!     .build()?;
//!
//! // 9. Update configuration
//! let update = UpdateConfigBuilder::new()
//!     .rate_limit(new_rate_limit)
//!     .build()?;
//! ```

use dwow_sdk::{
    crypto::{PublicKey, SecretKey},
    pasta::pallas,
};
#[cfg(feature = "async")]
use dwow_serial::async_trait;
use dwow_serial::{SerialDecodable, SerialEncodable};

use crate::model::{
    DrainConfig, ExitParamsV1, FundId, LockParamsV1, ProposeParamsV1, RateLimit,
    UnlockParamsV1, UpdateConfigParamsV1, VoteAction, VoteParamsV1, VoteThresholds,
};

// ============================================================================
// NOTE: Placeholder implementations
// ============================================================================
//
// The actual ZK proof generation requires the zkas circuit binary files
// which are compiled from the .zk circuit definitions.
//
// These builders are structured to match the expected API once circuits exist.
// ============================================================================

/// Builder for `DrainProtection::InitializeV1`
///
/// Creates a new protected fund with governance controls.
pub struct InitializeBuilder {
    fund_id: FundId,
    spend_authority: PublicKey,
    dao_escrow_bulla: pallas::Base,
    drain_config: DrainConfig,
}

impl InitializeBuilder {
    pub fn new() -> Self {
        Self {
            fund_id: pallas::Base::zero(),
            spend_authority: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
            dao_escrow_bulla: pallas::Base::zero(),
            drain_config: DrainConfig::default(),
        }
    }

    pub fn fund_id(mut self, id: FundId) -> Self {
        self.fund_id = id;
        self
    }

    pub fn spend_authority(mut self, key: PublicKey) -> Self {
        self.spend_authority = key;
        self
    }

    pub fn dao_escrow_bulla(mut self, bulla: pallas::Base) -> Self {
        self.dao_escrow_bulla = bulla;
        self
    }

    pub fn drain_config(mut self, config: DrainConfig) -> Self {
        self.drain_config = config;
        self
    }

    /// Build the initialize call parameters
    pub fn build(&self) -> Result<InitializeParams, &'static str> {
        Ok(InitializeParams {
            fund_id: self.fund_id,
            spend_authority: self.spend_authority,
            dao_escrow_bulla: self.dao_escrow_bulla,
            drain_config: self.drain_config.clone(),
        })
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    pub fund_id: FundId,
    pub spend_authority: PublicKey,
    pub dao_escrow_bulla: pallas::Base,
    pub drain_config: DrainConfig,
}

/// Builder for `DrainProtection::ProposeV1`
///
/// Propose a vote on an action (large withdrawal, lock, authority change).
pub struct ProposeBuilder {
    action: VoteAction,
    prover_pubkey: PublicKey,
    vote_period_blocks: u64,
    proof: Vec<u8>,
}

impl ProposeBuilder {
    pub fn new() -> Self {
        Self {
            action: VoteAction::LockFunds,
            prover_pubkey: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
            vote_period_blocks: 1000,
            proof: vec![],
        }
    }

    pub fn action(mut self, action: VoteAction) -> Self {
        self.action = action;
        self
    }

    pub fn large_withdrawal(mut self, amount: u64, recipient: PublicKey) -> Self {
        self.action = VoteAction::LargeWithdrawal { amount, recipient };
        self
    }

    pub fn lock_funds(mut self) -> Self {
        self.action = VoteAction::LockFunds;
        self
    }

    pub fn unlock_funds(mut self) -> Self {
        self.action = VoteAction::UnlockFunds;
        self
    }

    pub fn change_spend_authority(mut self, new_authority: PublicKey) -> Self {
        self.action = VoteAction::ChangeSpendAuthority { new_authority };
        self
    }

    pub fn renew_lock(mut self) -> Self {
        self.action = VoteAction::RenewLock;
        self
    }

    pub fn prover_pubkey(mut self, key: PublicKey) -> Self {
        self.prover_pubkey = key;
        self
    }

    pub fn vote_period_blocks(mut self, blocks: u64) -> Self {
        self.vote_period_blocks = blocks;
        self
    }

    pub fn proof(mut self, proof: Vec<u8>) -> Self {
        self.proof = proof;
        self
    }

    pub fn build(&self) -> Result<ProposeParamsV1, &'static str> {
        Ok(ProposeParamsV1 {
            action: self.action.clone(),
            prover_pubkey: self.prover_pubkey,
            vote_period_blocks: self.vote_period_blocks,
            proof: self.proof.clone(),
        })
    }
}

/// Builder for `DrainProtection::VoteV1`
///
/// Cast a vote on a pending proposal.
pub struct VoteBuilder {
    proposal_id: pallas::Base,
    voter_pubkey: PublicKey,
    vote: bool,
    signature: pallas::Base,
}

impl VoteBuilder {
    pub fn new() -> Self {
        Self {
            proposal_id: pallas::Base::zero(),
            voter_pubkey: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
            vote: true,
            signature: pallas::Base::zero(),
        }
    }

    pub fn proposal_id(mut self, id: pallas::Base) -> Self {
        self.proposal_id = id;
        self
    }

    pub fn voter_pubkey(mut self, key: PublicKey) -> Self {
        self.voter_pubkey = key;
        self
    }

    pub fn vote(mut self, yes: bool) -> Self {
        self.vote = yes;
        self
    }

    pub fn yes(mut self) -> Self {
        self.vote = true;
        self
    }

    pub fn no(mut self) -> Self {
        self.vote = false;
        self
    }

    pub fn signature(mut self, sig: pallas::Base) -> Self {
        self.signature = sig;
        self
    }

    pub fn build(&self) -> Result<VoteParamsV1, &'static str> {
        Ok(VoteParamsV1 {
            proposal_id: self.proposal_id,
            voter_pubkey: self.voter_pubkey,
            vote: self.vote,
            signature: self.signature,
        })
    }
}

/// Builder for `DrainProtection::ExecuteV1`
///
/// Execute a concluded proposal.
pub struct ExecuteBuilder {
    proposal_id: pallas::Base,
    signature: pallas::Base,
}

impl ExecuteBuilder {
    pub fn new() -> Self {
        Self {
            proposal_id: pallas::Base::zero(),
            signature: pallas::Base::zero(),
        }
    }

    pub fn proposal_id(mut self, id: pallas::Base) -> Self {
        self.proposal_id = id;
        self
    }

    pub fn signature(mut self, sig: pallas::Base) -> Self {
        self.signature = sig;
        self
    }

    pub fn build(&self) -> Result<ExecuteParams, &'static str> {
        Ok(ExecuteParams { proposal_id: self.proposal_id, signature: self.signature })
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteParams {
    pub proposal_id: pallas::Base,
    pub signature: pallas::Base,
}

/// Builder for `DrainProtection::ExitV1`
///
/// Exit the fund with a haircut (any member, any time).
pub struct ExitBuilder {
    fund_id: pallas::Base,
    member_pubkey: PublicKey,
    contribution_weight: u64,
    current_block: u64,
    proof: Vec<u8>,
}

impl ExitBuilder {
    pub fn new() -> Self {
        Self {
            fund_id: pallas::Base::zero(),
            member_pubkey: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
            contribution_weight: 0,
            current_block: 0,
            proof: vec![],
        }
    }

    pub fn fund_id(mut self, id: pallas::Base) -> Self {
        self.fund_id = id;
        self
    }

    pub fn member_pubkey(mut self, key: PublicKey) -> Self {
        self.member_pubkey = key;
        self
    }

    pub fn contribution_weight(mut self, weight: u64) -> Self {
        self.contribution_weight = weight;
        self
    }

    pub fn current_block(mut self, block: u64) -> Self {
        self.current_block = block;
        self
    }

    pub fn proof(mut self, proof: Vec<u8>) -> Self {
        self.proof = proof;
        self
    }

    pub fn build(&self) -> Result<ExitParamsV1, &'static str> {
        Ok(ExitParamsV1 {
            fund_id: self.fund_id,
            member_pubkey: self.member_pubkey,
            contribution_weight: self.contribution_weight,
            current_block: self.current_block,
            proof: self.proof.clone(),
        })
    }
}

/// Builder for `DrainProtection::TransferV1`
///
/// Transfer funds with rate limiting.
pub struct TransferBuilder {
    amount: u64,
    recipient: PublicKey,
    signature: pallas::Base,
    exceeds_rate_limit: bool,
    vote_proposal_id: Option<pallas::Base>,
}

impl TransferBuilder {
    pub fn new() -> Self {
        Self {
            amount: 0,
            recipient: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
            signature: pallas::Base::zero(),
            exceeds_rate_limit: false,
            vote_proposal_id: None,
        }
    }

    pub fn amount(mut self, amount: u64) -> Self {
        self.amount = amount;
        self
    }

    pub fn recipient(mut self, key: PublicKey) -> Self {
        self.recipient = key;
        self
    }

    pub fn signature(mut self, sig: pallas::Base) -> Self {
        self.signature = sig;
        self
    }

    pub fn exceeds_rate_limit(mut self, exceeds: bool) -> Self {
        self.exceeds_rate_limit = exceeds;
        self
    }

    pub fn vote_proposal_id(mut self, id: Option<pallas::Base>) -> Self {
        self.vote_proposal_id = id;
        self
    }

    pub fn build(&self) -> Result<TransferParams, &'static str> {
        Ok(TransferParams {
            amount: self.amount,
            recipient: self.recipient,
            signature: self.signature,
            exceeds_rate_limit: self.exceeds_rate_limit,
            vote_proposal_id: self.vote_proposal_id,
        })
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferParams {
    pub amount: u64,
    pub recipient: PublicKey,
    pub signature: pallas::Base,
    pub exceeds_rate_limit: bool,
    pub vote_proposal_id: Option<pallas::Base>,
}

/// Builder for `DrainProtection::LockV1`
///
/// Lock funds in emergency state.
pub struct LockBuilder {
    duration_blocks: u64,
    signature: pallas::Base,
}

impl LockBuilder {
    pub fn new() -> Self {
        Self { duration_blocks: 6000, signature: pallas::Base::zero() }
    }

    pub fn duration_blocks(mut self, blocks: u64) -> Self {
        self.duration_blocks = blocks;
        self
    }

    pub fn signature(mut self, sig: pallas::Base) -> Self {
        self.signature = sig;
        self
    }

    pub fn build(&self) -> Result<LockParamsV1, &'static str> {
        Ok(LockParamsV1 { duration_blocks: self.duration_blocks, signature: self.signature })
    }
}

/// Builder for `DrainProtection::UnlockV1`
///
/// Unlock funds after timelock.
pub struct UnlockBuilder {
    signature: pallas::Base,
}

impl UnlockBuilder {
    pub fn new() -> Self {
        Self { signature: pallas::Base::zero() }
    }

    pub fn signature(mut self, sig: pallas::Base) -> Self {
        self.signature = sig;
        self
    }

    pub fn build(&self) -> Result<UnlockParamsV1, &'static str> {
        Ok(UnlockParamsV1 { signature: self.signature })
    }
}

/// Builder for `DrainProtection::UpdateConfigV1`
///
/// Update fund configuration parameters.
pub struct UpdateConfigBuilder {
    rate_limit: Option<RateLimit>,
    thresholds: Option<VoteThresholds>,
    new_spend_authority: Option<PublicKey>,
}

impl UpdateConfigBuilder {
    pub fn new() -> Self {
        Self { rate_limit: None, thresholds: None, new_spend_authority: None }
    }

    pub fn rate_limit(mut self, limit: RateLimit) -> Self {
        self.rate_limit = Some(limit);
        self
    }

    pub fn thresholds(mut self, thresh: VoteThresholds) -> Self {
        self.thresholds = Some(thresh);
        self
    }

    pub fn new_spend_authority(mut self, key: PublicKey) -> Self {
        self.new_spend_authority = Some(key);
        self
    }

    pub fn build(&self) -> Result<UpdateConfigParamsV1, &'static str> {
        Ok(UpdateConfigParamsV1 {
            rate_limit: self.rate_limit.clone(),
            thresholds: self.thresholds.clone(),
            new_spend_authority: self.new_spend_authority,
        })
    }
}