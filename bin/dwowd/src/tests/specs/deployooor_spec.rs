//! ContractTestSpec for deployooor contract. Spec: heavyweight-spec.md §5.2.

use dwow_contract_test_harness::harness::{ContractHarness, DeployooorHarness};
use dwow_sdk::crypto::{DEPLOYOOOR_CONTRACT_ID, Keypair, SecretKey, PublicKey};
use dwow_serial::Encodable;

use crate::tests::blockchain::HeavyweightPipeline;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn deployooor_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DeployooorHarness::spawn()));
    let h: &DeployooorHarness = harness;

    ContractTestSpec {
        name: "deployooor",
        is_genesis: true,
        contract_id: *DEPLOYOOOR_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec {
                name: "DeployV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let c = *DEPLOYOOOR_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contracts_tree(&c.to_bytes())?;
                        assert!(r.is_some(), "DeployV1: deployed WASM must exist in contracts tree");
                        Ok(())
                    }
                })),
                generate: Box::new(|| {
                    let secret = SecretKey::from_bytes([9u8; 32]).unwrap();
                    let public = PublicKey::from_secret(secret.clone());
                    let kp = Keypair { secret, public };
                    let wasm = include_bytes!("../../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm");
                    let deploy = h.build_deploy_call(kp, wasm.to_vec(), vec![0x00])?;
                    let mut cd = vec![0x00];
                    deploy.params.encode(&mut cd)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: cd, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "LockV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(|| {
                    let secret = SecretKey::from_bytes([9u8; 32]).unwrap();
                    let public = PublicKey::from_secret(secret.clone());
                    let kp = Keypair { secret, public };
                    let lock = h.build_lock_call(kp)?;
                    let mut cd = vec![0x01];
                    cd.extend_from_slice(&lock.params.encode());
                    Ok(EndpointResult { call_data: cd, proofs: vec![] })
                }),
            },
        ],
    }
}
