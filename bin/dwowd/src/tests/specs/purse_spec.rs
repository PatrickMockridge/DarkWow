//! ContractTestSpec for purse contract.
//!
//! Category: L1 O-Cap Primitive (genesis).
//! Functions: 4 (InitializeV1=0x00 non-ZK, DepositV1=0x01 ZK,
//!                    WithdrawV1=0x02 ZK, BalanceV1=0x03 ZK).
//! Spec: heavyweight-spec.md §5.3.

use dwow_contract_test_harness::harness::{ContractHarness, PurseHarness};
use dwow_sdk::crypto::{PURSE_CONTRACT_ID, pasta_prelude::PrimeField};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn purse_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(PurseHarness::spawn()));

    ContractTestSpec {
        name: "purse",
        is_genesis: true,
        contract_id: *PURSE_CONTRACT_ID,
        harness,
        wasm_bytes: None,
        has_initialize: false,  // PurseHarness doesn't expose initialize() yet
        initialize: None,
        needs_coinbase_coordination: false,
        state_trees: harness.state_trees(),
        endpoints: vec![
            EndpointSpec {
                name: "DepositV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                state_tree: "purse_roots",
                state_key_fn: Box::new(|| {
                    pallas::Base::from(1u64).to_repr().to_vec()
                }),
                generate: Box::new(|| {
                    let r = harness.deposit(100)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "WithdrawV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                state_tree: "nullifiers",
                state_key_fn: Box::new(|| {
                    pallas::Base::from(1u64).to_repr().to_vec()
                }),
                generate: Box::new(|| {
                    let r = harness.withdraw(50)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "BalanceV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                state_tree: "purse_roots",
                state_key_fn: Box::new(|| {
                    pallas::Base::from(1u64).to_repr().to_vec()
                }),
                generate: Box::new(|| {
                    let r = harness.balance()?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
