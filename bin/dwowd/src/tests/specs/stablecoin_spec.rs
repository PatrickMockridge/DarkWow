use dwow_contract_test_harness::harness::{ContractHarness, StablecoinHarness};
use dwow_sdk::crypto::BaseBlind;
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn stablecoin_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(StablecoinHarness::spawn()));
    let h: &StablecoinHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/stablecoin/dwow_stablecoin_contract.wasm");
    let sk = pallas::Base::from(10u64);
    let cid = dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp");

    ContractTestSpec {
        name: "stablecoin",
        is_genesis: false,
        contract_id: cid,
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec {
                name: "OpenPositionV1",
                is_zk: true,
                generate: Box::new(move || {
                    let r = h.open_position(sk, 10000, 5000, pallas::Base::from(1u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
                generate_with_coinbase: None,
                verify_state: Some(Box::new(move |chain| {
                    let r = chain.query_contract_state(cid, "positions", &[])?;
                    if r.is_none() {
                        return Err(dwow_core::Error::Custom("stablecoin positions not found".into()));
                    }
                    Ok(())
                })),
                expectation: EndpointExpectation::Success,
            },
            EndpointSpec {
                name: "MintStableV1",
                is_zk: true,
                generate: Box::new(move || {
                    let r = h.mint_stable(sk, 10000, 5000, 1000,
                        BaseBlind::from_u64(100u64), BaseBlind::from_u64(200u64),
                        pallas::Base::from(1u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
                generate_with_coinbase: None,
                verify_state: Some(Box::new(move |chain| {
                    let r = chain.query_contract_state(cid, "stablecoin", &[])?;
                    if r.is_none() {
                        return Err(dwow_core::Error::Custom("stablecoin state not found".into()));
                    }
                    Ok(())
                })),
                expectation: EndpointExpectation::Success,
            },
            mk_ep("LiquidateV1", true, Box::new(move || {
                let r = h.liquidate(sk, 10000, 5000, 200, 1000, 500,
                    BaseBlind::from_u64(100u64), BaseBlind::from_u64(200u64),
                    pallas::Base::from(1u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("GovernanceReportV1", true, Box::new(move || {
                let r = h.governance_report(sk, 10000, 5000, 10, 3600, 42)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AccrueInterestV1", true, Box::new(move || {
                let r = h.accrue_interest(sk, 5000, 10, 3600)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AddCollateralV1", true, Box::new(move || {
                use dwow_stablecoin_contract::model::{DepositCollateralParams, CollateralType};
                use dwow_sdk::crypto::intent::IntentCommitment;
                let params = DepositCollateralParams {
                    deposit_commitment: IntentCommitment::from_base(pallas::Base::from(1u64)),
                    collateral_amount: 5000,
                    collateral_type: CollateralType::Xmr,
                    proof: vec![],
                    fee: 0,
                    zk_public_inputs: vec![pallas::Base::from(1u64), pallas::Base::from(2u64)],
                };
                let r = h.add_collateral(&params)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RemoveCollateralV1", true, Box::new(move || {
                use dwow_stablecoin_contract::model::WithdrawCollateralParams;
                use dwow_sdk::crypto::intent::{IntentCommitment, IntentNullifier};
                let params = WithdrawCollateralParams {
                    withdrawal_nullifier: IntentNullifier::from_base(pallas::Base::from(1u64)),
                    new_commitment: IntentCommitment::from_base(pallas::Base::from(2u64)),
                    withdraw_amount: 1000,
                    proof: vec![],
                    fee: 0,
                    zk_public_inputs: vec![pallas::Base::from(1u64)],
                };
                let r = h.remove_collateral(&params)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RepayStableV1", true, Box::new(move || {
                use dwow_stablecoin_contract::model::RepayStableParams;
                use dwow_sdk::crypto::intent::IntentCommitment;
                let params = RepayStableParams {
                    repay_commitment: IntentCommitment::from_base(pallas::Base::from(1u64)),
                    repay_amount: 500,
                    proof: vec![],
                    fee: 0,
                    zk_public_inputs: vec![pallas::Base::from(1u64)],
                };
                let r = h.repay_stable(&params)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("UpdateConfigV1", true, Box::new(move || {
                use dwow_stablecoin_contract::model::UpdateConfigParams;
                let params = UpdateConfigParams {
                    min_collateralization_ratio: 15000,
                    liquidation_threshold: 12000,
                    liquidation_penalty: 500,
                    base_rate: 500,
                    pi_kp: 0, pi_ki: 0,
                    twap_window: 3600,
                    price_deviation_threshold: 500,
                    gov_pub_x: pallas::Base::from(1u64),
                    gov_pub_y: pallas::Base::from(2u64),
                    config_nullifier: pallas::Base::from(3u64),
                };
                let r = h.update_config(&params)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
