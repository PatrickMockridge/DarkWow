use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        blind::ScalarBlind, pasta_prelude::{CurveAffine, PrimeField},
        pedersen_commitment_u64, poseidon_hash, MerkleNode, MerkleTree,
    },
    pasta::{group::{Curve, GroupEncoding}, pallas},
};
use crate::harness::ContractHarness;

pub struct PurseHarness { balance_zkbin: ZkBinary, balance_pk: ProvingKey, deposit_zkbin: ZkBinary, deposit_pk: ProvingKey, withdraw_zkbin: ZkBinary, withdraw_pk: ProvingKey }

impl PurseHarness {
    pub fn spawn() -> Self {
        let dz = ZkBinary::decode(include_bytes!("../../../purse/proof/deposit.zk.bin"), false).expect("decode deposit");
        let wz = ZkBinary::decode(include_bytes!("../../../purse/proof/withdraw.zk.bin"), false).expect("decode withdraw");
        let bz = ZkBinary::decode(include_bytes!("../../../purse/proof/balance.zk.bin"), false).expect("decode balance");
        let dp = ProvingKey::build(dz.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&dz).expect("empty deposit"), &dz)).expect("pk deposit");
        let wp = ProvingKey::build(wz.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&wz).expect("empty withdraw"), &wz)).expect("pk withdraw");
        let bp = ProvingKey::build(bz.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&bz).expect("empty balance"), &bz)).expect("pk balance");
        Self { balance_zkbin: bz, balance_pk: bp, deposit_zkbin: dz, deposit_pk: dp, withdraw_zkbin: wz, withdraw_pk: wp }
    }
    pub fn circuits(&self) -> Vec<&'static str> { vec!["Balance", "Deposit", "Withdraw"] }

    fn build_root(leaf: pallas::Base) -> (u32, Vec<MerkleNode>, pallas::Base) {
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(leaf));
        let mk = tree.mark().expect("tree.mark");
        let p: Vec<MerkleNode> = tree.witness(mk, 0).expect("tree.witness");
        let lp = u32::try_from(u64::from(mk)).expect("position");
        let root = tree.root(0).expect("tree.root");
        (lp, p, root.inner())
    }

    fn coords(pt: pallas::Point) -> (pallas::Base, pallas::Base) {
        let a = pt.to_affine(); let c = a.coordinates().expect("identity point"); (*c.x(), *c.y())
    }

    pub fn deposit(&self, amount: u64) -> Result<PurseDepositResult> {
        let dnl=pallas::Base::from(1u64);let dtb=pallas::Base::from(3u64);let dml=pallas::Base::from(5u64);let dss=pallas::Base::from(7u64);
        let os=pallas::Base::from(42u64);let op=poseidon_hash([dss,os]);let pid=pallas::Base::from(1u64);
        let sn=pallas::Base::zero();let ob:u64=0;let nb:u64=amount;let tc=pallas::Base::from(200u64);let tn=pallas::Base::from(300u64);
        let nf=poseidon_hash([dnl,os,pid,sn]);let tb=poseidon_hash([dtb,tc,tn]);
        let nl=poseidon_hash([dml,pid,pallas::Base::from(nb),sn]);let ol=poseidon_hash([dml,pid,pallas::Base::from(ob),sn]);
        let (lp,p,er)=Self::build_root(ol);
        let obl=ScalarBlind::from(1u64);let dbl=ScalarBlind::from(2u64);let nbl=ScalarBlind::from(3u64);
        let oc=pedersen_commitment_u64(ob,obl.clone());let nc=pedersen_commitment_u64(nb,nbl.clone());
        let (ocx,ocy)=Self::coords(oc);let (ncx,ncy)=Self::coords(nc);
        let w=vec![Witness::Base(Value::known(pid)),Witness::Base(Value::known(pallas::Base::from(ob))),Witness::Scalar(Value::known(obl.inner())),Witness::Base(Value::known(pallas::Base::from(amount))),Witness::Scalar(Value::known(dbl.inner())),Witness::Base(Value::known(pallas::Base::from(nb))),Witness::Scalar(Value::known(nbl.inner())),Witness::Base(Value::known(sn)),Witness::Base(Value::known(nf)),Witness::Base(Value::known(er)),Witness::Base(Value::known(nl)),Witness::Base(Value::known(ocx)),Witness::Base(Value::known(ocy)),Witness::Base(Value::known(ncx)),Witness::Base(Value::known(ncy)),Witness::Base(Value::known(os)),Witness::Base(Value::known(op)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path".into()))?)),Witness::Base(Value::known(tc)),Witness::Base(Value::known(tn)),Witness::Base(Value::known(tb))];
        let pi=vec![nf,er,ocx,ocy,ncx,ncy,nl,tb,tn];let c=ZkCircuit::new(w,&self.deposit_zkbin);
        let proof=Proof::create(&self.deposit_pk,&[c],&pi,rand::rngs::OsRng).map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {e:?}")))?;
        let pb:Vec<u8>=dwow_serial::serialize(&proof);let mpa:[pallas::Base;32]=p.iter().map(|n|n.inner()).collect::<Vec<_>>().try_into().map_err(|_| dwow_core::Error::Custom("path array".into()))?;
        let pr=dwow_purse_contract::model::DepositParams{purse_id:dwow_purse_contract::model::PurseId(pid),old_balance:ob,deposit_amount:amount,new_balance:nb,state_nonce:sn,nullifier:dwow_purse_contract::model::Nullifier(nf),expected_root:MerkleNode::from_base(er),new_leaf:MerkleNode::from_base(nl),old_commit_x:ocx,old_commit_y:ocy,new_commit_x:ncx,new_commit_y:ncy,leaf_pos:lp,merkle_path:mpa,proof:vec![],tx_binding:tb,tx_nonce:tn};
        let mut cd=vec![0x01u8];cd.extend_from_slice(&pr.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);Ok(PurseDepositResult{call_data:cd,proof})
    }

    pub fn withdraw(&self, amount: u64) -> Result<PurseWithdrawResult> {
        let dnl=pallas::Base::from(1u64);let dtb=pallas::Base::from(3u64);let dml=pallas::Base::from(5u64);let dss=pallas::Base::from(7u64);
        let os=pallas::Base::from(42u64);let op=poseidon_hash([dss,os]);let pid=pallas::Base::from(1u64);
        let sn=pallas::Base::from(1u64);let ob:u64=100;let nb:u64=ob-amount;let tc=pallas::Base::from(200u64);let tn=pallas::Base::from(300u64);
        let nf=poseidon_hash([dnl,os,pid,sn]);let tb=poseidon_hash([dtb,tc,tn]);
        let nl=poseidon_hash([dml,pid,pallas::Base::from(nb),sn]);let ol=poseidon_hash([dml,pid,pallas::Base::from(ob),sn]);
        let (lp,p,er)=Self::build_root(ol);
        let obl=ScalarBlind::from(1u64);let wbl=ScalarBlind::from(2u64);let nbl=ScalarBlind::from(3u64);
        let oc=pedersen_commitment_u64(ob,obl.clone());let nc=pedersen_commitment_u64(nb,nbl.clone());
        let (ocx,ocy)=Self::coords(oc);let (ncx,ncy)=Self::coords(nc);
        let w=vec![Witness::Base(Value::known(pid)),Witness::Base(Value::known(pallas::Base::from(ob))),Witness::Scalar(Value::known(obl.inner())),Witness::Base(Value::known(pallas::Base::from(amount))),Witness::Scalar(Value::known(wbl.inner())),Witness::Base(Value::known(pallas::Base::from(nb))),Witness::Scalar(Value::known(nbl.inner())),Witness::Base(Value::known(sn)),Witness::Base(Value::known(nf)),Witness::Base(Value::known(er)),Witness::Base(Value::known(nl)),Witness::Base(Value::known(ocx)),Witness::Base(Value::known(ocy)),Witness::Base(Value::known(ncx)),Witness::Base(Value::known(ncy)),Witness::Base(Value::known(os)),Witness::Base(Value::known(op)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path".into()))?)),Witness::Base(Value::known(tc)),Witness::Base(Value::known(tn)),Witness::Base(Value::known(tb))];
        let pi=vec![nf,er,ocx,ocy,ncx,ncy,nl,tb,tn];let c=ZkCircuit::new(w,&self.withdraw_zkbin);
        let proof=Proof::create(&self.withdraw_pk,&[c],&pi,rand::rngs::OsRng).map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {e:?}")))?;
        let pb:Vec<u8>=dwow_serial::serialize(&proof);let mpa:[pallas::Base;32]=p.iter().map(|n|n.inner()).collect::<Vec<_>>().try_into().map_err(|_| dwow_core::Error::Custom("path array".into()))?;
        let pr=dwow_purse_contract::model::WithdrawParams{purse_id:dwow_purse_contract::model::PurseId(pid),old_balance:ob,withdraw_amount:amount,new_balance:nb,state_nonce:sn,nullifier:dwow_purse_contract::model::Nullifier(nf),expected_root:MerkleNode::from_base(er),new_leaf:MerkleNode::from_base(nl),old_commit_x:ocx,old_commit_y:ocy,new_commit_x:ncx,new_commit_y:ncy,leaf_pos:lp,merkle_path:mpa,proof:vec![],tx_binding:tb,tx_nonce:tn};
        let mut cd=vec![0x02u8];cd.extend_from_slice(&pr.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);Ok(PurseWithdrawResult{call_data:cd,proof})
    }

    pub fn balance(&self) -> Result<PurseBalanceResult> {
        let dtc=pallas::Base::from(2u64);let dtb=pallas::Base::from(3u64);let dcc=pallas::Base::from(4u64);let dml=pallas::Base::from(5u64);let dss=pallas::Base::from(7u64);
        let os=pallas::Base::from(42u64);let op=poseidon_hash([dss,os]);let pid=pallas::Base::from(1u64);
        let tid=pallas::Base::from(1u64);let bal:u64=100;let sn=pallas::Base::from(1u64);let tblind=pallas::Base::from(5u64);
        let tc_=pallas::Base::from(200u64);let tn_=pallas::Base::from(300u64);
        let dpi=poseidon_hash([dcc,op,tid,pid]);let tcom=poseidon_hash([dtc,tid,tblind]);let tb=poseidon_hash([dtb,tc_,tn_]);
        let ol=poseidon_hash([dml,pid,pallas::Base::from(bal),sn]);let (lp,p,er)=Self::build_root(ol);
        let bbl=ScalarBlind::from(1u64);let bc=pedersen_commitment_u64(bal,bbl.clone());
        let (bcx,bcy)=Self::coords(bc);
        let w=vec![Witness::Base(Value::known(pid)),Witness::Base(Value::known(tid)),Witness::Base(Value::known(pallas::Base::from(bal))),Witness::Scalar(Value::known(bbl.inner())),Witness::Base(Value::known(sn)),Witness::Base(Value::known(dpi)),Witness::Base(Value::known(er)),Witness::Base(Value::known(tcom)),Witness::Base(Value::known(bcx)),Witness::Base(Value::known(bcy)),Witness::Base(Value::known(os)),Witness::Base(Value::known(op)),Witness::Base(Value::known(tblind)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path".into()))?)),Witness::Base(Value::known(tc_)),Witness::Base(Value::known(tn_)),Witness::Base(Value::known(tb))];
        let pi=vec![dpi,er,bcx,bcy,tcom,tb,tn_];let c=ZkCircuit::new(w,&self.balance_zkbin);
        let proof=Proof::create(&self.balance_pk,&[c],&pi,rand::rngs::OsRng).map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {e:?}")))?;
        let pb:Vec<u8>=dwow_serial::serialize(&proof);let mpa:[pallas::Base;32]=p.iter().map(|n|n.inner()).collect::<Vec<_>>().try_into().map_err(|_| dwow_core::Error::Custom("path array".into()))?;
        let pr=dwow_purse_contract::model::BalanceParams{purse_id:dwow_purse_contract::model::PurseId(pid),token_id:tid,balance:bal,state_nonce:sn,derived_purse_id:dpi,expected_root:MerkleNode::from_base(er),token_commit:tcom,balance_commit_x:bcx,balance_commit_y:bcy,leaf_pos:lp,merkle_path:mpa,proof:vec![],tx_binding:tb,tx_nonce:tn_};
        let mut cd=vec![0x03u8];cd.extend_from_slice(&pr.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);Ok(PurseBalanceResult{call_data:cd,proof})
    }
}

impl ContractHarness for PurseHarness {
    fn name(&self) -> &str { "purse" }
    fn circuits(&self) -> Vec<&'static str> { self.circuits() }
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> { match ns { "Balance"=>Some(&self.balance_zkbin),"Deposit"=>Some(&self.deposit_zkbin),"Withdraw"=>Some(&self.withdraw_zkbin),_=>None } }
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> { match ns { "Balance"=>Some(&self.balance_pk),"Deposit"=>Some(&self.deposit_pk),"Withdraw"=>Some(&self.withdraw_pk),_=>None } }
}

pub struct PurseDepositResult { pub call_data: Vec<u8>, pub proof: Proof }
pub struct PurseWithdrawResult { pub call_data: Vec<u8>, pub proof: Proof }
pub struct PurseBalanceResult { pub call_data: Vec<u8>, pub proof: Proof }
