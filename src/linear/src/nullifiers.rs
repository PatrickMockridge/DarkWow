//! OBL-C146 — derive a transaction's spent nullifier set from its calls.
//!
//! `Transaction.nullifiers` is caller-declared; the wallet fills it by hand and the chain replay
//! gate, `check_coinbase_maturity` and the mempool filter read it. Nothing reconciled it against the
//! calls, so a transaction could declare fewer nullifiers than it spends and bypass the maturity
//! rule and the mempool double-spend filter.
//!
//! This module derives the set the L1 calls actually spend, from the same params their proofs bind.
//! The node rejects a declaration that omits an L1 spend; the mempool uses the same derivation.
//!
//! **Scope and limit.** The four L1 contracts (NativeToken, PromissoryNote, Purse, Box) are decoded
//! here because the node links their models and their params carry the spent nullifier. A non-L1
//! call is not decoded; `all_l1` is then false and the caller requires only that the derived L1 set
//! is a *subset* of the declaration. The non-L1 consequence is not established by this module.
//!
//! Claim transactions — the coinbase (transaction 0 with the PoWRewardV1 classifier) and the
//! block-closing FeeCollectV1 plate — are excluded from the declared-set comparison because their
//! nullifier is a creation marker. They are not exempt from derivation: their L1 calls must derive
//! the empty set, so a spend smuggled into a claim transaction is refused.
//!
//! This is a consensus rule: it changes which blocks are valid. See
//! `doc/src/arch/consensus/consensus.md` §Declared Nullifier Set Reconciliation.

use std::collections::BTreeSet;

use dwow_sdk::crypto::{
    Nullifier, BOX_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID, PURSE_CONTRACT_ID,
};

use crate::error::LinearError;
use crate::transaction::ContractCall;
use crate::Result;

/// The nullifiers the call's params carry as spent inputs, and whether every call is an L1 call.
///
/// The set is empty for an L1 call that spends nothing (mint/pow-reward/fee-collect/initialize/
/// balance). An unknown selector on an L1 contract is an error rather than a silent empty set: a
/// declaration cannot be checked against a call the derivation does not understand.
pub fn derive_l1_nullifiers(calls: &[ContractCall]) -> Result<(BTreeSet<Nullifier>, bool)> {
    let mut out = BTreeSet::new();
    let mut all_l1 = true;

    for call in calls {
        let selector = *call.data.first().ok_or_else(|| {
            LinearError::BlockIsInvalid("call data is empty: no function selector".into())
        })?;
        let payload = &call.data[1..];

        if call.contract_id == *NATIVE_TOKEN_CONTRACT_ID {
            use dwow_native_token_contract::model::{
                BurnParamsV1, FeeParamsV3, SpendParamsV1, TransferParamsV1,
            };
            use dwow_native_token_contract::NativeTokenFunction;

            let func = NativeTokenFunction::try_from(selector).map_err(|e| {
                LinearError::BlockIsInvalid(format!("native_token selector invalid: {e}"))
            })?;
            match func {
                NativeTokenFunction::BurnV1 => {
                    let p = BurnParamsV1::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("native_token burn params decode failed: {e}"))
                    })?;
                    out.extend(p.inputs.into_iter().map(|i| i.nullifier));
                }
                NativeTokenFunction::TransferV1 => {
                    let p = TransferParamsV1::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("native_token transfer params decode failed: {e}"))
                    })?;
                    out.extend(p.inputs.into_iter().map(|i| i.nullifier));
                }
                NativeTokenFunction::SpendV1 => {
                    let p = SpendParamsV1::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("native_token spend params decode failed: {e}"))
                    })?;
                    out.insert(p.input.nullifier);
                }
                NativeTokenFunction::FeeV3 => {
                    let p = FeeParamsV3::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("native_token fee params decode failed: {e}"))
                    })?;
                    out.insert(p.input.nullifier);
                }
                NativeTokenFunction::MintV1
                | NativeTokenFunction::PoWRewardV1
                | NativeTokenFunction::FeeCollectV1
                | NativeTokenFunction::UncleMintV1 => {}
            }
        } else if call.contract_id == *PROMISSORY_NOTE_CONTRACT_ID {
            use dwow_promissory_note_contract::model::{
                OtcSwapParamsV1, RedeemParamsV1, RevokeParamsV1, TransferParamsV1,
            };
            use dwow_promissory_note_contract::PromissoryNoteFunction;

            let func = PromissoryNoteFunction::try_from(selector).map_err(|e| {
                LinearError::BlockIsInvalid(format!("promissory_note selector invalid: {e}"))
            })?;
            match func {
                PromissoryNoteFunction::RedeemV1 => {
                    let p = RedeemParamsV1::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("promissory_note redeem params decode failed: {e}"))
                    })?;
                    out.insert(p.input.nullifier);
                }
                PromissoryNoteFunction::RevokeV1 => {
                    let p = RevokeParamsV1::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("promissory_note revoke params decode failed: {e}"))
                    })?;
                    out.extend(p.inputs.into_iter().map(|i| i.nullifier));
                }
                PromissoryNoteFunction::TransferV1 => {
                    let p = TransferParamsV1::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("promissory_note transfer params decode failed: {e}"))
                    })?;
                    out.extend(p.inputs.into_iter().map(|i| i.nullifier));
                }
                PromissoryNoteFunction::OtcSwapV1 => {
                    let p = OtcSwapParamsV1::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("promissory_note otc_swap params decode failed: {e}"))
                    })?;
                    out.extend(p.inputs.into_iter().map(|i| i.nullifier));
                }
                PromissoryNoteFunction::RegisterTypeV1 | PromissoryNoteFunction::IssueV1 => {}
            }
        } else if call.contract_id == *PURSE_CONTRACT_ID {
            use dwow_purse_contract::model::{DepositParams, WithdrawParams};
            use dwow_purse_contract::PurseFunction;

            let func = PurseFunction::try_from(selector).map_err(|e| {
                LinearError::BlockIsInvalid(format!("purse selector invalid: {e}"))
            })?;
            match func {
                PurseFunction::Deposit => {
                    let p = DepositParams::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("purse deposit params decode failed: {e}"))
                    })?;
                    out.insert(p.nullifier);
                }
                PurseFunction::Withdraw => {
                    let p = WithdrawParams::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("purse withdraw params decode failed: {e}"))
                    })?;
                    out.insert(p.nullifier);
                }
                PurseFunction::Balance | PurseFunction::Initialize => {}
            }
        } else if call.contract_id == *BOX_CONTRACT_ID {
            use dwow_box_contract::model::{PutParams, TakeParams};
            use dwow_box_contract::BoxFunction;

            let func = BoxFunction::try_from(selector).map_err(|e| {
                LinearError::BlockIsInvalid(format!("box selector invalid: {e}"))
            })?;
            match func {
                BoxFunction::Put => {
                    let p = PutParams::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("box put params decode failed: {e}"))
                    })?;
                    out.insert(p.nullifier);
                }
                BoxFunction::Take => {
                    let p = TakeParams::decode(payload).map_err(|e| {
                        LinearError::BlockIsInvalid(format!("box take params decode failed: {e}"))
                    })?;
                    out.insert(p.nullifier);
                }
                BoxFunction::Initialize => {}
            }
        } else {
            all_l1 = false;
        }
    }

    Ok((out, all_l1))
}

/// Reconcile a transaction's declared set with the L1 calls.
///
/// A transaction whose calls are all L1 must declare exactly the derived set. A transaction with a
/// non-L1 call must declare at least the derived L1 set; its non-L1 nullifiers are its own and are
/// not derivable here.
pub fn reconcile_declared(
    derived: &BTreeSet<Nullifier>,
    declared: &[Nullifier],
    all_l1: bool,
) -> std::result::Result<(), String> {
    let declared: BTreeSet<Nullifier> = declared.iter().copied().collect();
    if all_l1 {
        if declared != *derived {
            return Err(format!(
                "declared nullifier set ({} entries) does not equal the set the L1 calls spend \
                 ({} entries)",
                declared.len(),
                derived.len()
            ))
        }
    } else if !derived.is_subset(&declared) {
        let missing: Vec<String> = derived
            .difference(&declared)
            .map(|n| hex::encode(n.to_bytes()))
            .collect();
        return Err(format!(
            "declared nullifier set omits L1 spend(s): {}",
            missing.join(", ")
        ))
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use dwow_native_token_contract::model::{
        fee::FeeParamsV3, ClearInput, Commitment, CommitmentAttributes, Input as NtInput,
        Output as NtOutput, PoWRewardParamsV1, DRKW_ASSET_ID,
    };
    use dwow_sdk::blockchain::{FeeAmount, FeeTier};
    use dwow_sdk::crypto::keypair::SecretKey;
    use dwow_sdk::crypto::note::AeadEncryptedNote;
    use dwow_sdk::crypto::pasta_prelude::Group;
    use dwow_sdk::crypto::{BaseBlind, Blind, ContractId, FuncId, MerkleNode, PublicKey};
    use dwow_sdk::pasta::pallas;

    pub(crate) fn nullifier(byte: u8) -> Nullifier {
        Nullifier::from_bytes([byte; 32]).expect("canonical, non-zero")
    }

    /// A genuine FeeV3 call, encoded by the contract's own encoder. The input nullifier is the
    /// one the proof binds; nothing else about the fixture matters to the derivation.
    pub(crate) fn fee_v3(nf: Nullifier) -> ContractCall {
        let pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(1u64)));
        let zero = pallas::Base::from(0u64);
        let params = FeeParamsV3 {
            input: NtInput {
                value_commit: pallas::Point::default(),
                token_commit: zero,
                nullifier: nf,
                merkle_root: MerkleNode::new(zero),
                user_data_enc: zero,
                spend_hook: FuncId::none(),
                signature_public: pk,
            },
            output: NtOutput {
                value_commit: pallas::Point::default(),
                token_commit: zero,
                commitment: Commitment::from_attributes(
                    &pk,
                    1000,
                    DRKW_ASSET_ID,
                    FuncId::none(),
                    zero,
                    BaseBlind::ZERO,
                ),
                nullifier: Some(nf),
                note: AeadEncryptedNote { ciphertext: vec![0u8; 32], ephem_public: pk },
            },
            fee: FeeAmount::new(1),
            tier: FeeTier::LOW,
            fee_value_commit: pallas::Point::default(),
            tx_nonce: zero,
        };
        let mut data = vec![dwow_native_token_contract::NativeTokenFunction::FeeV3 as u8];
        data.extend_from_slice(&params.encode().expect("fee params encode"));
        ContractCall { contract_id: *NATIVE_TOKEN_CONTRACT_ID, data }
    }

    /// A genuine PoWRewardV1 call whose params decode and carry `nf` as both the coin's claim
    /// nullifier and the output nullifier. Consensus does not verify the reward arithmetic in
    /// `connect_block` (that is `block_acceptor`'s job), so this fixture only has to be
    /// structurally decodable — which is what makes the claim record in `chain_state` land.
    pub(crate) fn pow_reward(nf: Nullifier) -> ContractCall {
        let pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(2u64)));
        let zero = pallas::Base::from(0u64);
        let commitment = Commitment::from_attributes(
            &pk,
            1000,
            DRKW_ASSET_ID,
            FuncId::none(),
            zero,
            Blind(pallas::Base::zero()),
        );
        let params = PoWRewardParamsV1 {
            input: ClearInput {
                value: 1000,
                asset_id: DRKW_ASSET_ID.inner(),
                value_blind: Blind(pallas::Scalar::zero()),
                token_blind: BaseBlind::ZERO,
                signature_public: pk,
            },
            total_pin: 0,
            effective_value: 1000,
            commitment_attrs: CommitmentAttributes {
                version: 0,
                public_key: pk,
                value: 1000,
                asset_id: DRKW_ASSET_ID,
                spend_hook: FuncId::none(),
                user_data: zero,
                blind: Blind(pallas::Base::zero()),
            },
            output: NtOutput {
                value_commit: pallas::Point::identity(),
                token_commit: zero,
                commitment,
                nullifier: Some(nf),
                note: AeadEncryptedNote { ciphertext: vec![0u8; 32], ephem_public: pk },
            },
            nullifier: nf,
            expected_cumulative_supply: 0,
            old_cumulative_commit: pallas::Point::identity(),
            old_cumulative_blind: pallas::Scalar::zero(),
            new_cumulative_commit: pallas::Point::identity(),
            tx_nonce: zero,
        };
        let mut data = vec![dwow_native_token_contract::NativeTokenFunction::PoWRewardV1 as u8];
        data.extend_from_slice(&params.encode().expect("pow reward params encode"));
        ContractCall { contract_id: *NATIVE_TOKEN_CONTRACT_ID, data }
    }

    fn call(contract_id: ContractId, data: Vec<u8>) -> ContractCall {
        ContractCall { contract_id, data }
    }

    #[test]
    fn derivation_reads_the_fee_v3_input_nullifier() {
        let nf = nullifier(0x11);
        let (derived, all_l1) = derive_l1_nullifiers(&[fee_v3(nf)]).expect("derives");
        assert!(all_l1, "FeeV3 is an L1 call");
        assert_eq!(derived, BTreeSet::from([nf]));
    }

    #[test]
    fn derivation_unions_every_call_and_accepts_a_spend_nothing_arm() {
        let a = nullifier(0x21);
        let b = nullifier(0x22);
        let pow_reward = call(*NATIVE_TOKEN_CONTRACT_ID, vec![0x05]);
        let (derived, all_l1) =
            derive_l1_nullifiers(&[fee_v3(a), pow_reward, fee_v3(b)]).expect("derives");
        assert!(all_l1);
        assert_eq!(derived, BTreeSet::from([a, b]));
    }

    #[test]
    fn exact_declaration_is_admitted_and_both_mismatches_are_refused() {
        let nf = nullifier(0x31);
        let other = nullifier(0x32);
        let (derived, all_l1) = derive_l1_nullifiers(&[fee_v3(nf)]).expect("derives");

        reconcile_declared(&derived, &[nf], all_l1).expect("an exact declaration is admitted");

        let err = reconcile_declared(&derived, &[], all_l1)
            .expect_err("an omitted L1 spend must be refused");
        assert!(err.contains("does not equal"), "got {err}");

        let err = reconcile_declared(&derived, &[nf, other], all_l1)
            .expect_err("an extra declared nullifier must be refused");
        assert!(err.contains("does not equal"), "got {err}");
    }

    #[test]
    fn non_l1_call_requires_a_superset_and_says_so() {
        let nf = nullifier(0x33);
        let foreign = nullifier(0x34);
        let (derived, all_l1) = derive_l1_nullifiers(&[
            fee_v3(nf),
            call(ContractId::from_base(pallas::Base::from(7u64)), vec![0x00]),
        ])
        .expect("derives");
        assert!(!all_l1, "a non-L1 call is not derived here");

        reconcile_declared(&derived, &[nf, foreign], all_l1)
            .expect("the non-L1 transaction's extra nullifier is its own business");

        let err = reconcile_declared(&derived, &[foreign], all_l1)
            .expect_err("omitting the L1 spend must be refused");
        assert!(err.contains("omits L1 spend"), "got {err}");
    }

    #[test]
    fn unknown_l1_selector_and_empty_call_data_are_errors() {
        let err = derive_l1_nullifiers(&[call(*NATIVE_TOKEN_CONTRACT_ID, vec![0xFF])])
            .expect_err("an unknown selector on an L1 contract cannot be reconciled");
        assert!(format!("{err:?}").contains("selector invalid"), "got {err:?}");

        let err = derive_l1_nullifiers(&[call(*NATIVE_TOKEN_CONTRACT_ID, vec![])])
            .expect_err("a call with no selector cannot be reconciled");
        assert!(format!("{err:?}").contains("no function selector"), "got {err:?}");
    }
}
