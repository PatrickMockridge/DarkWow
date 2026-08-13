use dwow_contract_test_harness::harness::{ContractHarness, DexHarness};
use dwow_sdk::crypto::SecretKey; use dwow_sdk::pasta::pallas;
use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

fn dex_vs(tree: &'static str) -> Option<Box<dyn Fn(&HeavyweightPipeline) -> dwow_core::Result<()> + 'static>> {
    let cid = dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp");
    Some(Box::new(move |chain: &HeavyweightPipeline| {
        let r = chain.query_contract_state(cid, tree, &[])?;
        if r.is_none() { return Err(dwow_core::Error::Custom(format!("dex {tree} not found"))); }
        Ok(())
    }))
}

pub fn dex_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DexHarness::spawn()));
    let h: &DexHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/dex/dwow_dex_contract.wasm");
    let s = pallas::Base::from(100u64); let ot = pallas::Base::from(1u64); let rt = pallas::Base::from(2u64);
    let sig = || SecretKey::from_bytes([1u8;32]).unwrap();
    ContractTestSpec { name: "dex", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm), has_initialize: true,
        initialize: Some(Box::new(move || {
            let r = h.initialize(s, ot, 1000, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
            Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
        })),
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec { name: "CreateSwapV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: dex_vs("info"),
                generate: Box::new(move || {
                    let r = h.create_swap(s, ot, 1000, rt, 500, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            mk_ep("AcceptSwapV1", true, Box::new(move || {
                let r = h.accept_swap(pallas::Base::from(1u64), pallas::Base::from(1u64), s, ot, 1000, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            EndpointSpec { name: "ExecuteSwapV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: dex_vs("nullifiers"),
                generate: Box::new(move || {
                    let r = h.execute_swap(s, ot, 1000, pallas::Base::from(10u64), s, rt, 500, pallas::Base::from(20u64), 1000, pallas::Base::from(1u64), pallas::Base::from(2u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            mk_ep("CancelSwapV1", true, Box::new(move || {
                let r = h.cancel_swap(pallas::Base::from(1u64), pallas::Base::from(1u64), s, ot, 1000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ExecuteSwapFeeV1", true, Box::new(move || {
                let r = h.execute_swap_fee(s, ot, pallas::Base::from(1000u64), pallas::Base::from(10u64), s, rt, pallas::Base::from(500u64), pallas::Base::from(20u64), pallas::Base::from(500u64), pallas::Base::from(30u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ExecuteSwapSlippageV1", true, Box::new(move || {
                let r = h.execute_swap_slippage(s, ot, pallas::Base::from(1000u64), pallas::Base::from(10u64), s, rt, pallas::Base::from(500u64), pallas::Base::from(20u64), pallas::Base::from(500u64), pallas::Base::from(50u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SetTransparencyLevelV1", false, Box::new(move || {
                let r = h.set_transparency_level(0).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("UpdateConfigV1", false, Box::new(move || {
                let r = h.update_config(200, 50).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
