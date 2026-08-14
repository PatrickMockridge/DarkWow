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

/// [`pool_stake_model::UpdatePoolConfigParamsV1`] python binding.
#[pyclass]
pub struct PoolStakeUpdatePoolConfigParamsV1(pool_stake_model::UpdatePoolConfigParamsV1);
impl_py_methods!(PoolStakeUpdatePoolConfigParamsV1);

impl FunctionParams for pool_stake_model::UpdatePoolConfigParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("pool_id", format!("{:?}", self.pool_id))?;
        dict.set_item("owner_pub", self.owner_pub.to_string())?;
        dict.set_item("max_coverage_ratio", format!("{:?}", self.max_coverage_ratio))?;
        dict.set_item("operator_fee_bp", format!("{:?}", self.operator_fee_bp))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}pool_id: {:?}", self.pool_id).unwrap();
        writeln!(out, "{prefix}owner_pub: {}", self.owner_pub).unwrap();
        writeln!(out, "{prefix}max_coverage_ratio: {:?}", self.max_coverage_ratio).unwrap();
        writeln!(out, "{prefix}operator_fee_bp: {:?}", self.operator_fee_bp).unwrap();
        Ok(())
    }
}
