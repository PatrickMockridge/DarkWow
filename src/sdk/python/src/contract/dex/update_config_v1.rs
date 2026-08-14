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

/// [`dex_model::UpdateConfigParams`] python binding.
#[pyclass]
pub struct DexUpdateConfigParamsV1(dex_model::UpdateConfigParams);
impl_py_methods!(DexUpdateConfigParamsV1);

impl FunctionParams for dex_model::UpdateConfigParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("timeout", format!("{:?}", self.timeout))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        dict.set_item("gov_pub_x", format!("{:?}", self.gov_pub_x))?;
        dict.set_item("gov_pub_y", format!("{:?}", self.gov_pub_y))?;
        dict.set_item("gov_nullifier", format!("{:?}", self.gov_nullifier))?;
        dict.set_item("tx_binding", format!("{:?}", self.tx_binding))?;
        dict.set_item("tx_nonce", format!("{:?}", self.tx_nonce))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}timeout: {:?}", self.timeout).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        writeln!(out, "{prefix}gov_pub_x: {:?}", self.gov_pub_x).unwrap();
        writeln!(out, "{prefix}gov_pub_y: {:?}", self.gov_pub_y).unwrap();
        writeln!(out, "{prefix}gov_nullifier: {:?}", self.gov_nullifier).unwrap();
        writeln!(out, "{prefix}tx_binding: {:?}", self.tx_binding).unwrap();
        writeln!(out, "{prefix}tx_nonce: {:?}", self.tx_nonce).unwrap();
        Ok(())
    }
}
