//! ContractTestSpec for bridge. Tier: UNDERPOWERED — 7 harness methods.
//! 3 active (update_config, deposit, withdraw), 4 chain-specific deposits pending.
//! Sinsemilla Merkle data mismatch may cause proof failures for deposit/withdraw.
use dwow_contract_test_harness::harness::{BridgeHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey, MerkleNode};
use dwow_sdk::pasta::pallas;
use dwow_bridge_contract::model::ExternalChain;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn bridge_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BridgeHarness::spawn()));
    let h: &BridgeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/bridge/dwow_bridge_contract.wasm");
    let secret = pallas::Base::from(100u64);
    let recipient = PublicKey::from_secret(SecretKey::from_bytes([3u8; 32]).unwrap());
    ContractTestSpec {
        name: "bridge", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![
            mk_ep("UpdateConfigV1", true, Box::new(move || {
                let r = h.update_config(100, 50, 6, 1_000_000, 500_000, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(99u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("DepositV1", true, Box::new(move || {
                let r = h.deposit(secret, 10000, recipient, 1, pallas::Base::from(200u64), pallas::Base::from(300u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32], ExternalChain::Monero, 0).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AztDepositV1", true, Box::new(move || {
                let r = h.azt_deposit(secret, pallas::Base::from(40u64), pallas::Base::from(50u64), 5000, 1, recipient, 1, pallas::Base::from(60u64), pallas::Base::from(70u64), pallas::Base::from(80u64), 100, 200, 6, pallas::Base::from(90u64), pallas::Base::from(100u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("LtcDepositV1", true, Box::new(move || {
                let r = h.ltc_deposit(secret, 5000, recipient, 1, pallas::Base::from(200u64), pallas::Base::from(210u64), 0, pallas::Base::from(220u64), 300, 6, 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("XmrDepositV1", true, Box::new(move || {
                let r = h.xmr_deposit(secret, pallas::Base::from(300u64), 5000, recipient, 1, pallas::Base::from(310u64), 400, 0, pallas::Base::from(320u64), pallas::Base::from(330u64), 6, pallas::Base::from(340u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ZecDepositV1", true, Box::new(move || {
                let r = h.zec_deposit(secret, pallas::Base::from(400u64), pallas::Base::from(410u64), 5000, recipient, 1, pallas::Base::from(420u64), pallas::Base::from(430u64), pallas::Base::from(440u64), 500, pallas::Base::from(450u64), pallas::Base::from(460u64), pallas::Base::from(470u64), 6, 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("WithdrawV1", true, Box::new(move || {
                let r = h.withdraw(secret, 5000, pallas::Base::from(400u64), pallas::Base::from(500u64), pallas::Base::from(600u64), [pallas::Base::from(0u64); 4], 0, 10, 1).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
