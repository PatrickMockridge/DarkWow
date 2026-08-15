use dwow_contract_test_harness::harness::{ContractHarness, PoolStakeHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey}; use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;
pub fn pool_stake_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(PoolStakeHarness::spawn()));
    let h: &PoolStakeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/pool_stake/dwow_pool_stake_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(10u64)));
    let mpk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(20u64)));
    ContractTestSpec { name: "pool_stake", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm), has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        endpoints: vec![
            mk_ep("CreatePoolV1", true, Box::new(move || {
                let r = h.create_pool(pk, 200, 100).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("JoinPoolV1", true, Box::new(move || {
                let r = h.join_pool(pallas::Base::from(1u64), 10000, [0u8; 32], mpk).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("LeavePoolV1", false, Box::new(move || {
                let r = h.leave_pool(pallas::Base::from(1u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("AllocateCoverageV1", true, Box::new(move || {
                let r = h.allocate_coverage(pallas::Base::from(1u64), mpk, 5000, pallas::Base::from(1u64), [0u8; 32], 1000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SlashCoverageV1", true, Box::new(move || {
                let r = h.slash_coverage(pallas::Base::from(1u64), 2000, pk, mpk).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
