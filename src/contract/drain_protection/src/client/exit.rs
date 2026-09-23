//! `ExitV1` proof generation — the first client proof module in this contract (`OBL-C78`).
//!
//! **What was missing and what that cost.** `src/contract/drain_protection/src/client/` held
//! `mod.rs` and `zkbins.rs` and nothing else, so no caller could build any of this contract's nine
//! proofs: the test harness fabricated them (`Proof::create(pk, &[c], &[], OsRng)` — one instance and
//! no advice), and the metadata arms published two literal zeros under circuits that instance five
//! values. `exit.zk` is the simplest of the nine — it derives `tx_binding` and instances
//! `[tx_binding, tx_nonce]`, and declares fifteen further witnesses that its own circuit block never
//! reads — so it is the first one built, and the others follow its shape.
//!
//! **The pair is the whole public surface.** The circuit derives
//! `tx_binding = poseidon_hash([3, tx_commitment, tx_nonce])` from the witnesses and exposes it with
//! `tx_nonce`, so the metadata must publish both — and the metadata can only publish what the call
//! carries, which is why they travel in the params (`ExitParamsV1::tx_binding`/`tx_nonce`) and are
//! computed here. `tx_pair` binds a real transaction; without it both halves are zero, which is what
//! the fixtures use.
//!
//! **The fifteen unread witnesses are passed as zeros deliberately**, and the reason is written
//! rather than implied: `exit.zk`'s circuit block is two lines, so `member_secret`, `dao_path`,
//! `DIVISOR`, `EXIT_MULTIPLIER` and the rest constrain nothing today. When the exit arithmetic is
//! put into the circuit, this is the function that has to supply them, and the call data grows then.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        constants::{DRK_POSEIDON_DOMAIN_TX_BINDING, MERKLE_DEPTH_ORCHARD},
        poseidon_hash, MerkleNode,
    },
    pasta::pallas,
};

/// The public inputs `exit.zk` instances, in the circuit's order: `tx_binding`, then `tx_nonce`.
#[derive(Debug, Clone)]
pub struct ExitPublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ExitPublicInputs {
    /// Convert to vector for ZK proof creation. Order must match `constrain_instance` in
    /// `exit.zk`: `tx_binding`, `tx_nonce`.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce]
    }
}

/// Input data for `exit.zk` proof generation.
#[derive(Debug, Clone)]
pub struct ExitCallData {
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl Default for ExitCallData {
    fn default() -> Self {
        Self::new()
    }
}

impl ExitCallData {
    /// A call with no transaction to bind — both halves zero, the fixture default.
    pub fn new() -> Self {
        Self { tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    /// Bind the proof to a transaction: the pair the witnesses and the public inputs both use, so
    /// the params and the proof agree (`OBL-C78`).
    pub fn tx_pair(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Self {
        self.tx_commitment = tx_commitment;
        self.tx_nonce = tx_nonce;
        self
    }

    /// The transaction binding, derived once here so the witness and the public input cannot
    /// disagree about it.
    pub fn tx_binding(&self) -> pallas::Base {
        poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> ExitPublicInputs {
        ExitPublicInputs { tx_binding: self.tx_binding(), tx_nonce: self.tx_nonce }
    }

    /// Prover witnesses, in the order `exit.zk` declares them. The fifteen the circuit's block does
    /// not read are zeros; see this module's header for why that is stated rather than hidden.
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let zero = Value::known(pallas::Base::zero());
        let path: [MerkleNode; MERKLE_DEPTH_ORCHARD] =
            [MerkleNode::from_base(pallas::Base::zero()); MERKLE_DEPTH_ORCHARD];
        vec![
            Witness::Base(zero),                       // fund_id
            Witness::Base(zero),                       // current_block
            Witness::Base(zero),                       // member_secret
            Witness::Base(zero),                       // member_pub_x
            Witness::Base(zero),                       // member_pub_y
            Witness::Base(zero),                       // deposited_at
            Witness::Base(zero),                       // contribution_amount
            Witness::Base(zero),                       // contribution_weight
            Witness::Base(zero),                       // dao_escrow_bulla
            Witness::Base(zero),                       // dao_membership_note
            Witness::Base(zero),                       // DIVISOR
            Witness::Base(zero),                       // EXIT_MULTIPLIER
            Witness::Uint32(Value::known(0u32)),       // dao_leaf_pos
            Witness::MerklePath(Value::known(path)),   // dao_path
            Witness::Base(zero),                       // dao_escrow_merkle_root
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.tx_binding())),
        ]
    }
}

/// Generate an `ExitV1` proof.
///
/// Witness order must match `exit.zk`; the public inputs must match the metadata arm's vector
/// (`entrypoint.rs`'s `drain_protection_exit_get_metadata_v1`), which publishes the pair the params
/// carry.
pub fn create_exit_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ExitCallData,
) -> Result<(Proof, ExitPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let prover_witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rand::rngs::OsRng)?;

    Ok((proof, public_inputs))
}
