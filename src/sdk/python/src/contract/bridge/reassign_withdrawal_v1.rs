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

/// [`bridge_model::ReassignWithdrawalParamsV1`] python binding.
#[pyclass]
pub struct BridgeReassignWithdrawalParamsV1(bridge_model::ReassignWithdrawalParamsV1);
impl_py_methods!(BridgeReassignWithdrawalParamsV1);

impl FunctionParams for bridge_model::ReassignWithdrawalParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("nullifier", format!("{:?}", self.nullifier))?;
        dict.set_item("new_relayer", format!("{:?}", self.new_relayer))?;
        dict.set_item("current_block", format!("{:?}", self.current_block))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}nullifier: {:?}", self.nullifier).unwrap();
        writeln!(out, "{prefix}new_relayer: {:?}", self.new_relayer).unwrap();
        writeln!(out, "{prefix}current_block: {:?}", self.current_block).unwrap();
        Ok(())
    }
}
