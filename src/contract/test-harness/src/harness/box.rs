use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, MerkleNode, MerkleTree},
    pasta::pallas,
};
use incrementalmerkletree::Hashable;
use rand::rngs::OsRng;
use crate::harness::ContractHarness;

pub struct BoxHarness { put_zkbin: ZkBinary, put_pk: ProvingKey, take_zkbin: ZkBinary, take_pk: ProvingKey }

impl BoxHarness {
    pub fn spawn() -> Self {
        let put_zkbin = ZkBinary::decode(include_bytes!("../../../box/proof/put.zk.bin"), false).map_err(|e| format!("decode put.zk.bin: {e:?}")).unwrap();
        let take_zkbin = ZkBinary::decode(include_bytes!("../../../box/proof/take.zk.bin"), false).map_err(|e| format!("decode take.zk.bin: {e:?}")).unwrap();
        let put_pk = ProvingKey::build(put_zkbin.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&put_zkbin).map_err(|e| format!("empty_witnesses put: {e:?}")).unwrap(), &put_zkbin)).map_err(|e| format!("ProvingKey::build put: {e:?}")).unwrap();
        let take_pk = ProvingKey::build(take_zkbin.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&take_zkbin).map_err(|e| format!("empty_witnesses take: {e:?}")).unwrap(), &take_zkbin)).map_err(|e| format!("ProvingKey::build take: {e:?}")).unwrap();
        Self { put_zkbin, put_pk, take_zkbin, take_pk }
    }
    pub fn circuits(&self) -> Vec<&'static str> { vec!["Put", "Take"] }

    fn build_tree_and_root(leaf: pallas::Base) -> Result<(u32, Vec<MerkleNode>, pallas::Base)> {
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(leaf));
        let mk = tree.mark().ok_or_else(|| "tree.mark failed".to_string())?;
        let p: Vec<MerkleNode> = tree.witness(mk, 0).ok_or_else(|| "tree.witness failed".to_string())?;
        let lp = u32::try_from(u64::from(mk)).map_err(|e| format!("position: {e}"))?;
        // Compute root deterministically from path + leaf — same as circuit merkle_root opcode
        let mut current = MerkleNode::from_base(leaf);
        for (level, sibling) in p.iter().enumerate() {
            current = if (lp as usize) & (1 << level) == 0 {
                MerkleNode::combine((level as u8).into(), &current, sibling)
            } else {
                MerkleNode::combine((level as u8).into(), sibling, &current)
            };
        }
        Ok((lp, p, current.inner()))
    }

    pub fn put(&self) -> Result<BoxPutResult> {
        let dnl = pallas::Base::from(1u64); let dtb = pallas::Base::from(3u64);
        let dml = pallas::Base::from(5u64); let dss = pallas::Base::from(7u64);
        let os = pallas::Base::from(42u64);
        let op = poseidon_hash([dss, os]);
        let bid = pallas::Base::from(1u64);
        let osn = pallas::Base::zero(); let nsn = pallas::Base::from(1u64);
        let occ = pallas::Base::zero();
        let ncc = poseidon_hash([pallas::Base::from(100u64)]);
        let tc = pallas::Base::from(200u64); let tn = pallas::Base::from(300u64);
        let nf = poseidon_hash([dnl, os, bid, osn]);
        let tb = poseidon_hash([dtb, tc, tn]);
        let nl = poseidon_hash([dml, bid, ncc, nsn]);
        let ol = poseidon_hash([dml, bid, occ, osn]);

        let (lp, p, er) = Self::build_tree_and_root(ol)?;
        let w = vec![Witness::Base(Value::known(bid)),Witness::Base(Value::known(osn)),Witness::Base(Value::known(nsn)),Witness::Base(Value::known(occ)),Witness::Base(Value::known(ncc)),Witness::Base(Value::known(nf)),Witness::Base(Value::known(er)),Witness::Base(Value::known(nl)),Witness::Base(Value::known(os)),Witness::Base(Value::known(op)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| "path conversion".to_string())?)),Witness::Base(Value::known(tc)),Witness::Base(Value::known(tn)),Witness::Base(Value::known(tb))];
        let pi = vec![nf, er, nl, tb, tn];
        let c = ZkCircuit::new(w, &self.put_zkbin);
        let proof = Proof::create(&self.put_pk, &[c], &pi, OsRng).map_err(|e| format!("Proof::create put: {e:?}"))?;

        let pb: Vec<u8> = dwow_serial::serialize(&proof);
        let mpa: [pallas::Base; 32] = p.iter().map(|n| n.inner()).collect::<Vec<_>>().try_into().map_err(|_| "path array".to_string())?;
        let params = dwow_box_contract::model::PutParams {
            box_id: dwow_box_contract::model::BoxId(bid), old_state_nonce: osn, new_state_nonce: nsn,
            old_contents_commit: occ, new_contents_commit: ncc, nullifier: dwow_sdk::crypto::Nullifier::from_bytes(nf.to_repr()).map_err(|e| format!("nullifier: {e:?}"))?, expected_root: er,
            new_leaf: nl, leaf_pos: lp, merkle_path: mpa, proof: pb, tx_binding: tb, tx_nonce: tn,
        };
        let mut cd = vec![0x01u8]; cd.extend_from_slice(&params.encode());
        Ok(BoxPutResult { call_data: cd, proof })
    }

    pub fn take(&self) -> Result<BoxTakeResult> {
        let dnl = pallas::Base::from(1u64); let dtb = pallas::Base::from(3u64);
        let dml = pallas::Base::from(5u64); let dss = pallas::Base::from(7u64);
        let os = pallas::Base::from(42u64);
        let op = poseidon_hash([dss, os]);
        let bid = pallas::Base::from(1u64); let sn = pallas::Base::from(1u64);
        let cc = pallas::Base::from(100u64);
        let tc = pallas::Base::from(200u64); let tn = pallas::Base::from(300u64);
        let nf = poseidon_hash([dnl, os, bid, sn]);
        let tb = poseidon_hash([dtb, tc, tn]);
        let ol = poseidon_hash([dml, bid, cc, sn]);

        let (lp, p, er) = Self::build_tree_and_root(ol)?;
        let w = vec![Witness::Base(Value::known(bid)),Witness::Base(Value::known(cc)),Witness::Base(Value::known(sn)),Witness::Base(Value::known(nf)),Witness::Base(Value::known(er)),Witness::Base(Value::known(os)),Witness::Base(Value::known(op)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| "path conversion".to_string())?)),Witness::Base(Value::known(tc)),Witness::Base(Value::known(tn)),Witness::Base(Value::known(tb))];
        let pi = vec![nf, er, tb, tn];
        let c = ZkCircuit::new(w, &self.take_zkbin);
        let proof = Proof::create(&self.take_pk, &[c], &pi, OsRng).map_err(|e| format!("Proof::create take: {e:?}"))?;

        let pb: Vec<u8> = dwow_serial::serialize(&proof);
        let mpa: [pallas::Base; 32] = p.iter().map(|n| n.inner()).collect::<Vec<_>>().try_into().map_err(|_| "path array".to_string())?;
        let params = dwow_box_contract::model::TakeParams {
            box_id: dwow_box_contract::model::BoxId(bid), contents_commit: cc, state_nonce: sn,
            nullifier: dwow_sdk::crypto::Nullifier::from_bytes(nf.to_repr()).map_err(|e| format!("nullifier: {e:?}"))?, expected_root: er, leaf_pos: lp, merkle_path: mpa,
            proof: pb, tx_binding: tb, tx_nonce: tn,
        };
        let mut cd = vec![0x02u8]; cd.extend_from_slice(&params.encode());
        Ok(BoxTakeResult { call_data: cd, proof })
    }
}

impl ContractHarness for BoxHarness {
    fn name(&self) -> &str { "box" }
    fn circuits(&self) -> Vec<&'static str> { self.circuits() }
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> { match ns { "Put" => Some(&self.put_zkbin), "Take" => Some(&self.take_zkbin), _ => None } }
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> { match ns { "Put" => Some(&self.put_pk), "Take" => Some(&self.take_pk), _ => None } }
}

pub struct BoxPutResult { pub call_data: Vec<u8>, pub proof: Proof }
pub struct BoxTakeResult { pub call_data: Vec<u8>, pub proof: Proof }
