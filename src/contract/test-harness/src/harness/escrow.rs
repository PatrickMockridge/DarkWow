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

//! Escrow Test Harness
//!
//! Provides isolated testing for Escrow contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{pedersen_commitment_u64, Blind, ContractId, MerkleNode, MerkleTree, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_escrow_contract::client::{
    claim::{ClaimEscrowCallData, create_claim_escrow_proof, ClaimEscrowPublicInputs},
    create_escrow::{CreateEscrowCallData, create_escrow_proof, CreateEscrowPublicInputs},
    fund::{FundEscrowCallData, create_fund_escrow_proof, FundEscrowPublicInputs},
    refund::{RefundEscrowCallData, create_refund_escrow_proof, RefundEscrowPublicInputs},
};
use dwow_escrow_contract::model::{
    CreateEscrowParamsV1, EscrowId, FundEscrowParamsV1, ClaimEscrowParamsV1, RefundEscrowParamsV1,
};

/// Escrow Harness for isolated testing
pub struct EscrowHarness {
    /// CreateEscrow_V1 ZkBinary
    create_escrow_zkbin: ZkBinary,
    /// CreateEscrow_V1 ProvingKey
    create_escrow_pk: ProvingKey,
    /// Fund_V1 ZkBinary
    fund_zkbin: ZkBinary,
    /// Fund_V1 ProvingKey
    fund_pk: ProvingKey,
    /// Claim_V1 ZkBinary
    claim_zkbin: ZkBinary,
    /// Claim_V1 ProvingKey
    claim_pk: ProvingKey,
    /// Refund_V1 ZkBinary
    refund_zkbin: ZkBinary,
    /// Refund_V1 ProvingKey
    refund_pk: ProvingKey,
    /// Merkle tree for escrow commitments
    merkle_tree: dwow_sdk::crypto::MerkleTree,
    /// List of created escrow commitments
    created_commitments: Vec<pallas::Base>,
    /// The deployed id of the escrow contract this harness builds calls for.
    ///
    /// A call carries the contract it addresses and the transaction commitment covers the call, so
    /// a harness that proves must be told which contract it is proving for (`OBL-C198`). It was a
    /// field on `spawn()`'s caller before, as the `ContractId::from_bytes([0u8; 32])` placeholder.
    contract_id: ContractId,
}

/// The commitment a call set's proofs must bind to (`OBL-C198`) — over the **whole ordered set**
/// the node will hash, children before parents, each call serialized with its contract id. One
/// helper because every builder needs the same derivation and a second copy is a second value
/// (`safety.md` RC5).
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

impl EscrowHarness {
    /// Spawn a new Escrow harness with pre-loaded circuits, for the contract deployed at
    /// `contract_id` — see the field's note for why the id cannot be defaulted.
    pub fn spawn(contract_id: ContractId) -> Self {
        let create_bin = include_bytes!("../../../escrow/proof/create_escrow.zk.bin");
        let fund_bin = include_bytes!("../../../escrow/proof/fund.zk.bin");
        let claim_bin = include_bytes!("../../../escrow/proof/claim.zk.bin");
        let refund_bin = include_bytes!("../../../escrow/proof/refund.zk.bin");

        let create_escrow_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let fund_zkbin = ZkBinary::decode(fund_bin, false).unwrap();
        let claim_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let refund_zkbin = ZkBinary::decode(refund_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_escrow_zkbin).unwrap(),
            &create_escrow_zkbin,
        );
        let fund_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&fund_zkbin).unwrap(),
            &fund_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&claim_zkbin).unwrap(),
            &claim_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&refund_zkbin).unwrap(),
            &refund_zkbin,
        );

        let create_escrow_pk = ProvingKey::build(create_escrow_zkbin.k, &create_circuit).expect("ProvingKey::build failed");
        let fund_pk = ProvingKey::build(fund_zkbin.k, &fund_circuit).expect("ProvingKey::build failed");
        let claim_pk = ProvingKey::build(claim_zkbin.k, &claim_circuit).expect("ProvingKey::build failed");
        let refund_pk = ProvingKey::build(refund_zkbin.k, &refund_circuit).expect("ProvingKey::build failed");

        // Initialize merkle tree for escrow commitments
        let merkle_tree = MerkleTree::new(1);
        let created_commitments = vec![];

        Self {
            create_escrow_zkbin,
            create_escrow_pk,
            fund_zkbin,
            fund_pk,
            claim_zkbin,
            claim_pk,
            refund_zkbin,
            refund_pk,
            merkle_tree,
            created_commitments,
            contract_id,
        }
    }
}

impl EscrowHarness {
    /// Create an escrow, binding the proof to the commitment over `children` **and** this call.
    ///
    /// Correct when the create *is* the transaction, which is what `test_heavyweight_metadata`
    /// exercises (`children` empty there), and the only correct form when it has a child.
    ///
    /// This is the one endpoint here that cannot be built in a single shot, and the reason is a
    /// genuine cycle: the spec's child is a box put whose `contents_commit` is a function of the
    /// escrow id; so the child cannot be built until this call's data is known; the escrow id is a
    /// function of *this* call's data; and the commitment — which this proof must bind to — is a
    /// function of the child's data. The order that satisfies all three is: build this call's data
    /// ([`Self::create_escrow_prepare`]), let the caller build the child around the exposed
    /// `public_inputs.commitment`, then prove over the whole set ([`Self::create_escrow_prove`]).
    pub fn create_escrow(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        buyer_secret: pallas::Base,
        buyer_pubkey: PublicKey,
        seller_pubkey: PublicKey,
        value: u64,
        asset_id: pallas::Base,
        timeout: u64,
        instance_seed: [u8; 32],
    ) -> Result<CreateEscrowResult, Box<dyn std::error::Error>> {
        let plan = self.create_escrow_prepare(
            buyer_secret, buyer_pubkey, seller_pubkey, value, asset_id, timeout, instance_seed,
        )?;
        self.create_escrow_prove(plan, children)
    }

    /// A create call built but not yet proven — see [`Self::create_escrow`] for why this endpoint
    /// needs the split. `public_inputs.commitment` is the escrow id the caller's child is built
    /// around, and it is a derivation over the call's own inputs, so it needs no proof.
    pub fn create_escrow_prepare(
        &self,
        buyer_secret: pallas::Base,
        buyer_pubkey: PublicKey,
        seller_pubkey: PublicKey,
        value: u64,
        asset_id: pallas::Base,
        timeout: u64,
        instance_seed: [u8; 32],
    ) -> Result<CreateEscrowPlan, Box<dyn std::error::Error>> {
        let input = CreateEscrowCallData::new(
            buyer_secret,
            buyer_pubkey,
            seller_pubkey,
            value,
            asset_id,
            timeout,
        );

        // The public inputs are a pure function of the call data — `commitment` is derived, not
        // proven — so they are available before the proof, which is what lets the caller's child be
        // built and the commitment be taken over the final set.
        let public_inputs = input.compute_public_inputs();
        let escrow_id = public_inputs.commitment;

        // Build CreateEscrowParamsV1
        let params = CreateEscrowParamsV1 {
            buyer_pubkey,
            seller_pubkey,
            value,
            asset_id,
            timeout,
            commitment: EscrowId(escrow_id),
            merkle_root: MerkleNode::new(pallas::Base::zero()),
            instance_seed,
        };

        // Kept verbatim from the pre-plan body: it appends to a *clone* of the tree and drops it, so
        // it changes nothing on `self` — `fund_escrow` builds its own tree from the escrow id. Left
        // as it was rather than tidied, so that this diff is the migration and nothing else.
        let mut tree = self.merkle_tree.clone();
        tree.append(MerkleNode::new(escrow_id));
        tree.mark();

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateEscrowPlan { input, call_data, public_inputs })
    }

    /// Prove a prepared `create` against the whole ordered call set (`OBL-C198`).
    pub fn create_escrow_prove(
        &self,
        plan: CreateEscrowPlan,
        children: &[dwow_sdk::tx::ContractCall],
    ) -> Result<CreateEscrowResult, Box<dyn std::error::Error>> {
        let CreateEscrowPlan { mut input, call_data, public_inputs } = plan;

        // ONE commitment over the whole ordered call set — children first, this call last (DFS
        // post-order) — so this proof and its children's bind to the same value.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_escrow_proof(&self.create_escrow_zkbin, &self.create_escrow_pk, &input)?;

        Ok(CreateEscrowResult { call_data, public_inputs, proof, commitment })
    }

    /// Fund an escrow, binding the proof to the commitment over `children` **and** this call.
    /// `FundV1` in this contract's spec carries two children, so the caller passes them.
    pub fn fund_escrow(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        escrow_id: pallas::Base,
        value: u64,
        value_blind: pallas::Scalar,
    ) -> Result<FundEscrowResult, Box<dyn std::error::Error>> {
        let plan = self.fund_escrow_prepare(escrow_id, value, value_blind)?;
        self.fund_escrow_prove(plan, children)
    }

    /// A fund call built but not yet proven (`OBL-C198`).
    pub fn fund_escrow_prepare(
        &self,
        escrow_id: pallas::Base,
        value: u64,
        value_blind: pallas::Scalar,
    ) -> Result<FundEscrowPlan, Box<dyn std::error::Error>> {
        // Build a local merkle tree for the fund proof (escrow_id is the only leaf).
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::new(escrow_id));
        let leaf_pos = tree.mark().unwrap();
        let merkle_leaf_pos: u32 = u64::from(leaf_pos).try_into().unwrap();
        let merkle_path = tree.witness(leaf_pos, 0).unwrap();

        let input = FundEscrowCallData::new(
            value,
            value_blind,
            escrow_id,
            merkle_leaf_pos,
            merkle_path,
        );

        // Compute value commitment using Pedersen commitment
        let value_commit = pedersen_commitment_u64(value, Blind(value_blind));

        // Build FundEscrowParamsV1 — `merkle_root` is the contract's own derivation, called rather
        // than re-implemented here.
        let public_inputs = input.compute_public_inputs().map_err(|e| dwow_core::Error::Custom(format!("{e:?}")))?;
        let params = FundEscrowParamsV1 {
            escrow_id: EscrowId(escrow_id),
            value_commit,
            merkle_proof: vec![],
            merkle_root: MerkleNode::new(public_inputs.merkle_root),
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        Ok(FundEscrowPlan { input, call_data, public_inputs })
    }

    /// Prove a prepared `fund` against the whole ordered call set (`OBL-C198`).
    pub fn fund_escrow_prove(
        &self,
        plan: FundEscrowPlan,
        children: &[dwow_sdk::tx::ContractCall],
    ) -> Result<FundEscrowResult, Box<dyn std::error::Error>> {
        let FundEscrowPlan { mut input, call_data, public_inputs } = plan;

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_fund_escrow_proof(&self.fund_zkbin, &self.fund_pk, &input)?;

        Ok(FundEscrowResult { call_data, public_inputs, proof, commitment })
    }

    /// Claim an escrow, binding the proof to the commitment over `children` **and** this call.
    /// `ClaimV1` in this contract's spec carries two children, so the caller passes them.
    pub fn claim_escrow(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        escrow_id: pallas::Base,
        seller_secret: pallas::Base,
        seller_pubkey: PublicKey,
        escrow_seller_commitment: pallas::Base,
        recipient_pubkey: PublicKey,
    ) -> Result<ClaimEscrowResult, Box<dyn std::error::Error>> {
        let plan = self.claim_escrow_prepare(escrow_id, seller_secret, seller_pubkey, escrow_seller_commitment, recipient_pubkey)?;
        self.claim_escrow_prove(plan, children)
    }

    /// A claim call built but not yet proven (`OBL-C198`). The nullifier is a poseidon hash of the
    /// escrow id and the seller's secret, so the call data is complete before the proof exists.
    pub fn claim_escrow_prepare(
        &self,
        escrow_id: pallas::Base,
        seller_secret: pallas::Base,
        seller_pubkey: PublicKey,
        escrow_seller_commitment: pallas::Base,
        recipient_pubkey: PublicKey,
    ) -> Result<ClaimEscrowPlan, Box<dyn std::error::Error>> {
        let input = ClaimEscrowCallData::new(
            escrow_id,
            seller_secret,
            seller_pubkey,
            escrow_seller_commitment,
        );

        // Build ClaimEscrowParamsV1 — no `seller_secret`: the circuit proves it as a witness and
        // exposes the nullifier, so the params carry only what a host reads.
        let public_inputs = input.compute_public_inputs();
        let params = ClaimEscrowParamsV1 {
            escrow_id: EscrowId(escrow_id),
            spent_nullifier: public_inputs.spent_nullifier,
            recipient_pubkey,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        Ok(ClaimEscrowPlan { input, call_data, public_inputs })
    }

    /// Prove a prepared `claim` against the whole ordered call set (`OBL-C198`).
    pub fn claim_escrow_prove(
        &self,
        plan: ClaimEscrowPlan,
        children: &[dwow_sdk::tx::ContractCall],
    ) -> Result<ClaimEscrowResult, Box<dyn std::error::Error>> {
        let ClaimEscrowPlan { mut input, call_data, public_inputs } = plan;

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_claim_escrow_proof(&self.claim_zkbin, &self.claim_pk, &input)?;

        Ok(ClaimEscrowResult { call_data, public_inputs, proof, commitment })
    }

    /// Refund an escrow (after timeout), binding the proof to the commitment over `children`
    /// **and** this call.
    #[expect(clippy::too_many_arguments, reason = "the circuit's witness list, plus the child set OBL-C198 needs")]
    pub fn refund_escrow(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        escrow_id: pallas::Base,
        timeout: u64,
        current_block: u64,
        buyer_secret: pallas::Base,
        buyer_pubkey: PublicKey,
        escrow_buyer_pub_x: pallas::Base,
        escrow_buyer_pub_y: pallas::Base,
        recipient_pubkey: PublicKey,
    ) -> Result<RefundEscrowResult, Box<dyn std::error::Error>> {
        let plan = self.refund_escrow_prepare(escrow_id, timeout, current_block, buyer_secret, buyer_pubkey, escrow_buyer_pub_x, escrow_buyer_pub_y, recipient_pubkey)?;
        self.refund_escrow_prove(plan, children)
    }

    /// A refund call built but not yet proven (`OBL-C198`).
    pub fn refund_escrow_prepare(
        &self,
        escrow_id: pallas::Base,
        timeout: u64,
        current_block: u64,
        buyer_secret: pallas::Base,
        buyer_pubkey: PublicKey,
        escrow_buyer_pub_x: pallas::Base,
        escrow_buyer_pub_y: pallas::Base,
        recipient_pubkey: PublicKey,
    ) -> Result<RefundEscrowPlan, Box<dyn std::error::Error>> {
        let input = RefundEscrowCallData::new(
            escrow_id,
            timeout,
            current_block,
            buyer_secret,
            buyer_pubkey,
            escrow_buyer_pub_x,
            escrow_buyer_pub_y,
        );

        // Build RefundEscrowParamsV1 — no `buyer_secret`, as `claim` above.
        let public_inputs = input.compute_public_inputs();
        let params = RefundEscrowParamsV1 {
            escrow_id: EscrowId(escrow_id),
            spent_nullifier: public_inputs.spent_nullifier,
            current_block,
            timeout,
            recipient_pubkey,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        Ok(RefundEscrowPlan { input, call_data, public_inputs })
    }

    /// Prove a prepared `refund` against the whole ordered call set (`OBL-C198`).
    pub fn refund_escrow_prove(
        &self,
        plan: RefundEscrowPlan,
        children: &[dwow_sdk::tx::ContractCall],
    ) -> Result<RefundEscrowResult, Box<dyn std::error::Error>> {
        let RefundEscrowPlan { mut input, call_data, public_inputs } = plan;

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_refund_escrow_proof(&self.refund_zkbin, &self.refund_pk, &input)?;

        Ok(RefundEscrowResult { call_data, public_inputs, proof, commitment })
    }
}

impl super::ContractHarness for EscrowHarness {
    fn name(&self) -> &str {
        "escrow"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateEscrowV2", "FundV2", "ClaimV2", "RefundV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateEscrowV2" => Some(&self.create_escrow_zkbin),
            "FundV2" => Some(&self.fund_zkbin),
            "ClaimV2" => Some(&self.claim_zkbin),
            "RefundV2" => Some(&self.refund_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateEscrowV2" => Some(&self.create_escrow_pk),
            "FundV2" => Some(&self.fund_pk),
            "ClaimV2" => Some(&self.claim_pk),
            "RefundV2" => Some(&self.refund_pk),
            _ => None,
        }
    }
}

// ============================================================================
// Prepared calls (`OBL-C198`)
// ============================================================================

/// A `create` call built but not yet proven — see [`EscrowHarness::create_escrow`] for why this
/// endpoint needs the split. `public_inputs.commitment` is the escrow id the caller's child is
/// built around.
pub struct CreateEscrowPlan {
    input: CreateEscrowCallData,
    call_data: Vec<u8>,
    pub public_inputs: CreateEscrowPublicInputs,
}

/// A `fund` call built but not yet proven (`OBL-C198`).
pub struct FundEscrowPlan {
    input: FundEscrowCallData,
    call_data: Vec<u8>,
    pub public_inputs: FundEscrowPublicInputs,
}

/// A `claim` call built but not yet proven (`OBL-C198`).
pub struct ClaimEscrowPlan {
    input: ClaimEscrowCallData,
    call_data: Vec<u8>,
    pub public_inputs: ClaimEscrowPublicInputs,
}

/// A `refund` call built but not yet proven (`OBL-C198`).
pub struct RefundEscrowPlan {
    input: RefundEscrowCallData,
    call_data: Vec<u8>,
    pub public_inputs: RefundEscrowPublicInputs,
}

// ============================================================================
// Result Structs
// ============================================================================

/// Result of create_escrow
pub struct CreateEscrowResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: CreateEscrowPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// Result of fund_escrow
pub struct FundEscrowResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: FundEscrowPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of claim_escrow
pub struct ClaimEscrowResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: ClaimEscrowPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of refund_escrow
pub struct RefundEscrowResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: RefundEscrowPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}