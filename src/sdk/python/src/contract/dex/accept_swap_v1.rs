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

use dwow_dex_contract::model as dex_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`dex_model::AcceptSwapParams`] python binding.
#[pyclass]
pub struct DexAcceptSwapParamsV1(dex_model::AcceptSwapParams);
impl_py_methods!(DexAcceptSwapParamsV1);

impl FunctionParams for dex_model::AcceptSwapParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("swap_id", format!("{:?}", self.swap_id))?;
        dict.set_item("lock_commitment", format!("{:?}", self.lock_commitment))?;
        dict.set_item("nullifier", format!("{:?}", self.nullifier))?;
        dict.set_item("signature_public", format!("{:?}", self.signature_public))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        dict.set_item("immediate_execute", format!("{:?}", self.immediate_execute))?;
        dict.set_item("tx_binding", format!("{:?}", self.tx_binding))?;
        dict.set_item("tx_nonce", format!("{:?}", self.tx_nonce))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}swap_id: {:?}", self.swap_id).unwrap();
        writeln!(out, "{prefix}lock_commitment: {:?}", self.lock_commitment).unwrap();
        writeln!(out, "{prefix}nullifier: {:?}", self.nullifier).unwrap();
        writeln!(out, "{prefix}signature_public: {:?}", self.signature_public).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        writeln!(out, "{prefix}immediate_execute: {:?}", self.immediate_execute).unwrap();
        writeln!(out, "{prefix}tx_binding: {:?}", self.tx_binding).unwrap();
        writeln!(out, "{prefix}tx_nonce: {:?}", self.tx_nonce).unwrap();
        Ok(())
    }
}
