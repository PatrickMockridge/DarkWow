/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use dwow_insurance_market_contract::{model as insurance_market_model, InsuranceMarketFunction};
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`InsuranceMarketFunction::RegisterRiskTypeV1`] function call parameter's python bindings.
pub mod register_risk_type_v1;
pub use register_risk_type_v1::InsuranceMarketRegisterRiskTypeParamsV1;

/// [`InsuranceMarketFunction::CreateMarketV1`] function call parameter's python bindings.
pub mod create_market_v1;
pub use create_market_v1::InsuranceMarketCreateMarketParamsV1;

/// [`InsuranceMarketFunction::UnderwriteV1`] function call parameter's python bindings.
pub mod underwrite_v1;
pub use underwrite_v1::InsuranceMarketUnderwriteParamsV1;

/// [`InsuranceMarketFunction::PurchaseCoverageV1`] function call parameter's python bindings.
pub mod purchase_coverage_v1;
pub use purchase_coverage_v1::InsuranceMarketPurchaseCoverageParamsV1;

/// [`InsuranceMarketFunction::FileClaimV1`] function call parameter's python bindings.
pub mod file_claim_v1;
pub use file_claim_v1::InsuranceMarketFileClaimParamsV1;

/// [`InsuranceMarketFunction::ResolveClaimV1`] function call parameter's python bindings.
pub mod resolve_claim_v1;
pub use resolve_claim_v1::InsuranceMarketResolveClaimParamsV1;

/// [`InsuranceMarketFunction::WithdrawPremiumV1`] function call parameter's python bindings.
pub mod withdraw_premium_v1;
pub use withdraw_premium_v1::InsuranceMarketWithdrawPremiumParamsV1;

/// [`InsuranceMarketFunction::UpdatePremiumV1`] function call parameter's python bindings.
pub mod update_premium_v1;
pub use update_premium_v1::InsuranceMarketUpdatePremiumParamsV1;

/// [`InsuranceMarketFunction::UnderwriteWithCapabilityV1`] function call parameter's python bindings.
pub mod underwrite_with_capability_v1;
pub use underwrite_with_capability_v1::InsuranceMarketUnderwriteWithCapabilityParamsV1;

/// [`InsuranceMarketFunction::PurchaseCoverageWithCapabilityV1`] function call parameter's python bindings.
pub mod purchase_coverage_with_capability_v1;
pub use purchase_coverage_with_capability_v1::InsuranceMarketPurchaseCoverageWithCapabilityParamsV1;

/// [`InsuranceMarketFunction::PurchaseCoverageWithDAGV1`] function call parameter's python bindings.
pub mod purchase_coverage_with_dag_v1;
pub use purchase_coverage_with_dag_v1::InsuranceMarketPurchaseCoverageWithDAGParamsV1;

/// [`InsuranceMarketFunction::ResolveClaimWithCapabilityV1`] function call parameter's python bindings.
pub mod resolve_claim_with_capability_v1;
pub use resolve_claim_with_capability_v1::InsuranceMarketResolveClaimWithCapabilityParamsV1;

/// [`InsuranceMarketFunction::DeactivateUnderwriterV1`] function call parameter's python bindings.
pub mod deactivate_underwriter_v1;
pub use deactivate_underwriter_v1::InsuranceMarketDeactivateUnderwriterParamsV1;

/// [`InsuranceMarketFunction::CloseMarketV1`] function call parameter's python bindings.
pub mod close_market_v1;
pub use close_market_v1::InsuranceMarketCloseMarketParamsV1;

/// [`InsuranceMarketFunction::RetireRiskTypeV1`] function call parameter's python bindings.
pub mod retire_risk_type_v1;
pub use retire_risk_type_v1::InsuranceMarketRetireRiskTypeParamsV1;

/// Decodes the parameters of an Insurance-Market contract function call.
pub fn decode_insurance_market_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match InsuranceMarketFunction::try_from(function_index)? {
        InsuranceMarketFunction::RegisterRiskTypeV1 => {
            let params = insurance_market_model::RegisterRiskTypeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::CreateMarketV1 => {
            let params = insurance_market_model::CreateMarketParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::UnderwriteV1 => {
            let params = insurance_market_model::UnderwriteParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::PurchaseCoverageV1 => {
            let params = insurance_market_model::PurchaseCoverageParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::FileClaimV1 => {
            let params = insurance_market_model::FileClaimParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::ResolveClaimV1 => {
            let params = insurance_market_model::ResolveClaimParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::WithdrawPremiumV1 => {
            let params = insurance_market_model::WithdrawPremiumParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::UpdatePremiumV1 => {
            let params = insurance_market_model::UpdatePremiumParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::UnderwriteWithCapabilityV1 => {
            let params = insurance_market_model::UnderwriteWithCapabilityParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::PurchaseCoverageWithCapabilityV1 => {
            let params = insurance_market_model::PurchaseCoverageWithCapabilityParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::PurchaseCoverageWithDAGV1 => {
            let params = insurance_market_model::PurchaseCoverageWithDAGParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::ResolveClaimWithCapabilityV1 => {
            let params = insurance_market_model::ResolveClaimWithCapabilityParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::DeactivateUnderwriterV1 => {
            let params = insurance_market_model::DeactivateUnderwriterParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::CloseMarketV1 => {
            let params = insurance_market_model::CloseMarketParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        InsuranceMarketFunction::RetireRiskTypeV1 => {
            let params = insurance_market_model::RetireRiskTypeParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        _ => return Err(dwow_core::Error::ParseFailed("unsupported Insurance-Market function")),
    };

    Ok(res)
}

/// Create insurance_market module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "insurance_market")?;

    submod.add_class::<InsuranceMarketRegisterRiskTypeParamsV1>()?;
    submod.add_class::<InsuranceMarketCreateMarketParamsV1>()?;
    submod.add_class::<InsuranceMarketUnderwriteParamsV1>()?;
    submod.add_class::<InsuranceMarketPurchaseCoverageParamsV1>()?;
    submod.add_class::<InsuranceMarketFileClaimParamsV1>()?;
    submod.add_class::<InsuranceMarketResolveClaimParamsV1>()?;
    submod.add_class::<InsuranceMarketWithdrawPremiumParamsV1>()?;
    submod.add_class::<InsuranceMarketUpdatePremiumParamsV1>()?;
    submod.add_class::<InsuranceMarketUnderwriteWithCapabilityParamsV1>()?;
    submod.add_class::<InsuranceMarketPurchaseCoverageWithCapabilityParamsV1>()?;
    submod.add_class::<InsuranceMarketPurchaseCoverageWithDAGParamsV1>()?;
    submod.add_class::<InsuranceMarketResolveClaimWithCapabilityParamsV1>()?;
    submod.add_class::<InsuranceMarketDeactivateUnderwriterParamsV1>()?;
    submod.add_class::<InsuranceMarketCloseMarketParamsV1>()?;
    submod.add_class::<InsuranceMarketRetireRiskTypeParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.insurance_market", &submod)?;

    Ok(submod)
}
