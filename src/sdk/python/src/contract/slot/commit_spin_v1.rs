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

use dwow_slot_contract::model as slot_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`slot_model::CommitSpinParamsV1`] python binding.
#[pyclass]
pub struct SlotCommitSpinParamsV1(slot_model::CommitSpinParamsV1);
impl_py_methods!(SlotCommitSpinParamsV1);

impl FunctionParams for slot_model::CommitSpinParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("player_pub", format!("{:?}", self.player_pub))?;
        dict.set_item("bet_value", self.bet_value)?;
        dict.set_item("paylines_played", self.paylines_played)?;
        dict.set_item("secret_nonce", format!("{:?}", self.secret_nonce))?;
        dict.set_item("blind", format!("{:?}", self.blind))?;
        dict.set_item("house_edge", self.house_edge)?;
        dict.set_item("confirmation_depth", self.confirmation_depth)?;
        dict.set_item("token_id", format!("{:?}", self.token_id))?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}player_pub: {:?}", self.player_pub).unwrap();
        writeln!(out, "{prefix}bet_value: {}", self.bet_value).unwrap();
        writeln!(out, "{prefix}paylines_played: {}", self.paylines_played).unwrap();
        writeln!(out, "{prefix}secret_nonce: {:?}", self.secret_nonce).unwrap();
        writeln!(out, "{prefix}blind: {:?}", self.blind).unwrap();
        writeln!(out, "{prefix}house_edge: {}", self.house_edge).unwrap();
        writeln!(out, "{prefix}confirmation_depth: {}", self.confirmation_depth).unwrap();
        writeln!(out, "{prefix}token_id: {:?}", self.token_id).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        Ok(())
    }
}
