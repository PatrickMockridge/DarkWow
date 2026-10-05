use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        blind::ScalarBlind, pasta_prelude::{CurveAffine, PrimeField},
        pedersen_commitment_u64, poseidon_hash, ContractId, MerkleNode, MerkleTree, Nullifier,
    },
    pasta::{group::{Curve, GroupEncoding}, pallas},
};
use rand::SeedableRng;

use crate::harness::ContractHarness;

/// The commitment a single-call endpoint's proof must bind to (`OBL-C198`). One helper because
/// every builder needs the same value, and a second derivation is a second value waiting to drift
/// (`safety.md` RC5). It is taken over the ordered call set (here, one call); the node recomputes
/// over the same bytes, including the contract id, so the harness is given the deployed id.
fn commitment_of(contract_id: &ContractId, call_data: &[u8]) -> pallas::Base {
    let call = dwow_sdk::tx::ContractCall { contract_id: *contract_id, data: call_data.to_vec() };
    dwow_sdk::crypto::util::tx_commitment([&call])
}

pub struct PurseHarness { balance_zkbin: ZkBinary, balance_pk: ProvingKey, deposit_zkbin: ZkBinary, deposit_pk: ProvingKey, withdraw_zkbin: ZkBinary, withdraw_pk: ProvingKey, contract_id: ContractId }

impl PurseHarness {
    pub fn spawn(contract_id: ContractId) -> Self {
        let dz = ZkBinary::decode(include_bytes!("../../../purse/proof/deposit.zk.bin"), false).expect("decode deposit");
        let wz = ZkBinary::decode(include_bytes!("../../../purse/proof/withdraw.zk.bin"), false).expect("decode withdraw");
        let bz = ZkBinary::decode(include_bytes!("../../../purse/proof/balance.zk.bin"), false).expect("decode balance");
        let dp = ProvingKey::build(dz.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&dz).expect("empty deposit"), &dz)).expect("pk deposit");
        let wp = ProvingKey::build(wz.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&wz).expect("empty withdraw"), &wz)).expect("pk withdraw");
        let bp = ProvingKey::build(bz.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&bz).expect("empty balance"), &bz)).expect("pk balance");
        Self { balance_zkbin: bz, balance_pk: bp, deposit_zkbin: dz, deposit_pk: dp, withdraw_zkbin: wz, withdraw_pk: wp, contract_id }
    }
    pub fn circuits(&self) -> Vec<&'static str> { vec!["BalanceV2", "DepositV2", "WithdrawV2"] }

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

    fn coords(pt: pallas::Point) -> Result<(pallas::Base, pallas::Base)> {
        let a = pt.to_affine(); let c = a.coordinates().into_option().ok_or(dwow_core::Error::Custom("identity point".into()))?; Ok((*c.x(), *c.y()))
    }

    /// The deposit a whole transaction is — the form purse's own spec uses.
    ///
    /// Binding over this call **alone** is correct here and only here: when the deposit *is* the
    /// transaction, the node's ordered call set is `[self]`. A deposit used as a **child** must
    /// not use this — see [`Self::deposit_prepare`].
    pub fn deposit(&self, amount: u64) -> Result<PurseDepositResult> {
        let plan = self.deposit_prepare(amount)?;
        let tc = commitment_of(&self.contract_id, &plan.call_data);
        plan.prove(tc)
    }

    /// A `deposit` call built but not yet proven, for use as a **child** (`OBL-C198`).
    ///
    /// [`Self::deposit`] binds its proof to the commitment over this call alone. A child cannot:
    /// the node hashes the whole ordered call set in DFS post-order, and the parent's bytes come
    /// *after* the child's, so the value the child must bind to does not exist yet when the child
    /// is built. This stops before the proof and hands the caller the call data; the caller
    /// assembles the set, takes the commitment, and calls [`PurseDepositPlan::prove`].
    ///
    /// Additive on purpose, exactly as `MultiSigHarness::finalize_prepare` is: rewriting
    /// `deposit` to take a child set would change every caller in purse's own spec, where binding
    /// over the call alone is already right.
    pub fn deposit_prepare(&self, amount: u64) -> Result<PurseDepositPlan> {
        let dnl=pallas::Base::from(1u64);let dml=pallas::Base::from(5u64);let dss=pallas::Base::from(7u64);
        let os=pallas::Base::from(42u64);let op=poseidon_hash([dss,os]);let pid=pallas::Base::from(1u64);
        // The purse identity the circuit publishes: `poseidon(4, owner_pub, asset_id, purse_id)` — the
        // derivation `balance.zk` constrains and `balance()` below computes, so a parent that knows
        // these three can require this operation to be *this* purse's.
        let tid=pallas::Base::from(1u64);
        // **Domain 4** — `DOMAIN_COMMITMENT`, the constant `balance()` below uses and `balance.zk`
        // constrains. `dml` (5) is the merkle-leaf domain and is one away; a proof built with it is a
        // valid field element that simply never matches, which is how this read on its first run:
        // `L2 proof verify … invalid proof: call[0] namespace 'Deposit'`.
        let dpi=poseidon_hash([pallas::Base::from(4u64),op,tid,pid]);
        let sn=pallas::Base::zero();let ob:u64=0;let nb:u64=amount;let tn=pallas::Base::from(300u64);
        let nf=poseidon_hash([dnl,os,pid,sn]);
        let nl=poseidon_hash([dml,pid,pallas::Base::from(nb),sn + pallas::Base::from(1u64),op]);let ol=poseidon_hash([dml,pid,pallas::Base::from(ob),sn,op]);
        let (lp,p,root)=Self::build_root(ol);
        let er_base: pallas::Base = root.inner();
        let obl=ScalarBlind::from_u64(1u64);let dbl=ScalarBlind::from_u64(2u64);let nbl=ScalarBlind::from_u64(3u64);
        let oc=pedersen_commitment_u64(ob,obl.clone());let nc=pedersen_commitment_u64(nb,nbl.clone());
        let (ocx,ocy)=Self::coords(oc)?;let (ncx,ncy)=Self::coords(nc)?;
        let mpa:[MerkleNode;32]=p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path array".into()))?;
        let nf_val=Nullifier::from_bytes(nf.to_repr()).map_err(|e| dwow_core::Error::Custom(format!("nullifier: {e:?}")))?;
        // The balances are gone from the params struct and from the call data, and the amount with
        // them: it is `off_wire` in the manifest, so a harness that built it from the manifest would
        // drop it, and this one builds the *contract's* wire directly. The circuit still sees all
        // three as witnesses (the `w` vector above).
        let pr=dwow_purse_contract::model::DepositParams{nullifier:nf_val,expected_root:root,new_leaf:MerkleNode::from_base(nl),old_commit_x:ocx,old_commit_y:ocy,new_commit_x:ncx,new_commit_y:ncy,leaf_pos:dwow_purse_contract::model::MerklePosition::new(lp),merkle_path:mpa,proof:vec![],tx_nonce:tn,derived_purse_id:dpi};
        let mut cd=vec![0x01u8];cd.extend_from_slice(&pr.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
        // Self-addressed AEAD note (wallet.md §2.3, §A.8.2): purse_capability note
        // carries {asset_id, value, balance_blind, commitment, purse_id, state_nonce}
        // encrypted to the holder's key — matches the manifest note_schema order.
        // `value` is a `pallas::Base` now, matching the manifest's note_schema — the note carries the
        // balance the circuit computed (`witness = 5`), not a `u64` copied from a wire param. The
        // field order still matches the schema's order, which is what the scan decodes against.
        #[derive(dwow_serial::SerialEncodable)]
        struct PurseNote { asset_id: pallas::Base, value: pallas::Base, balance_blind: pallas::Scalar, commitment: pallas::Base, purse_id: pallas::Base, state_nonce: pallas::Base }
        let note = PurseNote { asset_id: tid, value: pallas::Base::from(nb), balance_blind: nbl.inner(), commitment: nl, purse_id: pid, state_nonce: sn + pallas::Base::from(1u64) };
        let owner_pk = dwow_sdk::crypto::keypair::PublicKey::from_secret(dwow_sdk::crypto::keypair::SecretKey::from_base(os));
        let encrypted = dwow_sdk::crypto::note::AeadEncryptedNote::encrypt(&note, &owner_pk, &mut rand::rngs::StdRng::seed_from_u64(0)).map_err(|e| dwow_core::Error::Custom(format!("note encrypt: {e:?}")))?;
        let mut note_bytes=vec![];dwow_serial::Encodable::encode(&encrypted,&mut note_bytes).map_err(|e| dwow_core::Error::Custom(format!("note encode: {e:?}")))?;
        cd.extend_from_slice(&note_bytes);
        // `OBL-C198`: the call data is finished here and the proof is built later, over it.
        // `tx_binding` derives from a commitment that covers these very bytes, and the commitment
        // excludes proofs, which is what makes the order solvable. What stood here was
        // `tc = 200`, a literal.
        //
        // The pair is last among the *instances*, but the witness vector follows `deposit.zk`'s
        // own declaration order, in which `tid` and `dpi` are declared *after* the pair. So the
        // head stops before the pair and `witnesses_tail` carries the two that follow it;
        // `prove` splices the pair back between them, which is the order the circuit reads.
        Ok(PurseDepositPlan{
            witnesses_head: vec![Witness::Base(Value::known(pid)),Witness::Base(Value::known(pallas::Base::from(ob))),Witness::Scalar(Value::known(obl.inner())),Witness::Base(Value::known(pallas::Base::from(amount))),Witness::Scalar(Value::known(dbl.inner())),Witness::Base(Value::known(pallas::Base::from(nb))),Witness::Scalar(Value::known(nbl.inner())),Witness::Base(Value::known(sn)),Witness::Base(Value::known(nf)),Witness::Base(Value::known(er_base)),Witness::Base(Value::known(nl)),Witness::Base(Value::known(ocx)),Witness::Base(Value::known(ocy)),Witness::Base(Value::known(ncx)),Witness::Base(Value::known(ncy)),Witness::Base(Value::known(os)),Witness::Base(Value::known(op)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path".into()))?))],
            witnesses_tail: vec![Witness::Base(Value::known(tid)),Witness::Base(Value::known(dpi))],
            public_head: vec![nf,er_base,ocx,ocy,ncx,ncy,nl,dpi],
            tx_nonce: tn,
            call_data: cd,
            deposit_zkbin: self.deposit_zkbin.clone(),
            deposit_pk: self.deposit_pk.clone(),
        })
    }

    pub fn withdraw(&self, amount: u64) -> Result<PurseWithdrawResult> {
        let dnl=pallas::Base::from(1u64);let dtb=pallas::Base::from(3u64);let dml=pallas::Base::from(5u64);let dss=pallas::Base::from(7u64);
        // Single owner (os=42). state_nonce=1 is the consumed (deposit's output)
        // nonce; the produced nonce is state_nonce+1, computed in-circuit.
        let os=pallas::Base::from(42u64);let op=poseidon_hash([dss,os]);let pid=pallas::Base::from(1u64);
        // The purse identity the circuit publishes: `poseidon(4, owner_pub, asset_id, purse_id)` — the
        // derivation `balance.zk` constrains and `balance()` below computes, so a parent that knows
        // these three can require this operation to be *this* purse's.
        let tid=pallas::Base::from(1u64);
        // **Domain 4** — `DOMAIN_COMMITMENT`, the constant `balance()` below uses and `balance.zk`
        // constrains. `dml` (5) is the merkle-leaf domain and is one away; a proof built with it is a
        // valid field element that simply never matches, which is how this read on its first run:
        // `L2 proof verify … invalid proof: call[0] namespace 'Deposit'`.
        let dpi=poseidon_hash([pallas::Base::from(4u64),op,tid,pid]);
        let sn=pallas::Base::from(1u64);let ob:u64=100;let nb:u64=ob-amount;let tn=pallas::Base::from(300u64);
        let nf=poseidon_hash([dnl,os,pid,sn]);
        let nl=poseidon_hash([dml,pid,pallas::Base::from(nb),sn + pallas::Base::from(1u64),op]);let ol=poseidon_hash([dml,pid,pallas::Base::from(ob),sn,op]);
        let (lp,p,root)=Self::build_root(ol);
        let er_base: pallas::Base = root.inner();
        // obl=5: Pedersen blind balance: obl = nbl + wbl (5 = 3 + 2)
        let obl=ScalarBlind::from_u64(5u64);let wbl=ScalarBlind::from_u64(2u64);let nbl=ScalarBlind::from_u64(3u64);
        let oc=pedersen_commitment_u64(ob,obl.clone());let nc=pedersen_commitment_u64(nb,nbl.clone());
        let (ocx,ocy)=Self::coords(oc)?;let (ncx,ncy)=Self::coords(nc)?;
        let mpa:[MerkleNode;32]=p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path array".into()))?;
        let nf_val=Nullifier::from_bytes(nf.to_repr()).map_err(|e| dwow_core::Error::Custom(format!("nullifier: {e:?}")))?;
        let pr=dwow_purse_contract::model::WithdrawParams{nullifier:nf_val,expected_root:root,new_leaf:MerkleNode::from_base(nl),old_commit_x:ocx,old_commit_y:ocy,new_commit_x:ncx,new_commit_y:ncy,leaf_pos:dwow_purse_contract::model::MerklePosition::new(lp),merkle_path:mpa,proof:vec![],tx_nonce:tn,derived_purse_id:dpi};
        let mut cd=vec![0x02u8];cd.extend_from_slice(&pr.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
        // Self-addressed AEAD note — same {asset_id, value, balance_blind, commitment,
        // purse_id, state_nonce} schema (matches the manifest note_schema order).
        // `value` is a `pallas::Base` now, matching the manifest's note_schema — the note carries the
        // balance the circuit computed (`witness = 5`), not a `u64` copied from a wire param. The
        // field order still matches the schema's order, which is what the scan decodes against.
        #[derive(dwow_serial::SerialEncodable)]
        struct PurseNote { asset_id: pallas::Base, value: pallas::Base, balance_blind: pallas::Scalar, commitment: pallas::Base, purse_id: pallas::Base, state_nonce: pallas::Base }
        let note = PurseNote { asset_id: tid, value: pallas::Base::from(nb), balance_blind: nbl.inner(), commitment: nl, purse_id: pid, state_nonce: sn + pallas::Base::from(1u64) };
        let owner_pk = dwow_sdk::crypto::keypair::PublicKey::from_secret(dwow_sdk::crypto::keypair::SecretKey::from_base(os));
        let encrypted = dwow_sdk::crypto::note::AeadEncryptedNote::encrypt(&note, &owner_pk, &mut rand::rngs::StdRng::seed_from_u64(0)).map_err(|e| dwow_core::Error::Custom(format!("note encrypt: {e:?}")))?;
        let mut note_bytes=vec![];dwow_serial::Encodable::encode(&encrypted,&mut note_bytes).map_err(|e| dwow_core::Error::Custom(format!("note encode: {e:?}")))?;
        cd.extend_from_slice(&note_bytes);
        // `OBL-C198`: prove LAST over the finished call data — see the note in `deposit`.
        let tc: pallas::Base = commitment_of(&self.contract_id, &cd);
        let tb=poseidon_hash([dtb,tc,tn]);
        let w=vec![Witness::Base(Value::known(pid)),Witness::Base(Value::known(pallas::Base::from(ob))),Witness::Scalar(Value::known(obl.inner())),Witness::Base(Value::known(pallas::Base::from(amount))),Witness::Scalar(Value::known(wbl.inner())),Witness::Base(Value::known(pallas::Base::from(nb))),Witness::Scalar(Value::known(nbl.inner())),Witness::Base(Value::known(sn)),Witness::Base(Value::known(nf)),Witness::Base(Value::known(er_base)),Witness::Base(Value::known(nl)),Witness::Base(Value::known(ocx)),Witness::Base(Value::known(ocy)),Witness::Base(Value::known(ncx)),Witness::Base(Value::known(ncy)),Witness::Base(Value::known(os)),Witness::Base(Value::known(op)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path".into()))?)),Witness::Base(Value::known(tc)),Witness::Base(Value::known(tn)),Witness::Base(Value::known(tb)),Witness::Base(Value::known(tid)),Witness::Base(Value::known(dpi))];
        let pi=vec![nf,er_base,ocx,ocy,ncx,ncy,nl,dpi,tb,tn];let c=ZkCircuit::new(w,&self.withdraw_zkbin);
        let proof = if dwow_purse_contract::deterministic_zk_enabled() {
            Proof::create(&self.withdraw_pk, &[c], &pi, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.withdraw_pk, &[c], &pi, rand::rngs::OsRng)
        }.map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {e:?}")))?;
        Ok(PurseWithdrawResult{call_data:cd,proof})
    }

    pub fn balance(&self) -> Result<PurseBalanceResult> {
        let dtc=pallas::Base::from(2u64);let dtb=pallas::Base::from(3u64);let dcc=pallas::Base::from(4u64);let dml=pallas::Base::from(5u64);let dss=pallas::Base::from(7u64);
        let os=pallas::Base::from(42u64);let op=poseidon_hash([dss,os]);let pid=pallas::Base::from(1u64);
        // The purse identity, with the **commitment** domain (`dcc` = 4) — the derivation
        // `balance.zk` and the two other circuits publish. A copy of this line using `dml` (5) sat
        // above it for a moment while this harness was patched, and it was harmless only because the
        // correct line shadows it; the two constants differ by one and the wrong one produces a valid
        // field element that simply never matches.
        let tid=pallas::Base::from(1u64);let bal:u64=50;let sn=pallas::Base::from(2u64);let tblind=pallas::Base::from(5u64);
        let tn_=pallas::Base::from(300u64);
        let dpi=poseidon_hash([dcc,op,tid,pid]);let tcom=poseidon_hash([dtc,tid,tblind]);
        // Balance queries the CURRENT purse leaf (balance 50, nonce 2 — the withdraw
        // output). On-chain tree after Deposit(100)+Withdraw(50) is
        // [zero, leaf(100, nonce 1), leaf(50, nonce 2)]; witness the latest leaf.
        let deposit_leaf = poseidon_hash([dml, pid, pallas::Base::from(100u64), pallas::Base::from(1u64), op]);
        let withdraw_leaf = poseidon_hash([dml, pid, pallas::Base::from(bal), pallas::Base::from(2u64), op]);
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(deposit_leaf));
        tree.append(MerkleNode::from_base(withdraw_leaf));
        let mk = tree.mark().expect("tree.mark");
        let p: Vec<MerkleNode> = tree.witness(mk, 0).expect("tree.witness");
        let lp = u32::try_from(u64::from(mk)).expect("position");
        let root = tree.root(0).expect("tree.root");
        let er_base: pallas::Base = root.inner();
        let bbl=ScalarBlind::from_u64(1u64);let bc=pedersen_commitment_u64(bal,bbl.clone());
        let (bcx,bcy)=Self::coords(bc)?;
        let mpa:[MerkleNode;32]=p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path array".into()))?;
        // `bal_typed` stood here — a `Balance::new(bal)` bound to nothing. `BalanceParams` never
        // carried a balance (the circuit's is a witness), so it was dead before the balances left
        // the struct, and the struct leaving is what made it visible.
        let pr=dwow_purse_contract::model::BalanceParams{derived_purse_id:dpi,expected_root:root,token_commit:tcom,balance_commit_x:bcx,balance_commit_y:bcy,leaf_pos:dwow_purse_contract::model::MerklePosition::new(lp),merkle_path:mpa,proof:vec![],tx_nonce:tn_};
        let mut cd=vec![0x03u8];cd.extend_from_slice(&pr.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
        // `OBL-C198`: prove LAST over the finished call data — see the note in `deposit`. The push
        // order already has the pair last, matching `balance.zk`.
        let tc_: pallas::Base = commitment_of(&self.contract_id, &cd);
        let tb=poseidon_hash([dtb,tc_,tn_]);
        let w=vec![Witness::Base(Value::known(pid)),Witness::Base(Value::known(tid)),Witness::Base(Value::known(pallas::Base::from(bal))),Witness::Scalar(Value::known(bbl.inner())),Witness::Base(Value::known(sn)),Witness::Base(Value::known(dpi)),Witness::Base(Value::known(er_base)),Witness::Base(Value::known(tcom)),Witness::Base(Value::known(bcx)),Witness::Base(Value::known(bcy)),Witness::Base(Value::known(os)),Witness::Base(Value::known(op)),Witness::Base(Value::known(tblind)),Witness::Uint32(Value::known(lp)),Witness::MerklePath(Value::known(p.clone().try_into().map_err(|_| dwow_core::Error::Custom("path".into()))?)),Witness::Base(Value::known(tc_)),Witness::Base(Value::known(tn_)),Witness::Base(Value::known(tb))];
        let pi=vec![dpi,er_base,bcx,bcy,tcom,tb,tn_];let c=ZkCircuit::new(w,&self.balance_zkbin);
        let proof = if dwow_purse_contract::deterministic_zk_enabled() {
            Proof::create(&self.balance_pk, &[c], &pi, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.balance_pk, &[c], &pi, rand::rngs::OsRng)
        }.map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {e:?}")))?;
        Ok(PurseBalanceResult{call_data:cd,proof})
    }
}

/// A prepared purse `deposit`, proved against a caller-supplied commitment — see
/// [`PurseHarness::deposit_prepare`] for why a child needs this and a whole transaction does not.
pub struct PurseDepositPlan {
    witnesses_head: Vec<Witness>,
    /// The witnesses `deposit.zk` declares *after* the pair. `prove` splices the pair back between
    /// the head and these — the declaration order is not the instance order.
    witnesses_tail: Vec<Witness>,
    public_head: Vec<pallas::Base>,
    tx_nonce: pallas::Base,
    /// The call data the commitment must cover. The caller needs it to build the `ContractCall`
    /// this plan signs for.
    pub call_data: Vec<u8>,
    deposit_zkbin: ZkBinary,
    deposit_pk: ProvingKey,
}

impl PurseDepositPlan {
    /// Prove against `tx_commitment` — the commitment over the whole ordered call set the node
    /// will hash, not just this call.
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<PurseDepositResult> {
        // `DOMAIN_TX_BINDING` = 3, the constant the arm and every other builder use.
        let tx_binding=poseidon_hash([pallas::Base::from(3u64),tx_commitment,self.tx_nonce]);
        let mut w=self.witnesses_head;
        w.push(Witness::Base(Value::known(tx_commitment)));
        w.push(Witness::Base(Value::known(self.tx_nonce)));
        w.push(Witness::Base(Value::known(tx_binding)));
        w.extend(self.witnesses_tail);
        // constrain_instance order: nf, er_base, ocx, ocy, ncx, ncy, nl, dpi, tx_binding, tx_nonce —
        // the pair is last, which is the convention the node reads.
        let mut pi=self.public_head;
        pi.push(tx_binding);
        pi.push(self.tx_nonce);
        let c=ZkCircuit::new(w,&self.deposit_zkbin);
        let proof = if dwow_purse_contract::deterministic_zk_enabled() {
            Proof::create(&self.deposit_pk, &[c], &pi, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.deposit_pk, &[c], &pi, rand::rngs::OsRng)
        }.map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {e:?}")))?;
        Ok(PurseDepositResult{call_data:self.call_data,proof})
    }
}

impl ContractHarness for PurseHarness {
    fn name(&self) -> &str { "purse" }
    fn circuits(&self) -> Vec<&'static str> { self.circuits() }
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> { match ns { "BalanceV2"=>Some(&self.balance_zkbin),"DepositV2"=>Some(&self.deposit_zkbin),"WithdrawV2"=>Some(&self.withdraw_zkbin),_=>None } }
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> { match ns { "BalanceV2"=>Some(&self.balance_pk),"DepositV2"=>Some(&self.deposit_pk),"WithdrawV2"=>Some(&self.withdraw_pk),_=>None } }
}

pub struct PurseDepositResult { pub call_data: Vec<u8>, pub proof: Proof }
pub struct PurseWithdrawResult { pub call_data: Vec<u8>, pub proof: Proof }
pub struct PurseBalanceResult { pub call_data: Vec<u8>, pub proof: Proof }

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_sdk::pasta::group::Group;

    #[test]
    fn coords_identity_is_err() {
        assert!(PurseHarness::coords(pallas::Point::identity()).is_err());
    }
}
