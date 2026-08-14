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

use dwow_bridge_contract::model as bridge_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`bridge_model::CreateHtlcParams`] python binding.
#[pyclass]
pub struct BridgeCreateHtlcParamsV1(bridge_model::CreateHtlcParams);
impl_py_methods!(BridgeCreateHtlcParamsV1);

impl FunctionParams for bridge_model::CreateHtlcParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("swap_id", format!("{:?}", self.swap_id))?;
        dict.set_item("hash", format!("{:?}", self.hash))?;
        dict.set_item("timelock", format!("{:?}", self.timelock))?;
        dict.set_item("amount", format!("{:?}", self.amount))?;
        dict.set_item("external_recipient", format!("{:?}", self.external_recipient))?;
        dict.set_item("chain", format!("{:?}", self.chain))?;
        dict.set_item("deposit_proof", format!("{:?}", self.deposit_proof))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}swap_id: {:?}", self.swap_id).unwrap();
        writeln!(out, "{prefix}hash: {:?}", self.hash).unwrap();
        writeln!(out, "{prefix}timelock: {:?}", self.timelock).unwrap();
        writeln!(out, "{prefix}amount: {:?}", self.amount).unwrap();
        writeln!(out, "{prefix}external_recipient: {:?}", self.external_recipient).unwrap();
        writeln!(out, "{prefix}chain: {:?}", self.chain).unwrap();
        writeln!(out, "{prefix}deposit_proof: {:?}", self.deposit_proof).unwrap();
        Ok(())
    }
}
