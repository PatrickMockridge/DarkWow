//! ContractTestSpec for deployooor contract. Spec: heavyweight-spec.md §5.2.

use dwow_contract_test_harness::harness::{ContractHarness, DeployooorHarness};
use dwow_sdk::{crypto::{pasta_prelude::PrimeField, ContractId, DEPLOYOOOR_CONTRACT_ID, Keypair, PublicKey, SecretKey}, pasta::pallas};
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
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "DeployV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    // The deployed contract's WASM is stored in the contracts tree
                    // keyed by ContractId::derive_public(kp.public), where kp is the
                    // deploy keypair (SecretKey [9u8;32]) used in generate().
                    let secret = SecretKey::from_bytes([9u8; 32]).unwrap();
                    let public = PublicKey::from_secret(secret);
                    let deployed_cid = ContractId::derive_public(public);
                    let k = deployed_cid.to_bytes().to_vec();
                    // `OBL-C250`: the stored commitment is read back and compared with the
                    // module's own content commitment. Before the fix it hashed the length,
                    // so any equal-length module matched; this reader is what makes the
                    // stored value meaningful.
                    let mut info_key = dwow_deployooor_contract::DEPLOY_CONTRACT_WASM_HASH_KEY.to_vec();
                    info_key.extend_from_slice(&deployed_cid.to_bytes());
                    let expected = dwow_deployooor_contract::model::wasm_content_commitment(
                        include_bytes!("../../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm"),
                    );
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contracts_tree(&k)?;
                        if r.is_none() { return Err(dwow_core::Error::Custom("WARN [deployooor::DeployV1]: deployed WASM must exist in contracts tree".into())); }
                        let stored = chain.query_contract_state(*DEPLOYOOOR_CONTRACT_ID, "info", &info_key)?
                            .ok_or_else(|| dwow_core::Error::Custom("deployooor: no stored WASM commitment".into()))?;
                        if stored != expected.to_repr() {
                            return Err(dwow_core::Error::Custom(
                                "deployooor: stored WASM commitment does not match the module".into(),
                            ));
                        }
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
                    Ok(EndpointResult { children: vec![], call_data: cd, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "LockV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(|| {
                    let secret = SecretKey::from_bytes([9u8; 32]).unwrap();
                    let public = PublicKey::from_secret(secret.clone());
                    let kp = Keypair { secret, public };
                    let tx_nonce = pallas::Base::from(0u64);
                    let plan = h.build_lock_call(kp, tx_nonce)?;
                    let mut cd = vec![0x01];
                    cd.extend_from_slice(&plan.params().encode());
                    // The `LockV2` proof binds to the frame the call lands in (`OBL-C198`). With no
                    // children, the frame is the commitment over this one call.
                    let call = dwow_sdk::tx::ContractCall {
                        contract_id: *DEPLOYOOOR_CONTRACT_ID,
                        data: cd.clone(),
                    };
                    let frame = dwow_sdk::crypto::util::tx_commitment([&call]);
                    let r = plan.prove(frame)?;
                    // `cd`, not `r.params.encode()`: the call data carries the `0x01` selector byte
                    // that `get_metadata` reads to pick the handler, and it is the same byte string
                    // the frame above was proven over. Returning the bare params dropped the
                    // selector, so `DeployFunction::try_from` read the first byte of the encoded
                    // public key and errored with `InvalidFunction`.
                    Ok(EndpointResult { children: vec![], call_data: cd, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
