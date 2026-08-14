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

use dwow_betting_stake_contract::model as betting_stake_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`betting_stake_model::UpdateRiskParamsV1`] python binding.
#[pyclass]
pub struct BettingStakeUpdateRiskParamsV1(betting_stake_model::UpdateRiskParamsV1);
impl_py_methods!(BettingStakeUpdateRiskParamsV1);

impl FunctionParams for betting_stake_model::UpdateRiskParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("table_id", format!("{:?}", self.table_id))?;
        dict.set_item("payout_amount", self.payout_amount)?;
        dict.set_item("house_share", self.house_share)?;
        dict.set_item("betting_contract_id", format!("{:?}", self.betting_contract_id))?;
        dict.set_item("nonce", format!("{:?}", self.nonce))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}table_id: {:?}", self.table_id).unwrap();
        writeln!(out, "{prefix}payout_amount: {}", self.payout_amount).unwrap();
        writeln!(out, "{prefix}house_share: {}", self.house_share).unwrap();
        writeln!(out, "{prefix}betting_contract_id: {:?}", self.betting_contract_id).unwrap();
        writeln!(out, "{prefix}nonce: {:?}", self.nonce).unwrap();
        Ok(())
    }
}
