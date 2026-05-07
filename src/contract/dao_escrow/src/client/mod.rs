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

//! DAO-Escrow contract client API
//!
//! Builder structs for constructing DAO-Escrow contract calls.
//!
//! ## Usage
//!
//! ```ignore
//! // 1. Initialize a DAO-Escrow
//! let init = InitializeBuilder::new()
//!     .owner_pubkey(owner_key)
//!     .gov_token_id(DRKW_TOKEN_ID)
//!     .proposer_limit(1000)
//!     .quorum(5000)
//!     .build()?;
//!
//! // 2. Pay premiums as a member
//! let premium = PayPremiumBuilder::new()
//!     .dao_escrow_bulla(bulla)
//!     .value(100)
//!     .build()?;
//!
//! // 3. Propose a claim
//! let claim = ProposeClaimBuilder::new()
//!     .dao_escrow_bulla(bulla)
//!     .value(5000)
//!     .description_hash(description_hash)
//!     .recipient_pubkey(recipient)
//!     .build()?;
//!
//! // 4. Vote on a claim
//! let vote = VoteClaimBuilder::new()
//!     .claim_id(claim_id)
//!     .vote(VoteType::Yes)
//!     .build()?;
//!
//! // 5. Execute approved claim
//! let exec = ExecuteClaimBuilder::new()
//!     .claim_id(claim_id)
//!     .build()?;
//! ```

pub mod init_v1;
pub mod pay_premium_v1;

use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};
#[cfg(feature = "async")]
use dwow_serial::async_trait;
use dwow_serial::{SerialDecodable, SerialEncodable};

use crate::model::{
    ClaimId, DaoEscrowBulla, ProposeClaimParamsV1, VoteClaimParamsV1, VoteType,
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

/// Builder for `DaoEscrow::InitializeV1`
///
/// Creates a new DAO-Escrow instance with governance parameters.
pub struct InitializeBuilder {
    owner_pubkey: PublicKey,
    gov_token_id: pallas::Base,
    proposer_limit: u64,
    quorum: u64,
    early_exec_quorum: u64,
    approval_ratio_quot: u64,
    approval_ratio_base: u64,
    premium_rate_quot: u64,
    premium_rate_base: u64,
    max_claim_ratio_quot: u64,
    max_claim_ratio_base: u64,
    claim_voting_window: u64,
    claim_execution_window: u64,
}

impl InitializeBuilder {
    pub fn new() -> Self {
        Self {
            owner_pubkey: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
            gov_token_id: pallas::Base::zero(),
            proposer_limit: 1000,
            quorum: 5000,
            early_exec_quorum: 8000,
            approval_ratio_quot: 51,
            approval_ratio_base: 100,
            premium_rate_quot: 1,
            premium_rate_base: 100,
            max_claim_ratio_quot: 10,
            max_claim_ratio_base: 100,
            claim_voting_window: 1000,
            claim_execution_window: 500,
        }
    }

    pub fn owner_pubkey(mut self, key: PublicKey) -> Self {
        self.owner_pubkey = key;
        self
    }

    pub fn gov_token_id(mut self, id: pallas::Base) -> Self {
        self.gov_token_id = id;
        self
    }

    pub fn proposer_limit(mut self, limit: u64) -> Self {
        self.proposer_limit = limit;
        self
    }

    pub fn quorum(mut self, quorum: u64) -> Self {
        self.quorum = quorum;
        self
    }

    pub fn early_exec_quorum(mut self, quorum: u64) -> Self {
        self.early_exec_quorum = quorum;
        self
    }

    pub fn approval_ratio(mut self, quot: u64, base: u64) -> Self {
        self.approval_ratio_quot = quot;
        self.approval_ratio_base = base;
        self
    }

    pub fn premium_rate(mut self, quot: u64, base: u64) -> Self {
        self.premium_rate_quot = quot;
        self.premium_rate_base = base;
        self
    }

    pub fn max_claim_ratio(mut self, quot: u64, base: u64) -> Self {
        self.max_claim_ratio_quot = quot;
        self.max_claim_ratio_base = base;
        self
    }

    pub fn claim_voting_window(mut self, window: u64) -> Self {
        self.claim_voting_window = window;
        self
    }

    pub fn claim_execution_window(mut self, window: u64) -> Self {
        self.claim_execution_window = window;
        self
    }

    /// Build the initialize call parameters
    pub fn build(&self) -> Result<InitializeParams, &'static str> {
        Ok(InitializeParams {
            owner_pubkey: self.owner_pubkey,
            gov_token_id: self.gov_token_id,
            proposer_limit: self.proposer_limit,
            quorum: self.quorum,
            early_exec_quorum: self.early_exec_quorum,
            approval_ratio_quot: self.approval_ratio_quot,
            approval_ratio_base: self.approval_ratio_base,
            premium_rate_quot: self.premium_rate_quot,
            premium_rate_base: self.premium_rate_base,
            max_claim_ratio_quot: self.max_claim_ratio_quot,
            max_claim_ratio_base: self.max_claim_ratio_base,
            claim_voting_window: self.claim_voting_window,
            claim_execution_window: self.claim_execution_window,
        })
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    pub owner_pubkey: PublicKey,
    pub gov_token_id: pallas::Base,
    pub proposer_limit: u64,
    pub quorum: u64,
    pub early_exec_quorum: u64,
    pub approval_ratio_quot: u64,
    pub approval_ratio_base: u64,
    pub premium_rate_quot: u64,
    pub premium_rate_base: u64,
    pub max_claim_ratio_quot: u64,
    pub max_claim_ratio_base: u64,
    pub claim_voting_window: u64,
    pub claim_execution_window: u64,
}

/// Builder for `DaoEscrow::PayPremiumV1`
///
/// Members pay premiums into the endowment pool.
pub struct PayPremiumBuilder {
    dao_escrow_bulla: DaoEscrowBulla,
    value: u64,
    token_id: pallas::Base,
    period: u64,
}

impl PayPremiumBuilder {
    pub fn new() -> Self {
        Self {
            dao_escrow_bulla: pallas::Base::zero(),
            value: 0,
            token_id: pallas::Base::zero(),
            period: 0,
        }
    }

    pub fn dao_escrow_bulla(mut self, bulla: DaoEscrowBulla) -> Self {
        self.dao_escrow_bulla = bulla;
        self
    }

    pub fn value(mut self, value: u64) -> Self {
        self.value = value;
        self
    }

    pub fn token_id(mut self, id: pallas::Base) -> Self {
        self.token_id = id;
        self
    }

    pub fn period(mut self, period: u64) -> Self {
        self.period = period;
        self
    }

    pub fn build(&self) -> Result<PayPremiumParams, &'static str> {
        Ok(PayPremiumParams {
            dao_escrow_bulla: self.dao_escrow_bulla,
            value: self.value,
            token_id: self.token_id,
            period: self.period,
        })
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PayPremiumParams {
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub value: u64,
    pub token_id: pallas::Base,
    pub period: u64,
}

/// Builder for `DaoEscrow::ProposeClaimV1`
///
/// Members propose claims against the endowment.
pub struct ProposeClaimBuilder {
    dao_escrow_bulla: DaoEscrowBulla,
    claim_id: ClaimId,
    value: u64,
    description_hash: pallas::Base,
    recipient_pubkey: PublicKey,
    proposer_pubkey: PublicKey,
}

impl ProposeClaimBuilder {
    pub fn new() -> Self {
        Self {
            dao_escrow_bulla: pallas::Base::zero(),
            claim_id: pallas::Base::zero(),
            value: 0,
            description_hash: pallas::Base::zero(),
            recipient_pubkey: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
            proposer_pubkey: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
        }
    }

    pub fn dao_escrow_bulla(mut self, bulla: DaoEscrowBulla) -> Self {
        self.dao_escrow_bulla = bulla;
        self
    }

    pub fn claim_id(mut self, id: ClaimId) -> Self {
        self.claim_id = id;
        self
    }

    pub fn value(mut self, value: u64) -> Self {
        self.value = value;
        self
    }

    pub fn description_hash(mut self, hash: pallas::Base) -> Self {
        self.description_hash = hash;
        self
    }

    pub fn recipient_pubkey(mut self, key: PublicKey) -> Self {
        self.recipient_pubkey = key;
        self
    }

    pub fn proposer_pubkey(mut self, key: PublicKey) -> Self {
        self.proposer_pubkey = key;
        self
    }

    pub fn build(&self) -> Result<ProposeClaimParamsV1, &'static str> {
        Ok(ProposeClaimParamsV1 {
            dao_escrow_bulla: self.dao_escrow_bulla,
            claim_id: self.claim_id,
            value: self.value,
            description_hash: self.description_hash,
            recipient_pubkey: self.recipient_pubkey,
            proposer_pubkey: self.proposer_pubkey,
        })
    }
}

/// Builder for `DaoEscrow::VoteClaimV1`
///
/// DAO members vote on pending claims.
pub struct VoteClaimBuilder {
    dao_escrow_bulla: DaoEscrowBulla,
    claim_id: ClaimId,
    vote: VoteType,
    voter_pubkey: PublicKey,
}

impl VoteClaimBuilder {
    pub fn new() -> Self {
        Self {
            dao_escrow_bulla: pallas::Base::zero(),
            claim_id: pallas::Base::zero(),
            vote: VoteType::Yes,
            voter_pubkey: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
        }
    }

    pub fn dao_escrow_bulla(mut self, bulla: DaoEscrowBulla) -> Self {
        self.dao_escrow_bulla = bulla;
        self
    }

    pub fn claim_id(mut self, id: ClaimId) -> Self {
        self.claim_id = id;
        self
    }

    pub fn vote(mut self, vote: VoteType) -> Self {
        self.vote = vote;
        self
    }

    pub fn voter_pubkey(mut self, key: PublicKey) -> Self {
        self.voter_pubkey = key;
        self
    }

    pub fn yes(mut self) -> Self {
        self.vote = VoteType::Yes;
        self
    }

    pub fn no(mut self) -> Self {
        self.vote = VoteType::No;
        self
    }

    pub fn build(&self) -> Result<VoteClaimParamsV1, &'static str> {
        Ok(VoteClaimParamsV1 {
            dao_escrow_bulla: self.dao_escrow_bulla,
            claim_id: self.claim_id,
            vote: self.vote,
            voter_pubkey: self.voter_pubkey,
        })
    }
}

/// Builder for `DaoEscrow::ExecuteClaimV1`
///
/// Execute an approved claim, releasing endowment funds.
pub struct ExecuteClaimBuilder {
    claim_id: ClaimId,
}

impl ExecuteClaimBuilder {
    pub fn new() -> Self {
        Self { claim_id: pallas::Base::zero() }
    }

    pub fn claim_id(mut self, id: ClaimId) -> Self {
        self.claim_id = id;
        self
    }

    pub fn build(&self) -> Result<ExecuteClaimParams, &'static str> {
        Ok(ExecuteClaimParams { claim_id: self.claim_id })
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteClaimParams {
    pub claim_id: ClaimId,
}

/// Builder for `DaoEscrow::CancelClaimV1`
///
/// Proposer cancels their pending claim.
pub struct CancelClaimBuilder {
    claim_id: ClaimId,
    proposer_secret: SecretKey,
}

impl CancelClaimBuilder {
    pub fn new() -> Self {
        Self {
            claim_id: pallas::Base::zero(),
            proposer_secret: SecretKey::random(&mut rand::rngs::OsRng),
        }
    }

    pub fn claim_id(mut self, id: ClaimId) -> Self {
        self.claim_id = id;
        self
    }

    pub fn proposer_secret(mut self, secret: SecretKey) -> Self {
        self.proposer_secret = secret;
        self
    }

    pub fn build(&self) -> Result<CancelClaimParams, &'static str> {
        Ok(CancelClaimParams { claim_id: self.claim_id })
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelClaimParams {
    pub claim_id: ClaimId,
}

/// Builder for `DaoEscrow::WithdrawV1`
///
/// Owner withdraws from endowment (fees, etc.).
pub struct WithdrawBuilder {
    dao_escrow_bulla: DaoEscrowBulla,
    value: u64,
    recipient_pubkey: PublicKey,
}

impl WithdrawBuilder {
    pub fn new() -> Self {
        Self {
            dao_escrow_bulla: pallas::Base::zero(),
            value: 0,
            recipient_pubkey: PublicKey::from_secret(SecretKey::random(&mut rand::rngs::OsRng)),
        }
    }

    pub fn dao_escrow_bulla(mut self, bulla: DaoEscrowBulla) -> Self {
        self.dao_escrow_bulla = bulla;
        self
    }

    pub fn value(mut self, value: u64) -> Self {
        self.value = value;
        self
    }

    pub fn recipient_pubkey(mut self, key: PublicKey) -> Self {
        self.recipient_pubkey = key;
        self
    }

    pub fn build(&self) -> Result<WithdrawParams, &'static str> {
        Ok(WithdrawParams {
            dao_escrow_bulla: self.dao_escrow_bulla,
            value: self.value,
            recipient_pubkey: self.recipient_pubkey,
        })
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawParams {
    pub dao_escrow_bulla: DaoEscrowBulla,
    pub value: u64,
    pub recipient_pubkey: PublicKey,
}
