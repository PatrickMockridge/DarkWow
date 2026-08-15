//! ContractTestSpec for darkbet_exchange. Tier: UNDERPOWERED — 10 harness methods.
//! 4 real proofs active, 6 empty_witnesses deferred per RG-24.
use dwow_contract_test_harness::harness::{DarkbetExchangeHarness, ContractHarness};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn darkbet_exchange_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DarkbetExchangeHarness::spawn()));
    let h: &DarkbetExchangeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/darkbet_exchange/dwow_darkbet_exchange_contract.wasm");
    let px = pallas::Base::from(1u64); let py = pallas::Base::from(2u64);

    ContractTestSpec {
        name: "darkbet_exchange", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        endpoints: vec![
            mk_ep("CreateMarketV1", true, Box::new(move || {
                let r = h.create_market(px, py, 1000, 1, 0).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("BuyPositionV1", true, Box::new(move || {
                let r = h.buy_position(pallas::Base::from(1u64), px, py, 0, 100, 10, pallas::Scalar::from(1u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ClaimWinningsV1", true, Box::new(move || {
                let r = h.claim_winnings(pallas::Base::from(1u64), pallas::Base::from(2u64), px, py, 0, 20, 1).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AddLiquidityV1", true, Box::new(move || {
                let r = h.add_liquidity(pallas::Base::from(1u64), px, py, 500, 10, pallas::Scalar::from(1u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // empty_witnesses endpoints — client proof modules pending
            mk_ep("PlaceBackV1", true, Box::new(move || {
                let r = h.place_back(pallas::Base::from(1u64), pallas::Base::from(2u64),
                    pallas::Base::from(200u64), pallas::Base::from(1000u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("PlaceLayV1", true, Box::new(move || {
                let r = h.place_lay(pallas::Base::from(1u64), pallas::Base::from(2u64),
                    pallas::Base::from(200u64), pallas::Base::from(1000u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("MatchOrdersV1", true, Box::new(move || {
                let r = h.match_orders(pallas::Base::from(1u64), pallas::Base::from(2u64),
                    pallas::Base::from(3u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ResolveMarketV1", true, Box::new(move || {
                let r = h.resolve_market(pallas::Base::from(1u64), 1, vec![])
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("CancelOrderV1", true, Box::new(move || {
                let r = h.cancel_order(pallas::Base::from(1u64), pallas::Base::from(2u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RemoveLiquidityV1", true, Box::new(move || {
                let r = h.remove_liquidity(pallas::Base::from(1u64), pallas::Base::from(2u64),
                    pallas::Base::from(500u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
