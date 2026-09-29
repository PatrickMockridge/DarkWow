//! Nullifier replay rejection verification.
//!
//! Used by: Categories 1-3 (all contracts with ZK-gated functions).
//! Spec: heavyweight-spec.md §3.6 (Nullifier Replay Rejection).

use dwow_core::zk::Proof;
use dwow_core::Result;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::ChildCall;

/// Verify that resubmitting the same ZK-gated call is rejected.
///
/// First submission must have already succeeded (caller's responsibility).
/// Second submission with identical call_data+proofs MUST be rejected.
///
/// **The children ride with the replay, and for a long time they did not** (`OBL-C193`) — this is
/// the module's own version of the class `uniform_runner`'s rejection comment names. It submits
/// through `submit_single_call_block`, which takes no child list, so for the twelve specs whose
/// first ZK endpoint requires a child the replayed call was turned away for the *missing child*
/// (`Custom(22)` on the endpoint) and the nullifier this control exists to test was never
/// consulted. The assertion held for a reason its own name did not give.
///
/// **The blamed contract is a parameter, not a constant.** Calls are ordered DFS post-order, so
/// children execute first and the endpoint last: a replay is therefore normally refused by whichever
/// child holds the spent note, and by the endpoint itself only when it carries no child. The caller
/// knows which and must state it — a `ContractId` rather than a text needle, because the
/// per-contract error enums overlap and `Custom(N)` means different things in different contracts.
pub async fn verify_nullifier_replay(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    call_data: &[u8],
    proofs: Vec<Proof>,
    is_zk: bool,
    children: Vec<ChildCall>,
    blamed: ContractId,
) -> Result<()> {
    // Second submission with same call_data MUST be rejected
    let replay_result = super::block_submission::submit_multi_call_block(
        chain, cid, harness, call_data, proofs, is_zk, children,
    ).await;

    let err = replay_result.expect_err(
        "INFRA-FAIL [nullifier_replay]: nullifier replay MUST be rejected — second submission with identical call_data succeeded");
    let text = format!("{err}");
    let who = format!("(contract {blamed})");
    assert!(text.contains(&who),
        "INFRA-FAIL [nullifier_replay]: the replay was refused by some call other than the expected \
         {} — so the spent nullifier was never consulted and this control proved nothing. \
         Full error: {}",
        who, text);

    Ok(())
}
