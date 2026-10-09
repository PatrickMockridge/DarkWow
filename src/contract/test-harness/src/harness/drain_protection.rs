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

//! DrainProtection Test Harness
//!
//! Provides isolated testing for DrainProtection contract.
//!
//! **Every endpoint builds a real proof** (`OBL-C88`). The nine circuits load as before, and each
//! endpoint now proves through the contract's own client — `create_authority_proof` for the eight
//! authority circuits, `create_exit_proof` for `exit` — with params carrying **the same public
//! inputs the proof was made with**. Before this, `make_proof` fabricated a proof with one instance
//! and no advice, so an endpoint could not fail for the reason its name implies.
//!
//! One authority secret for the whole harness, and `initialize` registers **its** point as the
//! fund's `spend_authority` — the two are written to agree, which is what a host-side authority
//! check compares. The contract has no such check today (recorded as `OBL-C97`); the fixture is
//! built this way so that adding one does not require rewriting the fixture.

use dwow_core::{
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_drain_protection_contract::{
    client::{
        create_authority_proof,
        exit::{create_exit_proof, ExitCallData},
        AuthorityCallData,
    },
    model::{
        DrainConfig, ExecuteParamsV1, ExitParamsV1, InitializeParamsV1, LockParamsV1,
        ProposeParamsV1, TransferParamsV1, UnlockParamsV1, UpdateConfigParamsV1, VoteParamsV1,
    },
};
use dwow_sdk::{
    crypto::{PublicKey, SecretKey},
    pasta::pallas,
};

/// DrainProtection Harness for isolated testing
pub struct DrainProtectionHarness {
    /// The deployed contract's id — the frame commitment covers it (`OBL-C198`).
    contract_id: dwow_sdk::crypto::ContractId,
    /// ExitProof ZkBinary
    exit_zkbin: ZkBinary,
    /// ExitProof ProvingKey
    exit_pk: ProvingKey,
    /// ExecuteV1 ZkBinary
    execute_zkbin: ZkBinary,
    /// ExecuteV1 ProvingKey
    execute_pk: ProvingKey,
    /// InitializeV1 ZkBinary
    initialize_zkbin: ZkBinary,
    /// InitializeV1 ProvingKey
    initialize_pk: ProvingKey,
    /// LockV1 ZkBinary
    lock_zkbin: ZkBinary,
    /// LockV1 ProvingKey
    lock_pk: ProvingKey,
    /// ProposeV1 ZkBinary
    propose_zkbin: ZkBinary,
    /// ProposeV1 ProvingKey
    propose_pk: ProvingKey,
    /// TransferV1 ZkBinary
    transfer_zkbin: ZkBinary,
    /// TransferV1 ProvingKey
    transfer_pk: ProvingKey,
    /// UnlockV1 ZkBinary
    unlock_zkbin: ZkBinary,
    /// UnlockV1 ProvingKey
    unlock_pk: ProvingKey,
    /// UpdateConfigV1 ZkBinary
    update_config_zkbin: ZkBinary,
    /// UpdateConfigV1 ProvingKey
    update_config_pk: ProvingKey,
    /// VoteV1 ZkBinary
    vote_zkbin: ZkBinary,
    /// VoteV1 ProvingKey
    vote_pk: ProvingKey,
}

/// A prepared authority call: its call data is fixed, the proof is not yet made (`OBL-C198`).
///
/// The caller cannot supply the frame commitment until the call data exists — the commitment is over
/// the call set, which includes this call — so splitting assembly from proving is what lets a
/// child-bearing endpoint be framed over `[child…, parent]`. `prove` binds the proof to it.
pub struct AuthorityCallPlan {
    /// The encoded call data (selector + params), the same bytes the host will execute.
    pub call_data: Vec<u8>,
    authority: AuthorityCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl AuthorityCallPlan {
    /// Prove the call, binding its proof to `tx_commitment` — the commitment over the whole call set
    /// the transaction carries, this call's children and siblings included.
    pub fn prove(self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<Proof> {
        let authority = self.authority.tx_pair(tx_commitment, tx_nonce);
        let (proof, _pi) = create_authority_proof(&self.zkbin, &self.pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        Ok(proof)
    }
}

/// A prepared `exit` call (`OBL-C198`) — the exit circuit's twin of [`AuthorityCallPlan`]. `exit`
/// proves through `create_exit_proof` rather than `create_authority_proof`.
pub struct ExitCallPlan {
    /// The encoded call data (selector + params).
    pub call_data: Vec<u8>,
    call: ExitCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl ExitCallPlan {
    /// Prove the call, binding its proof to `tx_commitment` (`OBL-C198`).
    pub fn prove(self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<Proof> {
        let call = self.call.tx_pair(tx_commitment, tx_nonce);
        let (proof, _pi) = create_exit_proof(&self.zkbin, &self.pk, &call)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        Ok(proof)
    }
}

impl DrainProtectionHarness {
    /// Spawn a new DrainProtection harness with pre-loaded circuits, for the contract `contract_id`.
    ///
    /// The id is not decoration (`OBL-C198`): a proof now binds the frame commitment, which covers
    /// the call's `ContractCall` — and that carries the contract id — so the fixture and the host
    /// must derive over the same one. It is the id the spec deploys the wasm under.
    pub fn spawn(contract_id: dwow_sdk::crypto::ContractId) -> Self {
        let exit_bin = include_bytes!("../../../drain_protection/proof/exit.zk.bin");
        let execute_bin = include_bytes!("../../../drain_protection/proof/execute.zk.bin");
        let initialize_bin = include_bytes!("../../../drain_protection/proof/initialize.zk.bin");
        let lock_bin = include_bytes!("../../../drain_protection/proof/lock.zk.bin");
        let propose_bin = include_bytes!("../../../drain_protection/proof/propose.zk.bin");
        let transfer_bin = include_bytes!("../../../drain_protection/proof/transfer.zk.bin");
        let unlock_bin = include_bytes!("../../../drain_protection/proof/unlock.zk.bin");
        let update_config_bin = include_bytes!("../../../drain_protection/proof/update_config.zk.bin");
        let vote_bin = include_bytes!("../../../drain_protection/proof/vote.zk.bin");

        let exit_zkbin = ZkBinary::decode(exit_bin, false).unwrap();
        let execute_zkbin = ZkBinary::decode(execute_bin, false).unwrap();
        let initialize_zkbin = ZkBinary::decode(initialize_bin, false).unwrap();
        let lock_zkbin = ZkBinary::decode(lock_bin, false).unwrap();
        let propose_zkbin = ZkBinary::decode(propose_bin, false).unwrap();
        let transfer_zkbin = ZkBinary::decode(transfer_bin, false).unwrap();
        let unlock_zkbin = ZkBinary::decode(unlock_bin, false).unwrap();
        let update_config_zkbin = ZkBinary::decode(update_config_bin, false).unwrap();
        let vote_zkbin = ZkBinary::decode(vote_bin, false).unwrap();

        let exit_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&exit_zkbin).unwrap(),
            &exit_zkbin,
        );
        let execute_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&execute_zkbin).unwrap(),
            &execute_zkbin,
        );
        let initialize_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&initialize_zkbin).unwrap(),
            &initialize_zkbin,
        );
        let lock_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&lock_zkbin).unwrap(),
            &lock_zkbin,
        );
        let propose_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&propose_zkbin).unwrap(),
            &propose_zkbin,
        );
        let transfer_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&transfer_zkbin).unwrap(),
            &transfer_zkbin,
        );
        let unlock_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&unlock_zkbin).unwrap(),
            &unlock_zkbin,
        );
        let update_config_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&update_config_zkbin).unwrap(),
            &update_config_zkbin,
        );
        let vote_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&vote_zkbin).unwrap(),
            &vote_zkbin,
        );

        let exit_pk = ProvingKey::build(exit_zkbin.k, &exit_circuit).expect("ProvingKey::build failed");
        let execute_pk = ProvingKey::build(execute_zkbin.k, &execute_circuit).expect("ProvingKey::build failed");
        let initialize_pk = ProvingKey::build(initialize_zkbin.k, &initialize_circuit).expect("ProvingKey::build failed");
        let lock_pk = ProvingKey::build(lock_zkbin.k, &lock_circuit).expect("ProvingKey::build failed");
        let propose_pk = ProvingKey::build(propose_zkbin.k, &propose_circuit).expect("ProvingKey::build failed");
        let transfer_pk = ProvingKey::build(transfer_zkbin.k, &transfer_circuit).expect("ProvingKey::build failed");
        let unlock_pk = ProvingKey::build(unlock_zkbin.k, &unlock_circuit).expect("ProvingKey::build failed");
        let update_config_pk = ProvingKey::build(update_config_zkbin.k, &update_config_circuit).expect("ProvingKey::build failed");
        let vote_pk = ProvingKey::build(vote_zkbin.k, &vote_circuit).expect("ProvingKey::build failed");

        Self {
            contract_id,
            exit_zkbin,
            exit_pk,
            execute_zkbin,
            execute_pk,
            initialize_zkbin,
            initialize_pk,
            lock_zkbin,
            lock_pk,
            propose_zkbin,
            propose_pk,
            transfer_zkbin,
            transfer_pk,
            unlock_zkbin,
            unlock_pk,
            update_config_zkbin,
            update_config_pk,
            vote_zkbin,
            vote_pk,
        }
    }

    /// The one fund every endpoint acts on, and the one `initialize` creates.
    const FUND_ID: pallas::Base = pallas::Base::from_raw([1, 0, 0, 0]);

    /// The authority secret every endpoint proves with. `initialize` registers its **point** as the
    /// fund's `spend_authority`, so the two agree by construction (`OBL-C97`).
    const AUTHORITY_SECRET: pallas::Base = pallas::Base::from_raw([1234, 0, 0, 0]);

    /// A secret that is not the fund's authority, for the negative control.
    const STRANGER_SECRET: pallas::Base = pallas::Base::from_raw([4321, 0, 0, 0]);

    /// The governing group's **threshold**, and the secrets of the members who join it — three
    /// members, two of whom must approve (`OBL-C101`).
    ///
    /// These are the fixture's, and they are the only place the group is defined: `governance_group`
    /// below derives its id with the multisig contract's own `derive_group_id`, so the id the fund
    /// stores and the id the group signs under are one value computed once.
    pub const GOVERNANCE_THRESHOLD: u8 = 2;
    pub const GOVERNANCE_MEMBERS: [pallas::Base; 3] = [
        pallas::Base::from_raw([11, 0, 0, 0]),
        pallas::Base::from_raw([12, 0, 0, 0]),
        pallas::Base::from_raw([13, 0, 0, 0]),
    ];

    /// The member commitments of the governance group, in the order `create_group` is given them.
    pub fn governance_member_commitments() -> Vec<pallas::Base> {
        Self::GOVERNANCE_MEMBERS
            .iter()
            .map(|s| crate::harness::multisig::MultiSigHarness::member_commitment(*s))
            .collect()
    }

    /// The id of the governance group: the multisig contract's derivation, called rather than
    /// re-implemented, over the members above.
    ///
    /// The fund stores this id (`update_config`), and the approval `execute` requires must name it —
    /// so the id has one definition. It replaces a placeholder (`from_raw([9, …])`) that no group
    /// ever existed under, which is why `execute` had nothing to check against (`OBL-C101`).
    pub fn governance_group() -> pallas::Base {
        crate::harness::multisig::MultiSigHarness::group_id(
            Self::GOVERNANCE_THRESHOLD,
            &Self::governance_member_commitments(),
        )
    }

    /// The authority call data for an endpoint: the secret above, the one fund, and the zero
    /// transaction pair the fixtures bind.
    fn authority(&self) -> AuthorityCallData {
        AuthorityCallData::new(Self::AUTHORITY_SECRET, Self::FUND_ID)
    }

    /// The frame commitment over a call set that is this call alone (`OBL-C198`). Correct only for a
    /// **childless** endpoint: a caller with children assembles `[child…, parent]` in DFS post-order
    /// and derives over the whole set with `dwow_sdk::crypto::util::tx_commitment`.
    fn commitment(&self, call_data: &[u8]) -> pallas::Base {
        let call = dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.to_vec() };
        dwow_sdk::crypto::util::tx_commitment([&call])
    }

    /// The id `propose_process_instruction_v1` derives — `poseidon_hash([fund.id, message_hash])`
    /// (`entrypoint.rs:582`) — and therefore the id `vote` and `execute` have to name, and the
    /// message the fund's group must approve for an execution to be allowed. The three endpoints are
    /// a flow, not three independent calls, which is what the first two runs of this test said by
    /// failing at `vote` and then `execute`.
    pub fn proposal_id(&self) -> pallas::Base {
        self.proposal_id_of(Self::MESSAGE_HASH)
    }

    /// The proposal id the fund derives from any message — `proposal_id` with the message chosen, so
    /// a caller can name a proposal that is *not* the one the fixture executes (`OBL-C101`'s record
    /// check, whose control needs exactly that).
    pub fn proposal_id_of(&self, message_hash: pallas::Base) -> pallas::Base {
        dwow_sdk::crypto::poseidon_hash([Self::FUND_ID, message_hash])
    }

    /// The message hash `propose` proposes.
    const MESSAGE_HASH: pallas::Base = pallas::Base::from_raw([7, 0, 0, 0]);

    /// Assemble `initialize`'s call data and stop, before the proof (`OBL-C198`).
    pub fn initialize_prepare(&self) -> Result<AuthorityCallPlan> {
        let authority = self.authority();
        let pi = authority.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(e.to_string()))?;
        let params = InitializeParamsV1 {
            instance_seed: [0u8; 32],
            fund_id: Self::FUND_ID,
            spend_authority: authority.authority_pub(),
            dao_escrow_bulla: pallas::Base::zero(),
            drain_config: DrainConfig::default(),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode());
        Ok(AuthorityCallPlan { call_data, authority, zkbin: self.initialize_zkbin.clone(), pk: self.initialize_pk.clone() })
    }

    /// `initialize` as the **only** call in its transaction (childless fast path, `OBL-C198`).
    pub fn initialize(&self) -> Result<DrainInitResult> {
        let plan = self.initialize_prepare()?;
        let call_data = plan.call_data.clone();
        let commitment = self.commitment(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(DrainInitResult { call_data, proof })
    }

    /// Assemble `propose`'s call data and stop, before the proof (`OBL-C198`).
    pub fn propose_prepare(&self) -> Result<AuthorityCallPlan> {
        let authority = self.authority();
        let pi = authority.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(e.to_string()))?;
        let params = ProposeParamsV1 {
            message_hash: Self::MESSAGE_HASH,
            // `propose_process_instruction_v1` looks the **fund** up by this field
            // (`entrypoint.rs:520`), so the fixture passes the fund's id — the name says multisig
            // group and the key is the funds tree.
            multisig_group_id: Self::FUND_ID,
            prover_pubkey: authority.authority_pub(),
            vote_period_blocks: 1000,
            proof: vec![],
            fund_id: Self::FUND_ID,
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
        Ok(AuthorityCallPlan { call_data, authority, zkbin: self.propose_zkbin.clone(), pk: self.propose_pk.clone() })
    }

    /// `propose` as the **only** call in its transaction (childless fast path, `OBL-C198`).
    pub fn propose(&self) -> Result<DrainProposeResult> {
        let plan = self.propose_prepare()?;
        let call_data = plan.call_data.clone();
        let commitment = self.commitment(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(DrainProposeResult { call_data, proof })
    }

    /// Assemble `vote`'s call data and stop, before the proof (`OBL-C198`).
    pub fn vote_prepare(&self) -> Result<AuthorityCallPlan> {
        let authority = self.authority();
        let pi = authority.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(e.to_string()))?;
        let params = VoteParamsV1 {
            proposal_id: self.proposal_id(),
            voter_pubkey: authority.authority_pub(),
            vote: true,
            signature: pallas::Base::zero(),
            fund_id: Self::FUND_ID,
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());
        Ok(AuthorityCallPlan { call_data, authority, zkbin: self.vote_zkbin.clone(), pk: self.vote_pk.clone() })
    }

    /// `vote` as the **only** call in its transaction (childless fast path, `OBL-C198`).
    pub fn vote(&self) -> Result<DrainVoteResult> {
        let plan = self.vote_prepare()?;
        let call_data = plan.call_data.clone();
        let commitment = self.commitment(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(DrainVoteResult { call_data, proof })
    }

    /// Assemble `execute`'s call data and stop, before the proof (`OBL-C198`). This endpoint always
    /// carries a `multisig::FinalizeV1` child, so the caller frames `[child, parent]` and proves the
    /// parent against that commitment.
    pub fn execute_prepare(&self) -> Result<AuthorityCallPlan> {
        let authority = self.authority();
        let pi = authority.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(e.to_string()))?;
        let params = ExecuteParamsV1 {
            proposal_id: self.proposal_id(),
            signature: pallas::Base::zero(),
            fund_id: Self::FUND_ID,
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());
        Ok(AuthorityCallPlan { call_data, authority, zkbin: self.execute_zkbin.clone(), pk: self.execute_pk.clone() })
    }

    /// Assemble `exit`'s call data and stop, before the proof (`OBL-C198`). Carries a
    /// `pn_transfer_payout_child`, so the caller frames `[child, parent]`.
    pub fn exit_prepare(&self) -> Result<ExitCallPlan> {
        let call = ExitCallData::new();
        let pi = call.compute_public_inputs();
        let params = ExitParamsV1 {
            fund_id: Self::FUND_ID,
            member_pubkey: PublicKey::from_secret(SecretKey::from_base(Self::AUTHORITY_SECRET)),
            contribution_weight: 1000,
            current_block: 0,
            dao_escrow_bulla: pallas::Base::zero(),
            dao_membership_note: pallas::Base::zero(),
            effective_weight: pallas::Base::from(1000u64),
            proof: vec![],
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x04];
        call_data
            .extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
        Ok(ExitCallPlan { call_data, call, zkbin: self.exit_zkbin.clone(), pk: self.exit_pk.clone() })
    }

    /// Assemble `transfer`'s call data and stop, before the proof (`OBL-C198`). This endpoint always
    /// carries a `pn_transfer_child`, so the caller frames `[child, parent]`.
    pub fn transfer_prepare(&self) -> Result<AuthorityCallPlan> {
        self.transfer_naming_prepare(self.proposal_id())
    }

    /// `transfer` naming a proposal the caller chooses.
    ///
    /// The rate-limited path requires the proposal to have been **executed** (`OBL-C101`), not merely
    /// named, so this is the shape the fixture's control needs: the same call, the same proof, the
    /// same amount, naming a proposal the fund's group approved and never executed.
    pub fn transfer_naming_prepare(&self, proposal_id: pallas::Base) -> Result<AuthorityCallPlan> {
        let authority = self.authority();
        let pi = authority.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(e.to_string()))?;
        let params = TransferParamsV1 {
            fund_id: Self::FUND_ID,
            amount: 100,
            recipient: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(4321u64))),
            signature: pallas::Base::zero(),
            // The pool's `total_funds` is zero and nothing raises it, so `check_rate_limit`'s
            // threshold is zero and *every* transfer is rate-limited — which is why this endpoint
            // takes the multisig path, naming the proposal `propose` created. `exceeds_rate_limit`
            // without a `vote_proposal_id` is `Unauthorized` (`entrypoint.rs:674`), and with one it
            // must be a proposal `execute` recorded.
            exceeds_rate_limit: true,
            vote_proposal_id: Some(proposal_id),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode());
        Ok(AuthorityCallPlan { call_data, authority, zkbin: self.transfer_zkbin.clone(), pk: self.transfer_pk.clone() })
    }

    /// Assemble `lock`'s call data and stop, before the proof (`OBL-C198`).
    pub fn lock_prepare(&self) -> Result<AuthorityCallPlan> {
        let authority = self.authority();
        let pi = authority.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(e.to_string()))?;
        let params = LockParamsV1 {
            fund_id: Self::FUND_ID,
            duration_blocks: 6000,
            signature: pallas::Base::zero(),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&params.encode());
        Ok(AuthorityCallPlan { call_data, authority, zkbin: self.lock_zkbin.clone(), pk: self.lock_pk.clone() })
    }

    /// `lock` as the **only** call in its transaction (childless fast path, `OBL-C198`).
    pub fn lock(&self) -> Result<DrainLockResult> {
        let plan = self.lock_prepare()?;
        let call_data = plan.call_data.clone();
        let commitment = self.commitment(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(DrainLockResult { call_data, proof })
    }

    /// Assemble `unlock`'s call data and stop, before the proof (`OBL-C198`).
    pub fn unlock_prepare(&self) -> Result<AuthorityCallPlan> {
        let authority = self.authority();
        let pi = authority.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(e.to_string()))?;
        let params = UnlockParamsV1 {
            fund_id: Self::FUND_ID,
            signature: pallas::Base::zero(),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&params.encode());
        Ok(AuthorityCallPlan { call_data, authority, zkbin: self.unlock_zkbin.clone(), pk: self.unlock_pk.clone() })
    }

    /// `unlock` as the **only** call in its transaction (childless fast path, `OBL-C198`).
    pub fn unlock(&self) -> Result<DrainUnlockResult> {
        let plan = self.unlock_prepare()?;
        let call_data = plan.call_data.clone();
        let commitment = self.commitment(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(DrainUnlockResult { call_data, proof })
    }

    /// `update_config` as the **only** call in its transaction (childless fast path, `OBL-C198`).
    pub fn update_config(&self) -> Result<DrainUpdateConfigResult> {
        let plan = self.update_config_prepare()?;
        let call_data = plan.call_data.clone();
        let commitment = self.commitment(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(DrainUpdateConfigResult { call_data, proof })
    }

    /// `update_config`, but proving with a **stranger's** secret — the negative control for
    /// `OBL-C97`. The proof is a real proof of the real circuit, made by the contract's own client;
    /// the only thing that differs from `update_config` is which secret the point derives from. So a
    /// rejection can only come from the host comparing that point against the fund's registered one,
    /// and if the check is ever removed this endpoint starts succeeding.
    pub fn update_config_as_stranger(&self) -> Result<DrainUpdateConfigResult> {
        let plan = self.update_config_as_stranger_prepare()?;
        let call_data = plan.call_data.clone();
        let commitment = self.commitment(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(DrainUpdateConfigResult { call_data, proof })
    }

    /// Assemble `update_config`'s call data and stop, before the proof (`OBL-C198`).
    pub fn update_config_prepare(&self) -> Result<AuthorityCallPlan> {
        self.update_config_with_prepare(self.authority())
    }

    /// As [`Self::update_config_prepare`], from a stranger's secret (`OBL-C97`'s control).
    pub fn update_config_as_stranger_prepare(&self) -> Result<AuthorityCallPlan> {
        self.update_config_with_prepare(AuthorityCallData::new(Self::STRANGER_SECRET, Self::FUND_ID))
    }

    fn update_config_with_prepare(&self, authority: AuthorityCallData) -> Result<AuthorityCallPlan> {
        let pi = authority.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(e.to_string()))?;
        let params = UpdateConfigParamsV1 {
            fund_id: Self::FUND_ID,
            rate_limit: None,
            // `execute_process_instruction_v1` requires the fund to have a governance group
            // (`entrypoint.rs`), and `initialize` stores zero — this endpoint is the only path that
            // sets one, which is why the flow needs this call before `execute` rather than merely
            // tolerating it. The id is **`governance_group()`**: the group the fixture creates on
            // chain and gathers approvals from, so the fund's record and the approval agree
            // (`OBL-C101`).
            multisig_group_id: Some(Self::governance_group()),
            new_spend_authority: None,
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x08];
        call_data.extend_from_slice(&params.encode());
        Ok(AuthorityCallPlan { call_data, authority, zkbin: self.update_config_zkbin.clone(), pk: self.update_config_pk.clone() })
    }
}

impl super::ContractHarness for DrainProtectionHarness {
    fn name(&self) -> &str {
        "drain_protection"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "ExitProofV2",
            "ExecuteV2",
            "InitializeV2",
            "LockV2",
            "ProposeV2",
            "TransferV2",
            "UnlockV2",
            "UpdateConfigV2",
            "VoteV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "ExitProofV2" => Some(&self.exit_zkbin),
            "ExecuteV2" => Some(&self.execute_zkbin),
            "InitializeV2" => Some(&self.initialize_zkbin),
            "LockV2" => Some(&self.lock_zkbin),
            "ProposeV2" => Some(&self.propose_zkbin),
            "TransferV2" => Some(&self.transfer_zkbin),
            "UnlockV2" => Some(&self.unlock_zkbin),
            "UpdateConfigV2" => Some(&self.update_config_zkbin),
            "VoteV2" => Some(&self.vote_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "ExitProofV2" => Some(&self.exit_pk),
            "ExecuteV2" => Some(&self.execute_pk),
            "InitializeV2" => Some(&self.initialize_pk),
            "LockV2" => Some(&self.lock_pk),
            "ProposeV2" => Some(&self.propose_pk),
            "TransferV2" => Some(&self.transfer_pk),
            "UnlockV2" => Some(&self.unlock_pk),
            "UpdateConfigV2" => Some(&self.update_config_pk),
            "VoteV2" => Some(&self.vote_pk),
            _ => None,
        }
    }
}

pub struct DrainInitResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct DrainProposeResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct DrainVoteResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct DrainExecuteResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct DrainExitResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct DrainTransferResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct DrainLockResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct DrainUnlockResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct DrainUpdateConfigResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
