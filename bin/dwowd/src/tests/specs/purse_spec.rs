//! ContractTestSpec for purse contract.
//!
//! Category: L1 O-Cap Primitive (genesis).
//! Functions: 4 (InitializeV1=0x00 non-ZK, DepositV1=0x01 ZK,
//!                    WithdrawV1=0x02 ZK, BalanceV1=0x03 ZK).
//! Spec: heavyweight-spec.md §5.3.

use dwow_contract_test_harness::harness::{ContractHarness, PurseHarness};
use dwow_sdk::crypto::{PURSE_CONTRACT_ID, pasta_prelude::PrimeField};
use dwow_sdk::pasta::pallas;

use crate::tests::blockchain::HeavyweightPipeline;

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
        endpoints: vec![
            EndpointSpec {
                name: "DepositV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let k = pallas::Base::from(1u64).to_repr().to_vec();
                    let c = *PURSE_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contract_state(c, "purse_roots", &k)?;
                        if r.is_none() { return Err(dwow_core::Error::Custom("WARN [purse::DepositV1]: purse_roots must contain updated root".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new(|| {
                    let r = harness.deposit(100)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "WithdrawV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let k = pallas::Base::from(1u64).to_repr().to_vec();
                    let c = *PURSE_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contract_state(c, "nullifiers", &k)?;
                        if r.is_none() { return Err(dwow_core::Error::Custom("WARN [purse::WithdrawV1]: nullifier must exist after withdrawal".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new(|| {
                    let r = harness.withdraw(50)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "BalanceV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(|| {
                    let r = harness.balance()?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
