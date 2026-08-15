use dwow_contract_test_harness::harness::{ContractHarness, LotteryHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey}; use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;
pub fn lottery_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(LotteryHarness::spawn()));
    let h: &LotteryHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/lottery/dwow_lottery_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_bytes([1u8;32]).unwrap());
    let ps = pallas::Base::from(2u64);
    ContractTestSpec { name: "lottery", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm), has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            mk_ep("InitializeV1", false, Box::new(move || {
                let r = h.initialize(100, 200, 1000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("CommitTicketV1", true, Box::new(move || {
                let r = h.commit_ticket(pk, pallas::Base::from(1u64), vec![1u8,2,3,4,5,6], ps, 100, pallas::Base::from(3u64), pallas::Base::from(4u64), pallas::Base::from(5u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RevealTicketV1", true, Box::new(move || {
                let r = h.reveal_ticket(pk, 100, ps, pallas::Base::from(3u64), pallas::Base::from(4u64), pallas::Base::from(5u64), pallas::Base::from(6u64), vec![1u8,2,3,4,5,6])
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("DrawWinnersV1", false, Box::new(move || {
                let r = h.draw_winners(pallas::Base::from(1u64), pallas::Base::from(99u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("ClaimPrizeV1", false, Box::new(move || {
                let r = h.claim_prize(pallas::Base::from(1u64), ps).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("ExpireLotteryV1", false, Box::new(move || {
                let r = h.expire_lottery(pallas::Base::from(1u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
        ],
    }
}
