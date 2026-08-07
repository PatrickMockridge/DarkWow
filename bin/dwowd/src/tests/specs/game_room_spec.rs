//! ContractTestSpec for game_room. All endpoints use harness methods
//! with empty_witnesses proofs. Client proof modules pending.
use dwow_contract_test_harness::harness::{GameRoomHarness, ContractHarness};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn game_room_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(GameRoomHarness::spawn()));
    let h: &GameRoomHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/game_room/dwow_game_room_contract.wasm");
    ContractTestSpec {
        name: "game_room", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            mk_ep("create_room", true, Box::new(move || {
                let r = h.create_room().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("deposit", true, Box::new(move || {
                let r = h.deposit().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("withdraw", true, Box::new(move || {
                let r = h.withdraw().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("place_bet", true, Box::new(move || {
                let r = h.place_bet().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("raise", true, Box::new(move || {
                let r = h.raise().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("call", true, Box::new(move || {
                let r = h.call().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("fold", true, Box::new(move || {
                let r = h.fold().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("close_pot", true, Box::new(move || {
                let r = h.close_pot().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("settle_pot", true, Box::new(move || {
                let r = h.settle_pot().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("contribute_entropy", true, Box::new(move || {
                let r = h.contribute_entropy().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("claim", true, Box::new(move || {
                let r = h.claim().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
