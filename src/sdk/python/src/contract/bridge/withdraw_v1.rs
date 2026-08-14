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

/// [`bridge_model::WithdrawParams`] python binding.
#[pyclass]
pub struct BridgeWithdrawParamsV1(bridge_model::WithdrawParams);
impl_py_methods!(BridgeWithdrawParamsV1);

impl FunctionParams for bridge_model::WithdrawParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("nullifier", format!("{:?}", self.nullifier))?;
        dict.set_item("recipient_hash", format!("{:?}", self.recipient_hash))?;
        dict.set_item("deposit_leaf", format!("{:?}", self.deposit_leaf))?;
        dict.set_item("amount", format!("{:?}", self.amount))?;
        dict.set_item("proof", format!("{:?}", self.proof))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        dict.set_item("timeout_height", format!("{:?}", self.timeout_height))?;
        dict.set_item("feed_mode", format!("{:?}", self.feed_mode))?;
        dict.set_item("max_fee_bp", format!("{:?}", self.max_fee_bp))?;
        dict.set_item("expected_root", format!("{:?}", self.expected_root))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}nullifier: {:?}", self.nullifier).unwrap();
        writeln!(out, "{prefix}recipient_hash: {:?}", self.recipient_hash).unwrap();
        writeln!(out, "{prefix}deposit_leaf: {:?}", self.deposit_leaf).unwrap();
        writeln!(out, "{prefix}amount: {:?}", self.amount).unwrap();
        writeln!(out, "{prefix}proof: {:?}", self.proof).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        writeln!(out, "{prefix}timeout_height: {:?}", self.timeout_height).unwrap();
        writeln!(out, "{prefix}feed_mode: {:?}", self.feed_mode).unwrap();
        writeln!(out, "{prefix}max_fee_bp: {:?}", self.max_fee_bp).unwrap();
        writeln!(out, "{prefix}expected_root: {:?}", self.expected_root).unwrap();
        Ok(())
    }
}
