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

use dwow_drain_protection_contract::model as drain_protection_model;
use dwow_sdk::hex::AsHex;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`drain_protection_model::InitializeParamsV1`] python binding.
#[pyclass]
pub struct DrainProtectionInitializeParamsV1(drain_protection_model::InitializeParamsV1);
impl_py_methods!(DrainProtectionInitializeParamsV1);

impl FunctionParams for drain_protection_model::InitializeParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("instance_seed", self.instance_seed.hex())?;
        dict.set_item("fund_id", format!("{:?}", self.fund_id))?;
        dict.set_item("spend_authority", self.spend_authority.to_string())?;
        dict.set_item("dao_escrow_bulla", format!("{:?}", self.dao_escrow_bulla))?;
        dict.set_item("drain_config", format!("{:?}", self.drain_config))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}instance_seed: [{} bytes]", self.instance_seed.len()).unwrap();
        writeln!(out, "{prefix}fund_id: {:?}", self.fund_id).unwrap();
        writeln!(out, "{prefix}spend_authority: {}", self.spend_authority).unwrap();
        writeln!(out, "{prefix}dao_escrow_bulla: {:?}", self.dao_escrow_bulla).unwrap();
        writeln!(out, "{prefix}drain_config: {:?}", self.drain_config).unwrap();
        Ok(())
    }
}
