//! ContractTestSpec for purse contract.
//!
//! Category: L1 O-Cap Primitive (genesis).
//! Functions: 4 (InitializeV1=0x00 non-ZK, DepositV1=0x01 ZK,
//!                    WithdrawV1=0x02 ZK, BalanceV1=0x03 ZK).
//! Spec: heavyweight-spec.md §5.3.

use dwow_contract_test_harness::harness::{ContractHarness, PurseHarness};
use dwow_sdk::crypto::{MerkleNode, MerkleTree, PURSE_CONTRACT_ID, pasta_prelude::PrimeField, poseidon_hash};
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
        setup: None,
        endpoints: vec![
            EndpointSpec {
                name: "DepositV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    // Recompute the new Merkle root after DepositV1 appends new_leaf.
                    // new_leaf nl = poseidon_hash([dml=5, pid=1, nb=100, sn=0]).
                    let nl = poseidon_hash([pallas::Base::from(5u64), pallas::Base::from(1u64), pallas::Base::from(100u64), pallas::Base::zero()]);
                    let mut tree = MerkleTree::new(1);
                    tree.append(MerkleNode::from_base(pallas::Base::zero()));
                    tree.append(MerkleNode::from_base(nl));
                    let k = tree.root(0).expect("tree.root").to_bytes().to_vec();
                    let c = *PURSE_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contract_state(c, "purse_roots", &k)?;
                        if r.is_none() { return Err(dwow_core::Error::Custom("WARN [purse::DepositV1]: purse_roots must contain updated root".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new(|| {
                    let r = harness.deposit(100)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "WithdrawV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    // WithdrawV1 nullifier nf = poseidon_hash([dnl=1, os=43, pid=1, sn=0]).
                    let nf = poseidon_hash([pallas::Base::from(1u64), pallas::Base::from(43u64), pallas::Base::from(1u64), pallas::Base::zero()]);
                    let k = nf.to_repr().to_vec();
                    let c = *PURSE_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contract_state(c, "nullifiers", &k)?;
                        if r.is_none() { return Err(dwow_core::Error::Custom("WARN [purse::WithdrawV1]: nullifier must exist after withdrawal".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new(|| {
                    let r = harness.withdraw(50)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
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
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
