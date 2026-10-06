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

//! RelayerEndowment Test Harness
//!
//! Provides isolated testing for RelayerEndowment contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    blockchain::BlockHeight,
    crypto::{
        pasta_prelude::{Curve, Group, PrimeField},
        pedersen_commitment_u64, Blind, ContractId, PublicKey,
    },
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_relayer_endowment_contract::client::{
    initialize::{InitializeV1CallData, initialize_v1_proof, InitializeV1PublicInputs},
    deploy_capital::{DeployCapitalV1CallData, deploy_capital_v1_proof, DeployCapitalV1PublicInputs},
    claim_fees::{ClaimFeesV1CallData, claim_fees_v1_proof, ClaimFeesV1PublicInputs},
};
use dwow_relayer_endowment_contract::model::{
    InitializeParamsV1, DeployCapitalParamsV1, ClaimFeesParamsV1,
};

/// The commitment over an ordered call set (`OBL-C198`). The order is DFS post-order — children
/// before the parent — and **one** commitment is taken for the whole transaction, so a child's
/// proof and its parent's bind to the same value. It is derived over the call *including* the
/// contract id, which is why the harness is given the deployed id rather than a placeholder.
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

/// RelayerEndowment Harness for isolated testing
pub struct RelayerEndowmentHarness {
    /// Initialize_V1 ZkBinary
    initialize_zkbin: ZkBinary,
    /// Initialize_V1 ProvingKey
    initialize_pk: ProvingKey,
    /// DeployCapital_V1 ZkBinary
    deploy_capital_zkbin: ZkBinary,
    /// DeployCapital_V1 ProvingKey
    deploy_capital_pk: ProvingKey,
    /// ClaimFees_V1 ZkBinary
    claim_fees_zkbin: ZkBinary,
    /// ClaimFees_V1 ProvingKey
    claim_fees_pk: ProvingKey,
    /// The height of the block the next generated call will land in, published by the runner
    /// through `ContractHarness::set_next_block_height`. All three of this contract's circuits
    /// bind it into a public input, so every proof here is made against it.
    next_block_height: std::cell::Cell<Option<u64>>,
    /// The contract's deployed id (`OBL-C198`): the commitment is over the call set, and a call
    /// carries the contract it addresses, so a prover must know this id to derive the commitment
    /// its proof binds to. The spec supplies it.
    contract_id: ContractId,
}

impl RelayerEndowmentHarness {
    /// Spawn a new RelayerEndowment harness with pre-loaded circuits
    pub fn spawn(contract_id: ContractId) -> Self {
        let init_bin = include_bytes!("../../../relayer_endowment/proof/initialize.zk.bin");
        let deploy_bin = include_bytes!("../../../relayer_endowment/proof/deploy_capital.zk.bin");
        let claim_bin = include_bytes!("../../../relayer_endowment/proof/claim_fees.zk.bin");

        let initialize_zkbin = ZkBinary::decode(init_bin, false).unwrap();
        let deploy_capital_zkbin = ZkBinary::decode(deploy_bin, false).unwrap();
        let claim_fees_zkbin = ZkBinary::decode(claim_bin, false).unwrap();

        let init_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&initialize_zkbin).unwrap(),
            &initialize_zkbin,
        );
        let deploy_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&deploy_capital_zkbin).unwrap(),
            &deploy_capital_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&claim_fees_zkbin).unwrap(),
            &claim_fees_zkbin,
        );

        let initialize_pk = ProvingKey::build(initialize_zkbin.k, &init_circuit).expect("ProvingKey::build failed");
        let deploy_capital_pk = ProvingKey::build(deploy_capital_zkbin.k, &deploy_circuit).expect("ProvingKey::build failed");
        let claim_fees_pk = ProvingKey::build(claim_fees_zkbin.k, &claim_circuit).expect("ProvingKey::build failed");

        Self {
            initialize_zkbin,
            initialize_pk,
            deploy_capital_zkbin,
            deploy_capital_pk,
            claim_fees_zkbin,
            claim_fees_pk,
            next_block_height: std::cell::Cell::new(None),
            contract_id,
        }
    }

    /// The commitment for a single-call endpoint of this harness — the call, and the node's own
    /// derivation over it (`OBL-C198`). Every endpoint here is childless in its spec, so the
    /// single-call form is correct; a caller whose transaction carries children must assemble the
    /// whole ordered set and derive over it with `dwow_sdk::crypto::util::tx_commitment`.
    fn commitment(&self, call_data: &[u8]) -> pallas::Base {
        commitment_of(&[dwow_sdk::tx::ContractCall {
            contract_id: self.contract_id,
            data: call_data.to_vec(),
        }])
    }

    /// The height the call being generated will be validated at.
    ///
    /// Panics rather than defaulting: a proof made against a guessed height is a proof for a
    /// different call, and the only symptom is `invalid proof`, which names neither the height
    /// nor the circuit. The runner sets this before every generation.
    fn verifying_height(&self) -> Result<u64, Box<dyn std::error::Error>> {
        self.next_block_height.get().ok_or_else(|| {
            "RelayerEndowmentHarness: the runner did not publish the verifying block height \
             (ContractHarness::set_next_block_height) before generating a proof. All three of \
             this contract's circuits bind it, so it cannot be defaulted."
                .into()
        })
    }

    /// Initialize a relayer endowment account with ZK proof
    ///
    /// The verifying block height is taken from the runner, not from the caller: it is the one
    /// input the `InitializeV2` instance has that a caller cannot choose, and passing it as a
    /// parameter is how this harness came to make proofs against height 0 for blocks validated
    /// at height 2.
    pub fn initialize(
        &self,
        relayer_public: PublicKey,
        default_backer_cut_bp: u32,
    ) -> Result<InitializeResult, Box<dyn std::error::Error>> {
        let nonce = self.verifying_height()?;

        let params = InitializeParamsV1 {
            default_backer_cut_bp,
            signature_public: relayer_public,
            instance_seed: [0u8; 32],
        };

        // `OBL-C198`: the call data and the commitment over it come **before** the proof. The
        // params carry no tx pair, so the call is complete without it; the commitment it is hashed
        // into is what the proof binds to.
        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode());
        let commitment = self.commitment(&call_data);

        let mut input = InitializeV1CallData::new(relayer_public, default_backer_cut_bp, nonce);
        input.tx_commitment = commitment;
        let (proof, public_inputs) = initialize_v1_proof(
            &self.initialize_zkbin,
            &self.initialize_pk,
            &input,
        )?;

        Ok(InitializeResult { call_data, proof, public_inputs })
    }

    /// Deploy capital to a relayer's endowment with ZK proof
    ///
    /// `value_commit` in the params must be the commitment the proof's instances carry, because
    /// the contract's metadata reads the commitment **out of the params** and re-derives the
    /// instances from it. This passed `pallas::Point::identity()` — which has no affine
    /// coordinates, so the metadata's `coords.is_none()` guard fired and it answered with an
    /// empty buffer, the documented rejection signal. The proof could never have been checked.
    pub fn deploy_capital_prepare(
        &self,
        backer_public: PublicKey,
        deploy_amount: u64,
        asset_id: pallas::Base,
        value_blind: pallas::Scalar,
        relayer_pub: PublicKey,
        backer_cut_bp: u32,
    ) -> Result<DeployCapitalCallPlan, Box<dyn std::error::Error>> {
        let nonce = self.verifying_height()?;

        let params = DeployCapitalParamsV1 {
            relayer_pub,
            amount: deploy_amount,
            backer_cut_bp,
            signature_public: backer_public,
            value_commit: pedersen_commitment_u64(deploy_amount, Blind(value_blind)),
            min_success_rate_bp: None,
            max_slash_count: None,
            instance_seed: [0u8; 32],
        };

        // `OBL-C198`: the call data exists before the proof, and the commitment is taken over the
        // **whole call set** by the caller — `deploy_capital_v1` requires a promissory_note child,
        // so the commitment this proof binds cannot be derived here.
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        let input = DeployCapitalV1CallData::new(
            relayer_pub,
            backer_public,
            backer_cut_bp,
            deploy_amount,
            asset_id,
            nonce,
            value_blind,
        );
        Ok(DeployCapitalCallPlan {
            zkbin: self.deploy_capital_zkbin.clone(),
            pk: self.deploy_capital_pk.clone(),
            input,
            call_data,
        })
    }

    /// Prove a prepared DeployCapital call against `commitment`/`nonce`.
    pub fn deploy_capital_prove(
        &self,
        plan: DeployCapitalCallPlan,
        commitment: pallas::Base,
        tx_nonce: pallas::Base,
    ) -> Result<DeployCapitalResult, Box<dyn std::error::Error>> {
        let mut input = plan.input;
        input.tx_commitment = commitment;
        input.tx_nonce = tx_nonce;
        let (proof, public_inputs) = deploy_capital_v1_proof(&plan.zkbin, &plan.pk, &input)?;
        Ok(DeployCapitalResult { call_data: plan.call_data, proof, public_inputs })
    }

    /// Prepare **and** prove a DeployCapital call for a transaction in which it is the only call —
    /// correct only because `deploy_capital_v1` requires a child and this fixture gives it none.
    /// A caller with children must use [`Self::deploy_capital_prepare`], assemble the ordered set,
    /// derive over it with `dwow_sdk::crypto::util::tx_commitment`, then [`Self::deploy_capital_prove`].
    pub fn deploy_capital_solo(
        &self,
        backer_public: PublicKey,
        deploy_amount: u64,
        asset_id: pallas::Base,
        value_blind: pallas::Scalar,
        relayer_pub: PublicKey,
        backer_cut_bp: u32,
    ) -> Result<DeployCapitalResult, Box<dyn std::error::Error>> {
        let plan = self.deploy_capital_prepare(
            backer_public, deploy_amount, asset_id, value_blind, relayer_pub, backer_cut_bp,
        )?;
        let commitment = self.commitment(&plan.call_data);
        self.deploy_capital_prove(plan, commitment, pallas::Base::zero())
    }

    /// Claim accumulated fees from a deployment with ZK proof
    ///
    /// The params carry the backer's public key **coordinates as bytes**, because the contract's
    /// metadata hashes `params.backer_pub_x/y` — not the proof's witness. This passed
    /// `[0u8; 32]` for both, so the metadata derived a claim id from the point at infinity while
    /// the proof derived one from the backer's key. The two agree only for a backer whose
    /// coordinates are zero.
    pub fn claim_fees(
        &self,
        deployment_id: pallas::Base,
        backer_public: PublicKey,
        fee_share: u64,
    ) -> Result<ClaimFeesResult, Box<dyn std::error::Error>> {
        let nonce = self.verifying_height()?;

        let mut input = ClaimFeesV1CallData::new(deployment_id, backer_public, fee_share, nonce);
        let (backer_pub_x, backer_pub_y) = input.backer_pub_xy();
        let params = ClaimFeesParamsV1 {
            deployment_id,
            backer_pub_x: backer_pub_x.to_repr(),
            backer_pub_y: backer_pub_y.to_repr(),
            fee_share,
        };

        // `OBL-C198`: the call data and the commitment over it come **before** the proof.
        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());
        input.tx_commitment = self.commitment(&call_data);

        let (proof, public_inputs) = claim_fees_v1_proof(
            &self.claim_fees_zkbin,
            &self.claim_fees_pk,
            &input,
        )?;

        Ok(ClaimFeesResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for RelayerEndowmentHarness {
    fn name(&self) -> &str {
        "relayer_endowment"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["InitializeV2", "DeployCapitalV2", "ClaimFeesV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "InitializeV2" => Some(&self.initialize_zkbin),
            "DeployCapitalV2" => Some(&self.deploy_capital_zkbin),
            "ClaimFeesV2" => Some(&self.claim_fees_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "InitializeV2" => Some(&self.initialize_pk),
            "DeployCapitalV2" => Some(&self.deploy_capital_pk),
            "ClaimFeesV2" => Some(&self.claim_fees_pk),
            _ => None,
        }
    }

    fn set_next_block_height(&self, height: BlockHeight) {
        self.next_block_height.set(Some(height.get()));
    }
}

// ============================================================================
// Result Structs
// ============================================================================

/// Result of initialize
pub struct InitializeResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: InitializeV1PublicInputs,
}

/// A DeployCapital call whose data is assembled and whose proof is not yet made (`OBL-C198`).
///
/// Needed rather than a convenience because `deploy_capital_v1` requires a promissory_note child:
/// the commitment is a derivation over the whole ordered call set, so only the caller knows the
/// value this proof must bind to.
pub struct DeployCapitalCallPlan {
    zkbin: ZkBinary,
    pk: ProvingKey,
    input: DeployCapitalV1CallData,
    pub call_data: Vec<u8>,
}

/// Result of deploy_capital
pub struct DeployCapitalResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: DeployCapitalV1PublicInputs,
}

/// Result of claim_fees
pub struct ClaimFeesResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: ClaimFeesV1PublicInputs,
}
