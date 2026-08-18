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

//! Bridge Test Harness
//!
//! Provides isolated testing for the bridge-core contract (deposit/withdraw).

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{IntentCommitment, IntentNullifier, PublicKey, pasta_prelude::PrimeField},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_bridge_contract::client::{
    deposit::{DepositCallData, DepositPublicInputs, create_deposit_proof},
    withdraw::{WithdrawCallData, WithdrawPublicInputs, create_withdraw_proof},
};
use dwow_bridge_contract::model::{DepositParams, ExternalChain, ExternalChainProof, WithdrawParams};

/// Bridge Harness for isolated testing
pub struct BridgeHarness {
    /// Deposit_V1 ZkBinary
    deposit_zkbin: ZkBinary,
    /// Deposit_V1 ProvingKey
    deposit_pk: ProvingKey,
    /// Withdraw_V1 ZkBinary
    withdraw_zkbin: ZkBinary,
    /// Withdraw_V1 ProvingKey
    withdraw_pk: ProvingKey,
}

impl BridgeHarness {
    /// Spawn a new Bridge harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let deposit_bin = include_bytes!("../../../bridge/proof/deposit.zk.bin");
        let withdraw_bin = include_bytes!("../../../bridge/proof/withdraw.zk.bin");

        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();
        let withdraw_zkbin = ZkBinary::decode(withdraw_bin, false).unwrap();

        let deposit_pk = ProvingKey::build(
            deposit_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&deposit_zkbin).unwrap(), &deposit_zkbin),
        ).expect("ProvingKey::build failed");
        let withdraw_pk = ProvingKey::build(
            withdraw_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&withdraw_zkbin).unwrap(), &withdraw_zkbin),
        ).expect("ProvingKey::build failed");

        Self {
            deposit_zkbin, deposit_pk,
            withdraw_zkbin, withdraw_pk,
        }
    }

    /// Create a deposit with ZK proof
    pub fn deposit(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        external_block_hash: pallas::Base,
        chain: ExternalChain,
        fee: u64,
    ) -> Result<DepositResult, Box<dyn std::error::Error>> {
        let input = DepositCallData::new(
            secret,
            amount,
            recipient_public,
            bridge_nonce,
            external_block_hash,
        );

        let (proof, public_inputs) = create_deposit_proof(
            &self.deposit_zkbin,
            &self.deposit_pk,
            &input,
        )?;

        let params = DepositParams {
            commitment: IntentCommitment::from_bytes(public_inputs.commitment.to_repr())
                .map_err(|e| format!("Invalid commitment: {e}"))?,
            recipient_pub: recipient_public,
            bridge_nonce,
            chain,
            external_block_hash: public_inputs.external_block_hash.to_repr(),
            // External-chain merkle proof is supplied by the indexer; empty in the
            // harness (bridge-verify is feature-gated off).
            merkle_proof: vec![],
            // External-chain state root — not verified without bridge-verify.
            external_state_root: [0u8; 32],
            fee,
            amount,
            proof: proof.as_ref().to_vec(),
            chain_proof: ExternalChainProof::Ethereum,
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(DepositResult { call_data, proof, public_inputs })
    }

    /// Create a withdrawal with ZK proof (function code 0x02)
    pub fn withdraw(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_hash: pallas::Base,
        token_minimum: u64,
        fee: u64,
    ) -> Result<WithdrawResult, Box<dyn std::error::Error>> {
        let input = WithdrawCallData::new(
            secret,
            amount,
            recipient_hash,
            token_minimum,
        );

        let (proof, public_inputs) = create_withdraw_proof(
            &self.withdraw_zkbin,
            &self.withdraw_pk,
            &input,
        )?;

        let nullifier = IntentNullifier::from_bytes(public_inputs.nullifier.to_repr())
            .map_err(|e| format!("Invalid nullifier: {e}"))?;

        let params = WithdrawParams {
            nullifier,
            recipient_hash: public_inputs.recipient_hash.to_repr(),
            amount,
            proof: proof.as_ref().to_vec(),
            fee,
            timeout_height: 0,
            feed_mode: 0,
            max_fee_bp: None,
            token_minimum,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        Ok(WithdrawResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for BridgeHarness {
    fn name(&self) -> &str {
        "bridge"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["DepositV2", "WithdrawV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "DepositV2" => Some(&self.deposit_zkbin),
            "WithdrawV2" => Some(&self.withdraw_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "DepositV2" => Some(&self.deposit_pk),
            "WithdrawV2" => Some(&self.withdraw_pk),
            _ => None,
        }
    }
}

/// Result of deposit
pub struct DepositResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: DepositPublicInputs,
}

/// Result of withdraw
pub struct WithdrawResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: WithdrawPublicInputs,
}
