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

use dwow_promissory_note_contract::{model as promissory_model, PromissoryNoteFunction};
use dwow_serial::deserialize;
use pyo3::{
    prelude::{PyAnyMethods, PyDictMethods, PyModule, PyModuleMethods},
    pyclass,
    types::PyDict,
    Bound, Py, PyResult, Python,
};

use crate::crypto::AeadEncryptedNote;

use super::{impl_py_methods, FunctionParams};

/// [`PromissoryNoteFunction::RegisterTypeV1`] function call parameter's bindings.
pub mod token_mint_v1;
pub use token_mint_v1::PromissoryNoteTokenMintParamsV1;

/// [`PromissoryNoteFunction::TransferV1`] function call parameter's bindings.
pub mod transfer_v1;
pub use transfer_v1::PromissoryNoteTransferParamsV1;

/// [`PromissoryNoteFunction::RevokeV1`] function call parameter's bindings.
pub mod burn_v1;
pub use burn_v1::PromissoryNoteBurnParamsV1;

/// [`PromissoryNoteFunction::IssueV1`] function call parameter's bindings.
pub mod mint_v1;
pub use mint_v1::PromissoryNoteMintParamsV1;

/// [`PromissoryNoteFunction::RedeemV1`] function call parameter's bindings.
pub mod redeem_v1;

/// Decodes the parameters of a Promissory Note contract function call.
pub fn decode_promissory_function_params(
    function_index: u8,
    data: &[u8],
) -> dwow_core::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match PromissoryNoteFunction::try_from(function_index)? {
        PromissoryNoteFunction::RegisterTypeV1 => {
            let params: promissory_model::TokenMintParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        PromissoryNoteFunction::IssueV1 => {
            let params: promissory_model::MintParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        PromissoryNoteFunction::RevokeV1 => {
            let params: promissory_model::BurnParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        PromissoryNoteFunction::TransferV1 | PromissoryNoteFunction::OtcSwapV1 => {
            let params: promissory_model::TransferParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        PromissoryNoteFunction::RedeemV1 => {
            let params: promissory_model::RedeemParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// [`promissory_model::Input`] python binding
#[pyclass]
pub struct Input(promissory_model::Input);
impl_py_methods!(Input);

impl FunctionParams for promissory_model::Input {
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

/// [`promissory_model::Output`] python binding
#[pyclass]
pub struct Output(promissory_model::Output);
impl_py_methods!(Output);

impl FunctionParams for promissory_model::Output {
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

/// Create promissory module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "promissory")?;

    submod.add_class::<PromissoryNoteTokenMintParamsV1>()?;
    submod.add_class::<PromissoryNoteTransferParamsV1>()?;
    submod.add_class::<PromissoryNoteBurnParamsV1>()?;
    submod.add_class::<PromissoryNoteMintParamsV1>()?;
    submod.add_class::<Input>()?;
    submod.add_class::<Output>()?;
    submod.add_class::<AeadEncryptedNote>()?;

    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract.promissory", &submod)?;

    Ok(submod)
}
