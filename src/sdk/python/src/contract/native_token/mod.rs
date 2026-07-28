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

use dwow_native_token_contract::{model as native_token_model, NativeTokenFunction};
use dwow_serial::deserialize;
use pyo3::{
    prelude::{PyAnyMethods, PyDictMethods, PyModule, PyModuleMethods},
    pyclass,
    types::PyDict,
    Bound, Py, PyResult, Python,
};

use crate::crypto::AeadEncryptedNote;

use super::{impl_py_methods, FunctionParams};

/// [`NativeTokenFunction::FeeV1`] function call parameter's python bindings.
pub mod fee_v1;
pub use fee_v1::FeeParamsV1;

/// [`NativeTokenFunction::PoWRewardV1`] function call parameter's python bindings.
pub mod pow_reward_v1;
pub use pow_reward_v1::PoWRewardParamsV1;

/// Decodes the parameters of a NativeToken contract function call.
pub fn decode_native_token_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match NativeTokenFunction::try_from(function_index)? {
        NativeTokenFunction::FeeV1 => {
            let params = native_token_model::FeeParamsV1::decode(&data[9..])?;
            Box::new(params)
        }
        NativeTokenFunction::PoWRewardV1 => {
            let params = native_token_model::PoWRewardParamsV1::decode(&data[1..])?;
            Box::new(params)
        }
        _ => return Err(dwow_core::Error::ParseFailed("unsupported NativeToken function")),
    };

    Ok(res)
}

/// [`native_token_model::Input`] python binding
#[pyclass]
pub struct Input(native_token_model::Input);
impl_py_methods!(Input);

impl FunctionParams for native_token_model::Input {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("token_commit", format!("{:?}", self.token_commit))?;
        dict.set_item("nullifier", format!("{:?}", self.nullifier))?;
        dict.set_item("merkle_root", format!("{:?}", self.merkle_root))?;
        dict.set_item("user_data_enc", format!("{:?}", self.user_data_enc))?;
        dict.set_item("signature_public", self.signature_public.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}token_commit: {:?}", self.token_commit).unwrap();
        writeln!(out, "{prefix}nullifier: {:?}", self.nullifier).unwrap();
        writeln!(out, "{prefix}merkle_root: {:?}", self.merkle_root).unwrap();
        writeln!(out, "{prefix}user_data_enc: {:?}", self.user_data_enc).unwrap();
        writeln!(out, "{prefix}signature_public: {}", self.signature_public).unwrap();
        Ok(())
    }
}

/// [`native_token_model::Output`] python binding
#[pyclass]
pub struct Output(native_token_model::Output);
impl_py_methods!(Output);

impl FunctionParams for native_token_model::Output {
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

/// [`native_token_model::ClearInput`] python binding
#[pyclass]
pub struct ClearInput(native_token_model::ClearInput);
impl_py_methods!(ClearInput);

impl FunctionParams for native_token_model::ClearInput {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("value", self.value)?;
        dict.set_item("token_id", format!("{:?}", self.token_id))?;
        dict.set_item("value_blind", self.value_blind.to_string())?;
        dict.set_item("token_blind", format!("{:?}", self.token_blind))?;
        dict.set_item("signature_public", self.signature_public.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}value: {}", self.value).unwrap();
        writeln!(out, "{prefix}token_id: {:?}", self.token_id).unwrap();
        writeln!(out, "{prefix}value_blind: {}", self.value_blind).unwrap();
        writeln!(out, "{prefix}token_blind: {:?}", self.token_blind).unwrap();
        writeln!(out, "{prefix}signature_public: {}", self.signature_public).unwrap();
        Ok(())
    }
}

/// Create native_token module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "native_token")?;

    submod.add_class::<FeeParamsV1>()?;
    submod.add_class::<PoWRewardParamsV1>()?;
    submod.add_class::<Input>()?;
    submod.add_class::<Output>()?;
    submod.add_class::<ClearInput>()?;
    submod.add_class::<AeadEncryptedNote>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.native_token", &submod)?;

    Ok(submod)
}
