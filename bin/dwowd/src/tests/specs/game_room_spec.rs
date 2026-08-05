//! ContractTestSpec for game_room. Tier: STUB — all endpoints use empty_witnesses.
//! Per RG-24 (§4.11): NONE may appear as active specs.
//! Tracking: game_room-client-proofs
use dwow_contract_test_harness::harness::{GameRoomHarness, ContractHarness};
use crate::tests::uniform_runner::*;

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
        endpoints: vec![], // ALL empty_witnesses — tracked at game_room-client-proofs
    }
}
