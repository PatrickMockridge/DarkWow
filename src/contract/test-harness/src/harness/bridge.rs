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

    /// Prepare a deposit — the call data, and no proof yet (`OBL-C198`).
    ///
    /// The proof binds the **transaction** commitment, a derivation over the transaction's whole
    /// call set, so the call must exist before its proof can and only the caller knows what else
    /// its transaction carries. Nothing here depends on the commitment: the commitment and the
    /// recipient key are pure derivations of the input. And `DepositParams` no longer carries a
    /// proof at all — it could not bind while it did, because a proof inside the call data makes
    /// the commitment over that call data cover the proof, which is circular. That circularity is
    /// why this contract's metadata arm published a constant until `OBL-C198` took the proof off
    /// the wire; it now rides in `ContractCallImport.proofs`, where every binding contract puts it.
    pub fn deposit_prepare(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        external_block_hash: pallas::Base,
        chain: ExternalChain,
        merkle_proof: Vec<[u8; 32]>,
        fee: u64,
    ) -> Result<DepositPlan, Box<dyn std::error::Error>> {
        let input = DepositCallData::new(
            secret,
            amount,
            recipient_public,
            bridge_nonce,
            external_block_hash,
        );

        let commitment = input.compute_commitment();

        let params = DepositParams {
            commitment: IntentCommitment::from_bytes(commitment.to_repr())
                .map_err(|e| format!("Invalid commitment: {e}"))?,
            recipient_pub: recipient_public,
            bridge_nonce,
            chain,
            external_block_hash: external_block_hash.to_repr(),
            // External-chain merkle proof, supplied by the caller. It rides in `DepositParams` but
            // is not a public input of `deposit.zk` (whose instances are the derived commitment,
            // the tx binding and the nonce), so its contents do not affect the proof. Taking it as
            // a parameter rather than hardcoding emptiness is what lets a spec exercise the
            // deposit's external-chain gate: with `bridge-verify` off that gate refuses every
            // chain, while an empty proof is refused a step earlier by a shape check — and an
            // assertion that only sees "some error" cannot tell the two apart (OBL-C21).
            merkle_proof,
            // External-chain state root — not verified without bridge-verify.
            external_state_root: [0u8; 32],
            fee,
            amount,
            chain_proof: ExternalChainProof::Ethereum,
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        Ok(DepositPlan { call_data, input, zkbin: self.deposit_zkbin.clone(), pk: self.deposit_pk.clone() })
    }

    /// Prepare a withdrawal — the call data, and no proof yet. See [`Self::deposit_prepare`] for
    /// why the split exists and why the proof left the wire.
    pub fn withdraw_prepare(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_hash: pallas::Base,
        fee: u64,
    ) -> Result<WithdrawPlan, Box<dyn std::error::Error>> {
        let input = WithdrawCallData::new(
            secret,
            amount,
            recipient_hash,
        );

        let nullifier = IntentNullifier::from_bytes(input.compute_nullifier().to_repr())
            .map_err(|e| format!("Invalid nullifier: {e}"))?;

        let params = WithdrawParams {
            nullifier,
            recipient_hash: recipient_hash.to_repr(),
            amount,
            fee,
            timeout_height: 0,
            feed_mode: 0,
            max_fee_bp: None,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        Ok(WithdrawPlan { call_data, input, zkbin: self.withdraw_zkbin.clone(), pk: self.withdraw_pk.clone() })
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

/// A prepared deposit: the call data exists, the proof does not (`OBL-C198`).
pub struct DepositPlan {
    /// The deposit's call data — **without** a proof, because the proof binds a commitment over
    /// this call data and would otherwise be inside what it binds.
    pub call_data: Vec<u8>,
    input: DepositCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl DepositPlan {
    /// Prove the deposit, binding its proof to `tx_commitment` — the commitment over the whole call
    /// set the transaction carries, this call included.
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<(dwow_core::zk::Proof, DepositPublicInputs), Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = create_deposit_proof(&self.zkbin, &self.pk, &input)?;
        Ok((proof, public_inputs))
    }
}

/// A prepared withdrawal: the call data exists, the proof does not. See [`DepositPlan`].
pub struct WithdrawPlan {
    pub call_data: Vec<u8>,
    input: WithdrawCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl WithdrawPlan {
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<(dwow_core::zk::Proof, WithdrawPublicInputs), Box<dyn std::error::Error>> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let (proof, public_inputs) = create_withdraw_proof(&self.zkbin, &self.pk, &input)?;
        Ok((proof, public_inputs))
    }
}

/// Result of a one-step deposit.
pub struct DepositResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: DepositPublicInputs,
}

/// Result of a one-step withdraw.
pub struct WithdrawResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: WithdrawPublicInputs,
}

impl BridgeHarness {
    /// Create a deposit and its proof in one step, binding a **zero** commitment.
    ///
    /// Kept because the migration is half done and this half is honest about which half it is:
    /// the proof no longer rides in the call data, but `bridge`'s metadata arm still publishes a
    /// constant binding, so a one-shot caller here agrees with it. A caller that binds the real
    /// value uses [`Self::deposit_prepare`] and [`DepositPlan::prove`] — and cannot use it until
    /// this contract's fixtures frame their children, which is `OBL-C198`'s next step.
    pub fn deposit(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        external_block_hash: pallas::Base,
        chain: ExternalChain,
        merkle_proof: Vec<[u8; 32]>,
        fee: u64,
    ) -> Result<DepositResult, Box<dyn std::error::Error>> {
        let plan = self.deposit_prepare(secret, amount, recipient_public, bridge_nonce, external_block_hash, chain, merkle_proof, fee)?;
        let call_data = plan.call_data.clone();
        let (proof, public_inputs) = plan.prove(pallas::Base::zero())?;
        Ok(DepositResult { call_data, proof, public_inputs })
    }

    /// Create a withdrawal and its proof in one step, binding a **zero** commitment. See
    /// [`Self::deposit`] for why the one-step form still exists and what it does and does not bind.
    pub fn withdraw(
        &self,
        secret: pallas::Base,
        amount: u64,
        recipient_hash: pallas::Base,
        fee: u64,
    ) -> Result<WithdrawResult, Box<dyn std::error::Error>> {
        let plan = self.withdraw_prepare(secret, amount, recipient_hash, fee)?;
        let call_data = plan.call_data.clone();
        let (proof, public_inputs) = plan.prove(pallas::Base::zero())?;
        Ok(WithdrawResult { call_data, proof, public_inputs })
    }
}
