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
//! Note: drain_protection has 9 ZK circuits loaded, but the client proof
//! generation module is not yet implemented. This harness exposes the circuits and
//! proving keys via the ContractHarness trait for direct use in tests.

use dwow_core::{
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
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
        let exit_bin = include_bytes!("../../../drain_protection/proof/exit_v1.zk.bin");
        let execute_bin = include_bytes!("../../../drain_protection/proof/execute_v1.zk.bin");
        let initialize_bin = include_bytes!("../../../drain_protection/proof/initialize_v1.zk.bin");
        let lock_bin = include_bytes!("../../../drain_protection/proof/lock_v1.zk.bin");
        let propose_bin = include_bytes!("../../../drain_protection/proof/propose_v1.zk.bin");
        let transfer_bin = include_bytes!("../../../drain_protection/proof/transfer_v1.zk.bin");
        let unlock_bin = include_bytes!("../../../drain_protection/proof/unlock_v1.zk.bin");
        let update_config_bin = include_bytes!("../../../drain_protection/proof/update_config_v1.zk.bin");
        let vote_bin = include_bytes!("../../../drain_protection/proof/vote_v1.zk.bin");

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

    fn make_proof(&self, zkbin: &ZkBinary, pk: &ProvingKey) -> dwow_core::Result<Proof> {
        let w = dwow_core::zk::empty_witnesses(zkbin)?;
        let c = ZkCircuit::new(w, zkbin);
        Proof::create(pk, &[c], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))
    }

    pub fn initialize(&self) -> dwow_core::Result<DrainInitResult> {
        Ok(DrainInitResult { call_data: vec![0x00], proof: self.make_proof(&self.initialize_zkbin, &self.initialize_pk)? })
    }
    pub fn propose(&self) -> dwow_core::Result<DrainProposeResult> {
        Ok(DrainProposeResult { call_data: vec![0x01], proof: self.make_proof(&self.propose_zkbin, &self.propose_pk)? })
    }
    pub fn vote(&self) -> dwow_core::Result<DrainVoteResult> {
        Ok(DrainVoteResult { call_data: vec![0x02], proof: self.make_proof(&self.vote_zkbin, &self.vote_pk)? })
    }
    pub fn execute(&self) -> dwow_core::Result<DrainExecuteResult> {
        Ok(DrainExecuteResult { call_data: vec![0x03], proof: self.make_proof(&self.execute_zkbin, &self.execute_pk)? })
    }
    pub fn exit(&self) -> dwow_core::Result<DrainExitResult> {
        Ok(DrainExitResult { call_data: vec![0x04], proof: self.make_proof(&self.exit_zkbin, &self.exit_pk)? })
    }
    pub fn transfer(&self) -> dwow_core::Result<DrainTransferResult> {
        Ok(DrainTransferResult { call_data: vec![0x05], proof: self.make_proof(&self.transfer_zkbin, &self.transfer_pk)? })
    }
    pub fn lock(&self) -> dwow_core::Result<DrainLockResult> {
        Ok(DrainLockResult { call_data: vec![0x06], proof: self.make_proof(&self.lock_zkbin, &self.lock_pk)? })
    }
    pub fn unlock(&self) -> dwow_core::Result<DrainUnlockResult> {
        Ok(DrainUnlockResult { call_data: vec![0x07], proof: self.make_proof(&self.unlock_zkbin, &self.unlock_pk)? })
    }
    pub fn update_config(&self) -> dwow_core::Result<DrainUpdateConfigResult> {
        Ok(DrainUpdateConfigResult { call_data: vec![0x08], proof: self.make_proof(&self.update_config_zkbin, &self.update_config_pk)? })
    }
}

impl super::ContractHarness for DrainProtectionHarness {
    fn name(&self) -> &str {
        "drain_protection"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "ExitProof",
            "ExecuteV1",
            "InitializeV1",
            "LockV1",
            "ProposeV1",
            "TransferV1",
            "UnlockV1",
            "UpdateConfigV1",
            "VoteV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "ExitProof" => Some(&self.exit_zkbin),
            "ExecuteV1" => Some(&self.execute_zkbin),
            "InitializeV1" => Some(&self.initialize_zkbin),
            "LockV1" => Some(&self.lock_zkbin),
            "ProposeV1" => Some(&self.propose_zkbin),
            "TransferV1" => Some(&self.transfer_zkbin),
            "UnlockV1" => Some(&self.unlock_zkbin),
            "UpdateConfigV1" => Some(&self.update_config_zkbin),
            "VoteV1" => Some(&self.vote_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "ExitProof" => Some(&self.exit_pk),
            "ExecuteV1" => Some(&self.execute_pk),
            "InitializeV1" => Some(&self.initialize_pk),
            "LockV1" => Some(&self.lock_pk),
            "ProposeV1" => Some(&self.propose_pk),
            "TransferV1" => Some(&self.transfer_pk),
            "UnlockV1" => Some(&self.unlock_pk),
            "UpdateConfigV1" => Some(&self.update_config_pk),
            "VoteV1" => Some(&self.vote_pk),
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
