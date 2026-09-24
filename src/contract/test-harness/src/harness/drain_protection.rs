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

impl DrainProtectionHarness {
    /// Spawn a new DrainProtection harness with pre-loaded circuits
    pub fn spawn() -> Self {
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

    /// The authority call data for an endpoint: the secret above, the one fund, and the zero
    /// transaction pair the fixtures bind.
    fn authority(&self) -> AuthorityCallData {
        AuthorityCallData::new(Self::AUTHORITY_SECRET, Self::FUND_ID)
    }

    /// The id `propose_process_instruction_v1` derives — `poseidon_hash([fund.id, message_hash])`
    /// (`entrypoint.rs:531`) — and therefore the id `vote` and `execute` have to name. The three
    /// endpoints are a flow, not three independent calls, which is what the first two runs of this
    /// test said by failing at `vote` and then `execute`.
    fn proposal_id(&self) -> pallas::Base {
        dwow_sdk::crypto::poseidon_hash([Self::FUND_ID, Self::MESSAGE_HASH])
    }

    /// The message hash `propose` proposes.
    const MESSAGE_HASH: pallas::Base = pallas::Base::from_raw([7, 0, 0, 0]);

    pub fn initialize(&self) -> dwow_core::Result<DrainInitResult> {
        let authority = self.authority();
        let (proof, pi) = create_authority_proof(&self.initialize_zkbin, &self.initialize_pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = InitializeParamsV1 {
            instance_seed: [0u8; 32],
            fund_id: Self::FUND_ID,
            spend_authority: authority.authority_pub(),
            dao_escrow_bulla: pallas::Base::zero(),
            drain_config: DrainConfig::default(),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode());
        Ok(DrainInitResult { call_data, proof })
    }

    pub fn propose(&self) -> dwow_core::Result<DrainProposeResult> {
        let authority = self.authority();
        let (proof, pi) = create_authority_proof(&self.propose_zkbin, &self.propose_pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = ProposeParamsV1 {
            message_hash: Self::MESSAGE_HASH,
            // `propose_process_instruction_v1` looks the **fund** up by this field
            // (`entrypoint.rs:520`), so the fixture passes the fund's id — the name says multisig
            // group and the key is the funds tree.
            multisig_group_id: Self::FUND_ID,
            prover_pubkey: authority.authority_pub(),
            vote_period_blocks: 1000,
            proof: vec![],
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
        Ok(DrainProposeResult { call_data, proof })
    }

    pub fn vote(&self) -> dwow_core::Result<DrainVoteResult> {
        let authority = self.authority();
        let (proof, pi) = create_authority_proof(&self.vote_zkbin, &self.vote_pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = VoteParamsV1 {
            proposal_id: self.proposal_id(),
            voter_pubkey: authority.authority_pub(),
            vote: true,
            signature: pallas::Base::zero(),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());
        Ok(DrainVoteResult { call_data, proof })
    }

    pub fn execute(&self) -> dwow_core::Result<DrainExecuteResult> {
        let authority = self.authority();
        let (proof, pi) = create_authority_proof(&self.execute_zkbin, &self.execute_pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = ExecuteParamsV1 {
            proposal_id: self.proposal_id(),
            signature: pallas::Base::zero(),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());
        Ok(DrainExecuteResult { call_data, proof })
    }

    pub fn exit(&self) -> dwow_core::Result<DrainExitResult> {
        let call = ExitCallData::new();
        let (proof, pi) = create_exit_proof(&self.exit_zkbin, &self.exit_pk, &call)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = ExitParamsV1 {
            fund_id: Self::FUND_ID,
            member_pubkey: PublicKey::from_secret(SecretKey::from_base(Self::AUTHORITY_SECRET)),
            contribution_weight: 1000,
            current_block: 0,
            dao_escrow_bulla: pallas::Base::zero(),
            dao_membership_note: pallas::Base::zero(),
            effective_weight: pallas::Base::from(1000u64),
            proof: vec![],
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x04];
        call_data
            .extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
        Ok(DrainExitResult { call_data, proof })
    }

    pub fn transfer(&self) -> dwow_core::Result<DrainTransferResult> {
        let authority = self.authority();
        let (proof, pi) = create_authority_proof(&self.transfer_zkbin, &self.transfer_pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = TransferParamsV1 {
            fund_id: Self::FUND_ID,
            amount: 100,
            recipient: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(4321u64))),
            signature: pallas::Base::zero(),
            exceeds_rate_limit: false,
            vote_proposal_id: None,
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode());
        Ok(DrainTransferResult { call_data, proof })
    }

    pub fn lock(&self) -> dwow_core::Result<DrainLockResult> {
        let authority = self.authority();
        let (proof, pi) = create_authority_proof(&self.lock_zkbin, &self.lock_pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = LockParamsV1 {
            fund_id: Self::FUND_ID,
            duration_blocks: 6000,
            signature: pallas::Base::zero(),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&params.encode());
        Ok(DrainLockResult { call_data, proof })
    }

    pub fn unlock(&self) -> dwow_core::Result<DrainUnlockResult> {
        let authority = self.authority();
        let (proof, pi) = create_authority_proof(&self.unlock_zkbin, &self.unlock_pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = UnlockParamsV1 {
            fund_id: Self::FUND_ID,
            signature: pallas::Base::zero(),
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&params.encode());
        Ok(DrainUnlockResult { call_data, proof })
    }

    pub fn update_config(&self) -> dwow_core::Result<DrainUpdateConfigResult> {
        let authority = self.authority();
        let (proof, pi) = create_authority_proof(&self.update_config_zkbin, &self.update_config_pk, &authority)
            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
        let params = UpdateConfigParamsV1 {
            fund_id: Self::FUND_ID,
            rate_limit: None,
            multisig_group_id: None,
            new_spend_authority: None,
            authority_pub_x: pi.authority_pub_x,
            authority_pub_y: pi.authority_pub_y,
            authority_nullifier: pi.authority_nullifier,
            tx_binding: pi.tx_binding,
            tx_nonce: pi.tx_nonce,
        };
        let mut call_data = vec![0x08];
        call_data.extend_from_slice(&params.encode());
        Ok(DrainUpdateConfigResult { call_data, proof })
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
