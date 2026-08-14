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

/// [`betting_stake_model::ClaimEarningsParamsV1`] python binding.
#[pyclass]
pub struct BettingStakeClaimEarningsParamsV1(betting_stake_model::ClaimEarningsParamsV1);
impl_py_methods!(BettingStakeClaimEarningsParamsV1);

impl FunctionParams for betting_stake_model::ClaimEarningsParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("stake_id", format!("{:?}", self.stake_id))?;
        dict.set_item("table_id", format!("{:?}", self.table_id))?;
        dict.set_item("staker_pub", self.staker_pub.to_string())?;
        dict.set_item("current_amount", self.current_amount)?;
        dict.set_item("nonce", format!("{:?}", self.nonce))?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("staker_nullifier", format!("{:?}", self.staker_nullifier))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}stake_id: {:?}", self.stake_id).unwrap();
        writeln!(out, "{prefix}table_id: {:?}", self.table_id).unwrap();
        writeln!(out, "{prefix}staker_pub: {}", self.staker_pub).unwrap();
        writeln!(out, "{prefix}current_amount: {}", self.current_amount).unwrap();
        writeln!(out, "{prefix}nonce: {:?}", self.nonce).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}staker_nullifier: {:?}", self.staker_nullifier).unwrap();
        Ok(())
    }
}
