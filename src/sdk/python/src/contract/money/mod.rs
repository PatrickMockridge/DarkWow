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

use std::fmt::Write;

use dwow_money_v3_contract::{model as money_model, MoneyV3Function};
use dwow_serial::deserialize;
use pyo3::{
    prelude::{PyAnyMethods, PyDictMethods, PyModule, PyModuleMethods},
    pyclass,
    types::PyDict,
    Bound, Py, PyResult, Python,
};

use crate::crypto::AeadEncryptedNote;

use super::{impl_py_methods, FunctionParams};

/// [`MoneyV3Function::AuthTokenMintV1`] function call parameter's python bindings.
pub mod auth_token_mint_v1;
pub use auth_token_mint_v1::MoneyV3AuthTokenMintParamsV1;

/// [`MoneyV3Function::TokenMintV1`] function call parameter's bindings.
pub mod token_mint_v1;
pub use token_mint_v1::MoneyV3TokenMintParamsV1;

/// [`MoneyV3Function::TransferV1`] function call parameter's bindings.
pub mod transfer_v1;
pub use transfer_v1::MoneyV3TransferParamsV1;

/// [`MoneyV3Function::BurnV1`] function call parameter's bindings.
pub mod burn_v1;
pub use burn_v1::MoneyV3BurnParamsV1;

/// [`MoneyV3Function::MintV1`] function call parameter's bindings.
pub mod mint_v1;
pub use mint_v1::MoneyV3MintParamsV1;

/// Decodes the parameters of a Money V3 contract function call.
pub fn decode_money_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match MoneyV3Function::try_from(function_index)? {
        MoneyV3Function::TokenMintV1 => {
            let params: money_model::TokenMintParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyV3Function::AuthTokenMintV1 => {
            let params: money_model::AuthTokenMintParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyV3Function::MintV1 => {
            let params: money_model::MintParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyV3Function::BurnV1 => {
            let params: money_model::BurnParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyV3Function::TransferV1 | MoneyV3Function::OtcSwapV1 => {
            let params: money_model::TransferParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// [`money_model::Input`] python binding
#[pyclass]
pub struct Input(money_model::Input);
impl_py_methods!(Input);

impl FunctionParams for money_model::Input {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("token_commit", format!("{:?}", self.token_commit))?;
        dict.set_item("nullifier", format!("{:?}", self.nullifier))?;
        dict.set_item("merkle_root", self.merkle_root.to_string())?;
        dict.set_item("user_data_enc", format!("{:?}", self.user_data_enc))?;
        dict.set_item("signature_public", format!("{:?}", self.signature_public))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}token_commit: {:?}", self.token_commit).unwrap();
        writeln!(out, "{prefix}nullifier: {:?}", self.nullifier).unwrap();
        writeln!(out, "{prefix}merkle_root: {}", self.merkle_root).unwrap();
        writeln!(out, "{prefix}user_data_enc: {:?}", self.user_data_enc).unwrap();
        writeln!(out, "{prefix}signature_public: {:?}", self.signature_public).unwrap();
        Ok(())
    }
}

/// [`money_model::Output`] python binding
#[pyclass]
pub struct Output(money_model::Output);
impl_py_methods!(Output);

impl FunctionParams for money_model::Output {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("token_commit", format!("{:?}", self.token_commit))?;
        dict.set_item("coin", format!("{:?}", self.coin))?;
        dict.set_item("note", self.note.to_pydict(py)?)?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}token_commit: {:?}", self.token_commit).unwrap();
        writeln!(out, "{prefix}coin: {:?}", self.coin).unwrap();
        writeln!(out, "{prefix}note:").unwrap();
        self.note.fmt_pretty(out, depth + 2)?;
        Ok(())
    }
}

/// Create money module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "money")?;

    submod.add_class::<MoneyV3AuthTokenMintParamsV1>()?;
    submod.add_class::<MoneyV3TokenMintParamsV1>()?;
    submod.add_class::<MoneyV3TransferParamsV1>()?;
    submod.add_class::<MoneyV3BurnParamsV1>()?;
    submod.add_class::<MoneyV3MintParamsV1>()?;
    submod.add_class::<Input>()?;
    submod.add_class::<Output>()?;
    submod.add_class::<AeadEncryptedNote>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.money", &submod)?;

    Ok(submod)
}
