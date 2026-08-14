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

use dwow_pool_stake_contract::model as pool_stake_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`pool_stake_model::SlashCoverageParamsV1`] python binding.
#[pyclass]
pub struct PoolStakeSlashCoverageParamsV1(pool_stake_model::SlashCoverageParamsV1);
impl_py_methods!(PoolStakeSlashCoverageParamsV1);

impl FunctionParams for pool_stake_model::SlashCoverageParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("allocation_id", format!("{:?}", self.allocation_id))?;
        dict.set_item("owner_pub", self.owner_pub.to_string())?;
        dict.set_item("slash_amount", format!("{:?}", self.slash_amount))?;
        dict.set_item("user_pub", self.user_pub.to_string())?;
        dict.set_item("nonce", format!("{:?}", self.nonce))?;
        dict.set_item("derived_slash_id", format!("{:?}", self.derived_slash_id))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}allocation_id: {:?}", self.allocation_id).unwrap();
        writeln!(out, "{prefix}owner_pub: {}", self.owner_pub).unwrap();
        writeln!(out, "{prefix}slash_amount: {:?}", self.slash_amount).unwrap();
        writeln!(out, "{prefix}user_pub: {}", self.user_pub).unwrap();
        writeln!(out, "{prefix}nonce: {:?}", self.nonce).unwrap();
        writeln!(out, "{prefix}derived_slash_id: {:?}", self.derived_slash_id).unwrap();
        Ok(())
    }
}
