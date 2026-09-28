use crate::error::PurseError;
use dwow_sdk::{blockchain::SerializedLen, crypto::{pasta_prelude::PrimeField, MerkleNode, Nullifier}, error::ContractError, pasta::pallas};

// ============================================================================
// TOTAL BYTE READS
// ============================================================================
//
// Every `decode` below validates its buffer length before slicing, which makes each `data[a..b]`
// *provably* in-bounds. But provable is not free: the bounds check still compiles, and its panic
// location — `Location { file: &'static str, line: u32 }` — is a string and an integer in the
// contract artifact's data section, which neither `strip` nor `--release` removes. `get` +
// `try_into` removes the check rather than asserting it away, and `saturating_add` keeps the offset
// arithmetic itself total. Copied from native_token's model, which holds the same family.

/// Read exactly `N` bytes at `offset` — total: `get` + `try_into`, no index and no unwrap.
fn read_field<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], ContractError> {
    data.get(offset..offset.saturating_add(N))
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| {
            ContractError::IoError(format!(
                "truncated field: need {N} bytes at offset {offset}, buffer has {}",
                data.len()
            ))
        })
}

// `read_byte` lived here — for the three one-byte proof-length prefixes and the `Purse` record's
// version byte. All four are gone, so the only caller that needed a bare byte is gone and the helper
// with it.

/// Borrow exactly `len` bytes at `offset` — total, for the same reason as [`read_field`]. Borrowed
/// rather than copied, so a nested `decode` can take the sub-slice directly.
fn read_slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8], ContractError> {
    data.get(offset..offset.saturating_add(len)).ok_or_else(|| {
        ContractError::IoError(format!(
            "truncated field: need {len} bytes at offset {offset}, buffer has {}",
            data.len()
        ))
    })
}

// `PurseId` lived here. The `Purse` record was its only user, and both are gone — `purse_id` is not on
// the wire (`privacy.md` §5.5), the host never names one, and the prover's `derived = "purse_id"` rule
// is a different thing entirely: a `DerivedRule` in `src/sdk/src/prover.rs`, not this type.

// `Amount` and `Balance` lived here — two `u64` newtypes for the balances the wire carried, with
// `Amount`'s non-zero invariant enforced in its constructor. **Both are gone, and the invariant moved
// to the circuit.** They had no reader outside their own tests once the balances left the call data:
// `deposit_amount` is a parameter that is `off_wire`, so the host never sees it and `Amount::new`'s
// zero check could not run at all; `deposit.zk` now carries `less_than_strict(ZERO, deposit_amount)`
// beside `withdraw.zk:61`'s, which is where a consensus rule belongs (`privacy.md` §5.5: the circuit
// proves everything, the handlers compute nothing). A `u64` newtype for a value that is a circuit
// witness and a wallet record, and never a wire field, has nothing to name.

fn read_merkle_node(data: &[u8]) -> Result<MerkleNode, ContractError> {
    if data.len() != 32 { return Err(ContractError::IoError(format!("read_merkle_node: expected 32 bytes, got {}", data.len()))); }
    let arr: [u8; 32] = read_field::<32>(data, 0)?;
    MerkleNode::from_bytes(arr).ok_or_else(|| ContractError::IoError("read_merkle_node: invalid MerkleNode".into()))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)] pub struct MerklePosition(u32);
impl MerklePosition {
    pub fn new(v: u32) -> Self { Self(v) }
    pub fn inner(&self) -> u32 { self.0 }
    pub fn to_le_bytes(&self) -> [u8; 4] { self.0.to_le_bytes() }
    pub fn from_le_bytes(b: [u8; 4]) -> Self { Self(u32::from_le_bytes(b)) }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)] pub struct StateNonce(pallas::Base);
impl StateNonce {
    pub fn new(v: pallas::Base) -> Self { Self(v) }
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_repr(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_repr(b: [u8; 32]) -> Option<Self> { pallas::Base::from_repr(b).into_option().map(Self) }
}

// The `Purse` record — `version`, `purse_id`, `token_commit`, `balance_commit`, `owner_commit`, 129
// bytes — lived here, described in its own doc comment as a "future schema. Not yet used by
// entrypoints — currently only exercised in integration tests."
//
// **Removed rather than wired, and the reason is a measurement.** The re-wire programme called for
// wiring it: `initialize` would create it and deposit/withdraw would read it, which is what would make
// the host-level owner check constructible. Nothing can read it. A deposit or withdrawal carries no
// owner — `DepositParams` has no such field, and the owner is bound *inside* the circuit, which is the
// point of the leaf being a commitment: `new_leaf = poseidon_hash(DOMAIN, purse_id, new_balance,
// state_nonce, owner_pub)` cannot be inverted by the host, and `purse_id` and `state_nonce` were
// deliberately taken off the wire. So an owner check would need a plaintext owner field added to a
// genesis contract's call data, which is exactly what `privacy.md` §2 and §5.5 and
// `scripts/check-l1-wire-conformance.sh` exist to prevent — and it would be a *second* source of truth
// beside the proof that already binds the owner, the argument `OBL-C101` records for not re-counting a
// threshold the child contract already enforces.
//
// A record written and never read is dead state in a new place, so it goes. Everything the record
// claimed to hold is in the state that is actually used: the merkle leaf holds the balance and the
// owner commitment, and `Purse::derive_*` on the client side holds the derivation.

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { if data.len()!=32 { return Err(ContractError::IoError(format!("read_base: expected 32 bytes, got {}", data.len()))); } Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 0)?)).ok_or_else(||ContractError::IoError("invalid base".into())) }
type MerklePath = [MerkleNode; 32];

// ============================================================================
// DEPOSIT — hdr=228
//
// `purse_id` and `state_nonce` are NOT on the wire. `privacy.md` §2 promises an
// observer sees "only a nullifier and a Merkle root — not which resource was
// operated on", and §5.5 says the object id "is never a public input"; both were in
// the plaintext call data, which the transaction hash commits to byte for byte.
// Measured before removal: nothing read them — not the host (purse/src/entrypoint),
// not the circuit as an instance — and the wallet's prover takes them from its own
// record (`CapRecord.object_id`, `.state_nonce`) through the `note:` witness sources.
//
// **The balances are gone too, and the note's `value` is why they had stayed.** §2 also
// promises "not how much", and until 2026-09-28 all three were written as plaintext 8-byte LE
// at the front of every deposit and withdraw while the same doc said the balance was hidden
// in a Pedersen commitment. They stayed because the note's `value` field was declared `u64`
// and `encode_params_values` refuses a value of another type, while a `witness = N` source
// yields the circuit's `Base` — so a balance that left the params could not reach the note.
// The note's `value` is `pallas_base` now and the three route as follows:
//
//   * `old_balance` — `note:value`, the wallet's own record of the purse it is spending.
//   * `deposit_amount`/`withdraw_amount` — still a param, but `off_wire`: the caller supplies
//     it, the prover binds it, `encode_params_values` does not write it. This is the fourth
//     quadrant `OBL-C179` names, and the reason it had to be built.
//   * `new_balance` — `derived:base_add:1,3` (deposit) / `derived:base_sub:1,3` (withdraw),
//     the circuit's own arithmetic applied to the two above. `deposit.zk:94-95` constrains the
//     same sum against the witness, so the generic prover computes what the circuit checks.
//
// Nothing in the contract ever read them: `deposit_metadata` publishes nullifier,
// expected_root, new_leaf, the four commitment coordinates, `tx_binding`, `tx_nonce` and
// `derived_purse_id`, and `exec`/`apply` read none of the three. They were in the struct
// because the *decoder* read them, which is exactly the wire this removes.
// ============================================================================

#[derive(Debug, Clone)] pub struct DepositParams {
    pub nullifier: Nullifier, pub expected_root: MerkleNode, pub new_leaf: MerkleNode,
    pub old_commit_x: pallas::Base, pub old_commit_y: pallas::Base, pub new_commit_x: pallas::Base, pub new_commit_y: pallas::Base,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
    /// `poseidon_hash(4, owner_pub, asset_id, purse_id)` — the purse this operation names.
    ///
    /// **It replaced `asset_id`, which was on the wire for a reason that no longer holds.** The field
    /// was there because the *note* needed it and the circuit had no `asset_id` witness to source it
    /// from; it is slot 22 now, the note reads it from there, and it travels in no call data. What
    /// takes its place is a one-way function of the purse id — the only form `privacy.md` §5.5 permits
    /// to be public — and it is what lets a parent require a deposit into a *named* purse: the circuit
    /// constrains it against the same derivation `balance.zk` publishes.
    ///
    /// `scripts/check-l1-wire-conformance.sh` never declared `asset_id`: its rule covers witness-map
    /// `param:` slots, and a plain params field is invisible to it. So that gate's count of ten was a
    /// floor, and this change lowers it without the gate's number moving.
    pub derived_purse_id: pallas::Base,
}

impl dwow_serial::Encodable for DepositParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        // hdr = 228 (was 252): the three balances left the wire. `nullifier` is at offset 0 now, so
        // every offset below moved by 24.
        let hdr=228usize; let pb:Vec<u8>=self.merkle_path.iter().flat_map(|n|n.to_bytes()).collect();
        let mut b=Vec::with_capacity(hdr+pb.len()+1+self.proof.len()+64);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.expected_root.to_bytes()); b.extend_from_slice(&self.new_leaf.to_bytes());
        b.extend_from_slice(&self.old_commit_x.to_repr()); b.extend_from_slice(&self.old_commit_y.to_repr());
        b.extend_from_slice(&self.new_commit_x.to_repr()); b.extend_from_slice(&self.new_commit_y.to_repr());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes()); b.extend_from_slice(&pb);
        // `SerializedLen`, not a bare `u8`. The prefix was one byte, decoded with `read_byte`, so a
        // proof of 256 bytes or more could not be encoded at all (`u8::try_from` failed) and a
        // *truncated* frame could not be told from a short one — the same class `OBL-C150` records for
        // `dao_escrow`, and the reason `SerializedLen` exists: it is always four bytes, it refuses a
        // length that does not fit, and its decoder is the exact inverse of its encoder.
        let pl = SerializedLen::try_from_len(self.proof.len())?;
        b.extend_from_slice(&pl.to_le_bytes());
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); b.extend_from_slice(&self.derived_purse_id.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr=228usize; if data.len()<=hdr+1024usize { return Err(PurseError::DecodeFailure{field:"DepositParams".into()}.into()); }
        let nf={let a:[u8;32]=read_field::<32>(data,0)?; Nullifier::from_bytes(a)?};
        let er=read_merkle_node(read_slice(data,32,32)?)?; let nl=read_merkle_node(read_slice(data,64,32)?)?;
        let ocx=read_base(read_slice(data,96,32)?)?; let ocy=read_base(read_slice(data,128,32)?)?;
        let ncx=read_base(read_slice(data,160,32)?)?; let ncy=read_base(read_slice(data,192,32)?)?;
        let lp=MerklePosition::from_le_bytes(read_field::<4>(data,224)?);
        let mut mp=[MerkleNode::from_base(pallas::Base::zero());32]; for (i,slot) in mp.iter_mut().enumerate() { *slot=read_merkle_node(read_slice(data,hdr.saturating_add(i.saturating_mul(32)),32)?)?; }
        let pe=hdr+1024usize; let pl=SerializedLen::from_le_bytes(read_field::<4>(data,pe)?).to_usize();
        let p2 = pe.saturating_add(SerializedLen::ENCODED_SIZE).saturating_add(pl);
        // **A minimum, not an equality, and that is load-bearing.** The params are the *front* of the
        // payload, not the whole of it: call data is `selector ++ params ++ AEAD note`, and the note is
        // what the wallet scans for the produced state. An equality here refuses every real call — which
        // is how this was found, on the first deposit of `test_heavyweight_purse`. The length prefix
        // still does its job: it says where the proof ends, so the three trailing fields are read at the
        // right offsets whatever follows them.
        if data.len() < p2.saturating_add(96) { return Err(PurseError::DecodeFailure{field:"DepositParams".into()}.into()); }
        let proof=read_slice(data,pe+SerializedLen::ENCODED_SIZE,pl)?.to_vec();
        let tb=read_base(read_slice(data,p2,32)?)?; let tn=read_base(read_slice(data,p2+32,32)?)?; let dpi=read_base(read_slice(data,p2+64,32)?)?;
        Ok(DepositParams{nullifier:nf,expected_root:er,new_leaf:nl,old_commit_x:ocx,old_commit_y:ocy,new_commit_x:ncx,new_commit_y:ncy,leaf_pos:lp,merkle_path:mp,proof,tx_binding:tb,tx_nonce:tn,derived_purse_id:dpi})
    }
}

#[derive(Debug, Clone)] pub struct DepositUpdate { pub nullifier: Nullifier, pub new_leaf: MerkleNode }
impl dwow_serial::Encodable for DepositUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositUpdate { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut v=Vec::with_capacity(64); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.new_leaf.to_bytes()); Ok(v) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len()!=64 { return Err(PurseError::DecodeFailure{field:"DepositUpdate".into()}.into()); } Ok(DepositUpdate{nullifier:{let a:[u8;32]=read_field::<32>(data,0)?; Nullifier::from_bytes(a)?}, new_leaf:read_merkle_node(read_slice(data,32,32)?)?}) } }

// ============================================================================
// WITHDRAW
// ============================================================================

#[derive(Debug, Clone)] pub struct WithdrawParams {
    pub nullifier: Nullifier, pub expected_root: MerkleNode, pub new_leaf: MerkleNode,
    pub old_commit_x: pallas::Base, pub old_commit_y: pallas::Base, pub new_commit_x: pallas::Base, pub new_commit_y: pallas::Base,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
    /// See `DepositParams::derived_purse_id` — the field is the same in the same position, and
    /// `asset_id` left the wire in the same change.
    pub derived_purse_id: pallas::Base,
}

// WithdrawParams shares DepositParams' wire format (hdr=228: the header is the eight fixed fields
// before the merkle path — 32+32+32 + 32+32+32+32 + 4 — and **the balances are no longer among
// them**). `withdraw_amount` aliases `deposit_amount` through the delegation below, and both are
// `off_wire` now, so neither appears in either width.
// encode/decode delegates to DepositParams with withdraw_amount aliased as
// deposit_amount. This is intentional — the two operations have identical
// payload layout. If DepositParams' encoding changes, verify WithdrawParams
// round-trip tests in tests/integration.rs still pass.
impl WithdrawParams { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { DepositParams{nullifier:self.nullifier,expected_root:self.expected_root,new_leaf:self.new_leaf,old_commit_x:self.old_commit_x,old_commit_y:self.old_commit_y,new_commit_x:self.new_commit_x,new_commit_y:self.new_commit_y,leaf_pos:self.leaf_pos,merkle_path:self.merkle_path,proof:self.proof.clone(),tx_binding:self.tx_binding,tx_nonce:self.tx_nonce,derived_purse_id:self.derived_purse_id}.encode() } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { let dp = DepositParams::decode(data)?; Ok(WithdrawParams{nullifier:dp.nullifier,expected_root:dp.expected_root,new_leaf:dp.new_leaf,old_commit_x:dp.old_commit_x,old_commit_y:dp.old_commit_y,new_commit_x:dp.new_commit_x,new_commit_y:dp.new_commit_y,leaf_pos:dp.leaf_pos,merkle_path:dp.merkle_path,proof:dp.proof,tx_binding:dp.tx_binding,tx_nonce:dp.tx_nonce,derived_purse_id:dp.derived_purse_id}) } }
impl dwow_serial::Encodable for WithdrawParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = DepositParams{nullifier:self.nullifier,expected_root:self.expected_root,new_leaf:self.new_leaf,old_commit_x:self.old_commit_x,old_commit_y:self.old_commit_y,new_commit_x:self.new_commit_x,new_commit_y:self.new_commit_y,leaf_pos:self.leaf_pos,merkle_path:self.merkle_path,proof:self.proof.clone(),tx_binding:self.tx_binding,tx_nonce:self.tx_nonce,derived_purse_id:self.derived_purse_id}.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

#[derive(Debug, Clone)] pub struct WithdrawUpdate { pub nullifier: Nullifier, pub new_leaf: MerkleNode }
impl dwow_serial::Encodable for WithdrawUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawUpdate { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut v=Vec::with_capacity(64); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.new_leaf.to_bytes()); Ok(v) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len()!=64 { return Err(PurseError::DecodeFailure{field:"WithdrawUpdate".into()}.into()); } Ok(WithdrawUpdate{nullifier:{let a:[u8;32]=read_field::<32>(data,0)?; Nullifier::from_bytes(a)?}, new_leaf:read_merkle_node(read_slice(data,32,32)?)?}) } }

// ============================================================================
// BALANCE — hdr=164
//
// Same reduction as DEPOSIT above, and for the same reason: `purse_id`, `asset_id`,
// `balance` and `state_nonce` are in the wallet's record (`CapRecord.object_id`,
// `.asset_id`, `.value`, `.state_nonce`), no host reads them, and publishing them
// told an observer which purse, which token and how much. What remains is the
// read-only operation's public inputs.
// ============================================================================

#[derive(Debug, Clone)] pub struct BalanceParams {
    pub derived_purse_id: pallas::Base, pub expected_root: MerkleNode, pub token_commit: pallas::Base,
    pub balance_commit_x: pallas::Base, pub balance_commit_y: pallas::Base,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for BalanceParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for BalanceParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl BalanceParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let hdr=164usize; let pb:Vec<u8>=self.merkle_path.iter().flat_map(|n|n.to_bytes()).collect();
        let mut b=Vec::with_capacity(hdr+pb.len()+1+self.proof.len()+64);
        b.extend_from_slice(&self.derived_purse_id.to_repr()); b.extend_from_slice(&self.expected_root.to_bytes());
        b.extend_from_slice(&self.token_commit.to_repr()); b.extend_from_slice(&self.balance_commit_x.to_repr());
        b.extend_from_slice(&self.balance_commit_y.to_repr()); b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&pb);
        // `SerializedLen`, for the reason stated at `DepositParams::encode` — the third and last bare
        // `u8` proof prefix in this file.
        let pl = SerializedLen::try_from_len(self.proof.len())?;
        b.extend_from_slice(&pl.to_le_bytes());
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr=164usize; if data.len()<=hdr+1024usize { return Err(PurseError::DecodeFailure{field:"BalanceParams".into()}.into()); }
        let dpi=read_base(read_slice(data,0,32)?)?; let er=read_merkle_node(read_slice(data,32,32)?)?; let tc=read_base(read_slice(data,64,32)?)?;
        let bcx=read_base(read_slice(data,96,32)?)?; let bcy=read_base(read_slice(data,128,32)?)?;
        let lp=MerklePosition::from_le_bytes(read_field::<4>(data,160)?);
        let mut mp=[MerkleNode::from_base(pallas::Base::zero());32]; for (i,slot) in mp.iter_mut().enumerate() { *slot=read_merkle_node(read_slice(data,hdr.saturating_add(i.saturating_mul(32)),32)?)?; }
        let pe=hdr+1024usize; let pl=SerializedLen::from_le_bytes(read_field::<4>(data,pe)?).to_usize();
        let p2 = pe.saturating_add(SerializedLen::ENCODED_SIZE).saturating_add(pl);
        // A minimum, for the reason stated at `DepositParams::decode`: the payload continues past the
        // params with the caller's note.
        if data.len() < p2.saturating_add(64) { return Err(PurseError::DecodeFailure{field:"BalanceParams".into()}.into()); }
        let proof=read_slice(data,pe+SerializedLen::ENCODED_SIZE,pl)?.to_vec();
        let tb=read_base(read_slice(data,p2,32)?)?; let tn=read_base(read_slice(data,p2+32,32)?)?;
        Ok(BalanceParams{derived_purse_id:dpi,expected_root:er,token_commit:tc,balance_commit_x:bcx,balance_commit_y:bcy,leaf_pos:lp,merkle_path:mp,proof,tx_binding:tb,tx_nonce:tn})
    }
}
