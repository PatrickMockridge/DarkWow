use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, MerkleNode, MerkleTree},
    pasta::pallas,
};
use rand::SeedableRng;
use crate::harness::ContractHarness;

pub struct BoxHarness { put_zkbin: ZkBinary, put_pk: ProvingKey, take_zkbin: ZkBinary, take_pk: ProvingKey }

impl BoxHarness {
    pub fn spawn() -> Self {
        let put_zkbin = ZkBinary::decode(include_bytes!("../../../box/proof/put.zk.bin"), false).expect("decode put.zk.bin");
        let take_zkbin = ZkBinary::decode(include_bytes!("../../../box/proof/take.zk.bin"), false).expect("decode take.zk.bin");
        let put_pk = ProvingKey::build(put_zkbin.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&put_zkbin).expect("empty_witnesses put"), &put_zkbin)).expect("ProvingKey::build put");
        let take_pk = ProvingKey::build(take_zkbin.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&take_zkbin).expect("empty_witnesses take"), &take_zkbin)).expect("ProvingKey::build take");
        Self { put_zkbin, put_pk, take_zkbin, take_pk }
    }
    pub fn circuits(&self) -> Vec<&'static str> { vec!["PutV2", "TakeV2"] }

    fn build_root(leaf: pallas::Base) -> (u32, Vec<MerkleNode>, MerkleNode) {
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(leaf));
        let mk = tree.mark().expect("tree.mark");
        let p: Vec<MerkleNode> = tree.witness(mk, 0).expect("tree.witness");
        let lp = u32::try_from(u64::from(mk)).expect("position");
        let root = tree.root(0).expect("tree.root");
        (lp, p, root)
    }

    /// Put the default contents — the fixture `box_spec` proves valid against the store.
    pub fn put(&self) -> Result<BoxPutResult> {
        self.put_contents(poseidon_hash([pallas::Base::from(100u64)]))
    }

    /// Put a chosen contents commitment, everything else fixed.
    ///
    /// The box id, the state nonces, the keys, the merkle path and the expected root are the fixture
    /// box's own spec exercises against a real store; only the contents vary. That is what lets
    /// another contract require a take of *its* box rather than of any box: the caller puts its
    /// commitment and the verifier's host compares `TakeParams.contents_commit` against it.
    pub fn put_contents(&self, contents: pallas::Base) -> Result<BoxPutResult> {
        self.put_contents_with_successor(contents, pallas::Base::from(1u64))
    }

    /// The same put with the successor nonce supplied by the caller, so a test can attempt one the
    /// circuit's `new_state_nonce == old_state_nonce + ONE` constraint forbids. `old_state_nonce` stays
    /// zero, so the only difference from `put_contents` is the value that constraint binds.
    pub fn put_contents_with_successor(&self, contents: pallas::Base, nsn: pallas::Base) -> Result<BoxPutResult> {
        self.put_inner(contents, nsn, false)
    }

    /// A control for the arms above: everything as `put_contents` — the successor nonce is the valid
    /// one — except the nullifier *witness* is zeroed while the public input keeps its real value. The
    /// circuit's `nullifier == poseidon(DOMAIN_NULLIFIER, owner_secret, box_id, old_state_nonce)`
    /// constraint must reject it. If this call proves, the negative arms above prove nothing about the
    /// circuit and the reader must not read them as evidence.
    pub fn put_with_zeroed_nullifier_witness(&self) -> Result<BoxPutResult> {
        self.put_inner(poseidon_hash([pallas::Base::from(100u64)]), pallas::Base::from(1u64), true)
    }

    fn put_inner(&self, contents: pallas::Base, nsn: pallas::Base, zero_nullifier_witness: bool) -> Result<BoxPutResult> {
        let plan = self.put_prepare_inner(contents, nsn, zero_nullifier_witness)?;
        // Binding over this call **alone**, which is correct here and only here: box's own spec
        // makes the put the whole transaction. A put used as a child must not use this — see
        // [`Self::put_prepare`].
        let call = dwow_sdk::tx::ContractCall { contract_id: *dwow_sdk::crypto::BOX_CONTRACT_ID, data: plan.call_data.clone() };
        let tc: pallas::Base = dwow_sdk::crypto::util::tx_commitment([&call]);
        plan.prove(tc)
    }

    /// A `PutV1` call built but not yet proven, for use as a **child** (`OBL-C198`).
    ///
    /// The argument is `PurseHarness::deposit_prepare`'s: the node hashes the whole ordered call
    /// set, the parent's bytes come *after* the child's in DFS post-order, so the value the child
    /// must bind to does not exist yet when the child is built. This stops before the proof and
    /// hands the caller the call data; the caller assembles the set, takes the commitment, and
    /// calls [`BoxPutPlan::prove`].
    pub fn put_prepare(&self, contents: pallas::Base) -> Result<BoxPutPlan> {
        self.put_prepare_with_successor(contents, pallas::Base::from(1u64))
    }

    /// [`Self::put_prepare`] with the successor nonce supplied by the caller — the child-side
    /// counterpart of [`Self::put_contents_with_successor`].
    pub fn put_prepare_with_successor(&self, contents: pallas::Base, nsn: pallas::Base) -> Result<BoxPutPlan> {
        self.put_prepare_inner(contents, nsn, false)
    }

    fn put_prepare_inner(&self, contents: pallas::Base, nsn: pallas::Base, zero_nullifier_witness: bool) -> Result<BoxPutPlan> {
        let dnl=pallas::Base::from(1u64);let dml=pallas::Base::from(5u64);let dsig=pallas::Base::from(7u64);
        let os=pallas::Base::from(42u64);let bid=pallas::Base::from(1u64);
        let op=poseidon_hash([dsig,os]);
        let osn=pallas::Base::zero();let occ=pallas::Base::zero();
        let ncc=contents;let tn=pallas::Base::from(300u64);
        let nf=poseidon_hash([dnl,os,bid,osn]);let nl=poseidon_hash([dml,bid,ncc,nsn,op]);
        let ol=poseidon_hash([dml,bid,occ,osn,op]);let (lp,p,root)=Self::build_root(ol);
        let er_base: pallas::Base = root.inner();
        // `p` is cloned, not moved: the reorder above put this before the witness vector, which
        // also needs the path.
        let mpa:[MerkleNode;32]=p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path array".into()))?;
        let nf_val=dwow_sdk::crypto::Nullifier::from_bytes(nf.to_repr()).map_err(|e| dwow_core::Error::Custom(format!("nullifier: {e:?}")))?;
        let nl_node = dwow_sdk::crypto::MerkleNode::from_base(nl);
        // The nonce and both contents commitments are not in this struct — but that is a property of
        // this struct, not of the wire. They are still listed in `box/manifest.toml`'s `[[parameters]]`,
        // and a `witness = N` tag does not keep a field out of the call data (`OBL-C179`, corrected
        // 2026-09-28): a client that builds this call from the manifest encodes them. This harness builds
        // it from `params.encode()` below, which is why they are absent *here*. Unit 7 removes them from
        // the manifest; until then the manifest and this decoder disagree.
        let params=dwow_box_contract::model::PutParams{nullifier:nf_val,expected_root:root,new_leaf:nl_node,leaf_pos:dwow_box_contract::model::MerklePosition::new(lp),merkle_path:mpa,proof:vec![],tx_nonce:tn};
        let mut cd=vec![0x01u8];cd.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
        // Self-addressed AEAD note (wallet.md §2.3, contract-wasm-type-system.md
        // §A.8.2): the produce-side box_capability note carries {commitment,
        // state_nonce} encrypted to the holder's key so the wallet discovers the
        // new leaf by trial-decryption. Appended to the call data — the scan
        // byte-slides over call.data looking for AeadEncryptedNote structures.
        #[derive(dwow_serial::SerialEncodable)]
        struct BoxNote { commitment: pallas::Base, state_nonce: pallas::Base, box_id: pallas::Base, user_data: pallas::Base }
        // `user_data` is the produced contents — the *commitment*, which is all the box layer ever
        // holds — and the field order matches the manifest's `note_schema`, which the scan decodes
        // against. Without it a wallet that put or received a box could not learn what it held.
        let note = BoxNote { commitment: nl, state_nonce: nsn, box_id: bid, user_data: ncc };
        let owner_pk = dwow_sdk::crypto::keypair::PublicKey::from_secret(dwow_sdk::crypto::keypair::SecretKey::from_base(os));
        let encrypted = dwow_sdk::crypto::note::AeadEncryptedNote::encrypt(&note, &owner_pk, &mut rand::rngs::StdRng::seed_from_u64(0)).map_err(|e| dwow_core::Error::Custom(format!("note encrypt: {e:?}")))?;
        let mut note_bytes=vec![];dwow_serial::Encodable::encode(&encrypted,&mut note_bytes).map_err(|e| dwow_core::Error::Custom(format!("note encode: {e:?}")))?;
        cd.extend_from_slice(&note_bytes);

        // `OBL-C198`: the call data is finished here and the proof is built later, over it —
        // `tx_binding` derives from the enclosing transaction's commitment, and that commitment
        // covers these very bytes while excluding *proofs*, which is what makes the order
        // solvable. Proving first — the order this harness used until now — cannot bind to a real
        // transaction at all, and the value it bound to was the literal `200`: not a
        // transaction's commitment but a number chosen here.
        //
        // The pair is last among the *instances*; in the witness vector `op` is declared after it,
        // so the head stops before the pair and `witnesses_tail` carries `op`. `prove` splices the
        // pair back between them, which is the order the circuit reads.
        Ok(BoxPutPlan{
            witnesses_head: vec![Witness::Base(Value::known(bid)),Witness::Base(Value::known(osn)),Witness::Base(Value::known(nsn)),Witness::Base(Value::known(occ)),Witness::Base(Value::known(ncc)),Witness::Base(Value::known(if zero_nullifier_witness { pallas::Base::zero() } else { nf })),Witness::Base(Value::known(er_base)),Witness::Base(Value::known(nl)),Witness::Base(Value::known(os)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path".into()))?))],
            witnesses_tail: vec![Witness::Base(Value::known(op))],
            public_head: vec![nf,er_base,nl],
            tx_nonce: tn,
            call_data: cd,
            put_zkbin: self.put_zkbin.clone(),
            put_pk: self.put_pk.clone(),
        })
    }

    /// Take the default contents — the counterpart of `put`.
    pub fn take(&self) -> Result<BoxTakeResult> {
        self.take_contents(poseidon_hash([pallas::Base::from(100u64)]))
    }

    /// Take a box whose contents commitment is `contents`. It must be the contents a `put_contents`
    /// wrote, or box's own exec rejects the take.
    /// The take a whole transaction is — the form box's own spec uses. A take used as a **child**
    /// must not use this; see [`Self::take_prepare`].
    pub fn take_contents(&self, contents: pallas::Base) -> Result<BoxTakeResult> {
        let plan = self.take_prepare(contents)?;
        let call = dwow_sdk::tx::ContractCall { contract_id: *dwow_sdk::crypto::BOX_CONTRACT_ID, data: plan.call_data.clone() };
        let tc: pallas::Base = dwow_sdk::crypto::util::tx_commitment([&call]);
        plan.prove(tc)
    }

    /// A `TakeV1` call built but not yet proven, for use as a **child** (`OBL-C198`) — the take
    /// escrow's `ClaimV1` needs. See `PurseHarness::deposit_prepare` for the argument.
    pub fn take_prepare(&self, contents: pallas::Base) -> Result<BoxTakePlan> {
        let dnl=pallas::Base::from(1u64);let dml=pallas::Base::from(5u64);let dsig=pallas::Base::from(7u64);
        let os=pallas::Base::from(42u64);let bid=pallas::Base::from(1u64);let sn=pallas::Base::from(1u64);
        let op=poseidon_hash([dsig,os]);
        let cc=contents;let tn=pallas::Base::from(300u64);
        let nf=poseidon_hash([dnl,os,bid,sn]);let ol=poseidon_hash([dml,bid,cc,sn,op]);
        let (lp,p,root)=Self::build_root(ol);
        let er_base: pallas::Base = root.inner();
        // Cloned as in `put_inner`: the reorder put this before the witness vector.
        let mpa:[MerkleNode;32]=p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path array".into()))?;
        let nf_val=dwow_sdk::crypto::Nullifier::from_bytes(nf.to_repr()).map_err(|e| dwow_core::Error::Custom(format!("nullifier: {e:?}")))?;
        // `contents_commit` is a field again, and on the wire: a parent contract reads it through the
        // child call to check the box taken is the one it named. See the note in box's model.
        let params=dwow_box_contract::model::TakeParams{contents_commit:cc,nullifier:nf_val,expected_root:root,leaf_pos:dwow_box_contract::model::MerklePosition::new(lp),merkle_path:mpa,proof:vec![],tx_nonce:tn};
        let mut cd=vec![0x02u8];cd.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        // Reordered as `put_prepare_inner` — see the note there. The call data is finished here;
        // the proof comes later, over the caller's commitment (`OBL-C198`).
        Ok(BoxTakePlan{
            witnesses_head: vec![Witness::Base(Value::known(bid)),Witness::Base(Value::known(cc)),Witness::Base(Value::known(sn)),Witness::Base(Value::known(nf)),Witness::Base(Value::known(er_base)),Witness::Base(Value::known(os)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path".into()))?))],
            witnesses_tail: vec![Witness::Base(Value::known(op))],
            public_head: vec![nf,er_base],
            tx_nonce: tn,
            call_data: cd,
            take_zkbin: self.take_zkbin.clone(),
            take_pk: self.take_pk.clone(),
        })
    }
}

/// A prepared box `put`, proved against a caller-supplied commitment — see
/// [`BoxHarness::put_prepare`] for why a child needs this and a whole transaction does not.
pub struct BoxPutPlan {
    witnesses_head: Vec<Witness>,
    /// The witnesses `put.zk` declares *after* the pair — see [`PurseDepositPlan`] in `purse.rs`
    /// for why the head cannot simply stop at the pair.
    witnesses_tail: Vec<Witness>,
    public_head: Vec<pallas::Base>,
    tx_nonce: pallas::Base,
    /// The call data the commitment must cover. The caller needs it to build the `ContractCall`
    /// this plan signs for.
    pub call_data: Vec<u8>,
    put_zkbin: ZkBinary,
    put_pk: ProvingKey,
}

impl BoxPutPlan {
    /// Prove against `tx_commitment` — the commitment over the whole ordered call set the node
    /// will hash, not just this call.
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<BoxPutResult> {
        // `DOMAIN_TX_BINDING` = 3, the constant the arm and every other builder use.
        let tx_binding=poseidon_hash([pallas::Base::from(3u64),tx_commitment,self.tx_nonce]);
        let mut w=self.witnesses_head;
        w.push(Witness::Base(Value::known(tx_commitment)));
        w.push(Witness::Base(Value::known(self.tx_nonce)));
        w.push(Witness::Base(Value::known(tx_binding)));
        w.extend(self.witnesses_tail);
        // constrain_instance order: nf, er_base, nl, tx_binding, tx_nonce — the pair is last.
        let mut pi=self.public_head;
        pi.push(tx_binding);
        pi.push(self.tx_nonce);
        let c=ZkCircuit::new(w,&self.put_zkbin);
        let proof=Proof::create(&self.put_pk,&[c],&pi,rand::rngs::StdRng::seed_from_u64(0)).map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {e:?}")))?;
        Ok(BoxPutResult{call_data:self.call_data,proof,inputs:pi})
    }
}

/// A prepared box `take`, proved against a caller-supplied commitment — see
/// [`BoxHarness::take_prepare`].
pub struct BoxTakePlan {
    witnesses_head: Vec<Witness>,
    /// The witnesses `take.zk` declares *after* the pair — see [`BoxPutPlan`] above.
    witnesses_tail: Vec<Witness>,
    public_head: Vec<pallas::Base>,
    tx_nonce: pallas::Base,
    /// The call data the commitment must cover. The caller needs it to build the `ContractCall`
    /// this plan signs for.
    pub call_data: Vec<u8>,
    take_zkbin: ZkBinary,
    take_pk: ProvingKey,
}

impl BoxTakePlan {
    /// Prove against `tx_commitment` — the commitment over the whole ordered call set the node
    /// will hash, not just this call.
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<BoxTakeResult> {
        // `DOMAIN_TX_BINDING` = 3, the constant the arm and every other builder use.
        let tx_binding=poseidon_hash([pallas::Base::from(3u64),tx_commitment,self.tx_nonce]);
        let mut w=self.witnesses_head;
        w.push(Witness::Base(Value::known(tx_commitment)));
        w.push(Witness::Base(Value::known(self.tx_nonce)));
        w.push(Witness::Base(Value::known(tx_binding)));
        w.extend(self.witnesses_tail);
        // constrain_instance order: nf, er_base, tx_binding, tx_nonce — the pair is last.
        let mut pi=self.public_head;
        pi.push(tx_binding);
        pi.push(self.tx_nonce);
        let c=ZkCircuit::new(w,&self.take_zkbin);
        let proof=Proof::create(&self.take_pk,&[c],&pi,rand::rngs::StdRng::seed_from_u64(0)).map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {e:?}")))?;
        Ok(BoxTakeResult{call_data:self.call_data,proof})
    }
}

impl ContractHarness for BoxHarness {
    fn name(&self) -> &str { "box" }
    fn circuits(&self) -> Vec<&'static str> { self.circuits() }
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> { match ns { "PutV2"=>Some(&self.put_zkbin),"TakeV2"=>Some(&self.take_zkbin),_=>None } }
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> { match ns { "PutV2"=>Some(&self.put_pk),"TakeV2"=>Some(&self.take_pk),_=>None } }
    /// BoxFunction: InitializeV1 (0x00, non-ZK), PutV1 (0x01, ZK), TakeV1 (0x02, ZK)
    fn non_zk_functions(&self) -> &'static [u8] { &[0x00] }
    fn state_trees(&self) -> &'static [&'static str] { &["nullifiers", "box_roots", "info"] }
    fn function_count(&self) -> usize { 3 }
}

pub struct BoxPutResult { pub call_data: Vec<u8>, pub proof: Proof, pub inputs: Vec<pallas::Base> }
pub struct BoxTakeResult { pub call_data: Vec<u8>, pub proof: Proof }
