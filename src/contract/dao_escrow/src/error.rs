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

//! DAO-Escrow contract errors.
//!
//! **One variant per refusal the contract can actually produce.** The enum used to carry 55 variants for
//! a contract with 17 endpoints, of which 29 were constructed nowhere — measured, not guessed: a script
//! that matched every `DaoEscrowError::X` across `src/` against `From<DaoEscrowError>`'s arms found 26
//! live and 29 dead. The dead half was the vocabulary of the retired models — the OCap registry
//! (`CapabilityRequirementNotRegistered`, `CapabilityVerificationFailed`, `InvalidCapabilityForAction`,
//! `CapabilityExpired`), the oracle/dispute path (`DisputeNotFound`, `DisputeAlreadyResolved`,
//! `OracleThresholdNotMet`, `InvalidAttestationRef`, `AttestationAlreadyConsumed`), and the quorum
//! arithmetic the group's `FinalizeV1` now performs (`QuorumNotMet`, `InvalidQuorum`,
//! `ApprovalRatioNotMet`, `InvalidApprovalRatio`) — plus general-purpose ones nothing ever reached
//! (`DoubleSpend`, `InvalidZkProof`, `InvalidSignature`, `InvalidCommitment`, `InvalidNullifier`).
//!
//! **Removing a variant does not renumber anything**: every arm below maps to an explicit
//! `ContractError::Custom(N)`, so the codes that stay keep their values and the codes that go are left
//! *unmapped* rather than reused. That is the rule the retirement of `NotClaimProposer` (19) set: a code
//! that has been recorded keeps meaning what it meant.
//!
//! The value of the reduction is that the enum is now an auditable statement of what this contract
//! refuses — "a rejection names its cause" can be checked by reading it, which it could not when two
//! thirds of it named causes the contract could not reach.

use dwow_sdk::error::ContractError;

/// DAO-Escrow contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum DaoEscrowError {
    #[error("DAO-Escrow not found: {0}")]
    DaoEscrowNotFound(String),

    #[error("DAO-Escrow already exists: {0}")]
    DaoEscrowAlreadyExists(String),

    #[error("Invalid state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("Claim not found: {0}")]
    ClaimNotFound(String),

    #[error("Claim already exists: {0}")]
    ClaimAlreadyExists(String),

    #[error("Claim not pending")]
    ClaimNotPending,

    #[error("Claim already approved")]
    ClaimAlreadyApproved,

    #[error("Claim already rejected")]
    ClaimAlreadyRejected,

    #[error("Claim already executed")]
    ClaimAlreadyExecuted,

    #[error("Claim already cancelled")]
    ClaimAlreadyCancelled,

    #[error("Premium below the endowment's minimum")]
    InsufficientPremium,

    #[error("Unauthorized: not DAO-Escrow owner")]
    NotOwner,

    #[error("Unauthorized: not authorized to withdraw")]
    NotAuthorizedToWithdraw,

    #[error("Already voted on this claim")]
    AlreadyVoted,

    #[error("Claim proposal expired")]
    ClaimExpired,

    #[error("Claim execution deadline passed")]
    ClaimExecutionDeadlinePassed,

    #[error("Invalid children indexes: expected promissory_note::transfer_v1 call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call: expected promissory_note::transfer_v1")]
    InvalidChildCall,

    #[error("Proposal not found: {0}")]
    ProposalNotFound(String),

    #[error("Proposal not in pending state")]
    ProposalNotPending,

    #[error("Governance not active")]
    GovernanceNotActive,

    #[error("Proposal already executed")]
    ProposalAlreadyExecuted,

    // ── Governance approvals (`OBL-C151`). Appended after 52 so no existing `Custom(N)` moves: the
    // register cites `Custom(43)` for `GovernanceNotActive`.
    #[error("Governance approval names a different group than the endowment's")]
    GovernanceApprovalForeignGroup,

    #[error("Governance approval names a different action than the one being authorised")]
    GovernanceApprovalWrongMessage,

    #[error("Governance is already active for this endowment, and there is no rotation")]
    GovernanceAlreadyActive,

    #[error("This ownership proof has already been used")]
    OwnershipProofReplayed,

    /// `UpdateV1` (0x01) exists to install the endowment's governance group — the record's mode, owner
    /// and premium floor are all immutable after `InitializeV1`, so a call naming no group has nothing
    /// to do. **It must be refused rather than allowed to do nothing**, and the reason is the owner's
    /// one-shot proof: `owner_nullifier` is deterministic in `(owner_secret, dao_escrow_bulla)`, and
    /// `update_apply_v1` records it unconditionally. A no-op that reached apply would spend the only
    /// credential that can ever install a group, so governance would be permanently uninstallable on
    /// that endowment (`OBL-C161`). A silent success here is the same shape as a gate that cannot fail.
    #[error("UpdateV1 installs the governance group, and this call names none")]
    NoGovernanceGroup,
}

impl From<DaoEscrowError> for ContractError {
    fn from(e: DaoEscrowError) -> Self {
        match e {
            DaoEscrowError::DaoEscrowNotFound(_) => Self::Custom(2),
            DaoEscrowError::DaoEscrowAlreadyExists(_) => Self::Custom(3),
            DaoEscrowError::InvalidState { .. } => Self::Custom(4),
            DaoEscrowError::ClaimNotFound(_) => Self::Custom(5),
            DaoEscrowError::ClaimAlreadyExists(_) => Self::Custom(6),
            DaoEscrowError::ClaimNotPending => Self::Custom(7),
            DaoEscrowError::ClaimAlreadyApproved => Self::Custom(8),
            DaoEscrowError::ClaimAlreadyRejected => Self::Custom(9),
            DaoEscrowError::ClaimAlreadyExecuted => Self::Custom(10),
            DaoEscrowError::ClaimAlreadyCancelled => Self::Custom(11),
            DaoEscrowError::InsufficientPremium => Self::Custom(13),
            DaoEscrowError::NotOwner => Self::Custom(20),
            DaoEscrowError::NotAuthorizedToWithdraw => Self::Custom(21),
            DaoEscrowError::AlreadyVoted => Self::Custom(23),
            DaoEscrowError::ClaimExpired => Self::Custom(24),
            DaoEscrowError::ClaimExecutionDeadlinePassed => Self::Custom(25),
            DaoEscrowError::InvalidChildrenIndexes => Self::Custom(33),
            DaoEscrowError::InvalidChildCall => Self::Custom(34),
            DaoEscrowError::ProposalNotFound(_) => Self::Custom(37),
            DaoEscrowError::ProposalNotPending => Self::Custom(38),
            DaoEscrowError::GovernanceNotActive => Self::Custom(43),
            DaoEscrowError::ProposalAlreadyExecuted => Self::Custom(50),
            DaoEscrowError::GovernanceApprovalForeignGroup => Self::Custom(53),
            DaoEscrowError::GovernanceApprovalWrongMessage => Self::Custom(54),
            DaoEscrowError::GovernanceAlreadyActive => Self::Custom(55),
            DaoEscrowError::OwnershipProofReplayed => Self::Custom(56),
            DaoEscrowError::NoGovernanceGroup => Self::Custom(57),
        }
    }
}
