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

use dwow_otc_swap_contract::model as otc_swap_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`otc_swap_model::FundSwapParamsV1`] python binding.
#[pyclass]
pub struct OtcSwapFundSwapParamsV1(otc_swap_model::FundSwapParamsV1);
impl_py_methods!(OtcSwapFundSwapParamsV1);

impl FunctionParams for otc_swap_model::FundSwapParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("swap_id", format!("{:?}", self.swap_id))?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("merkle_proof", format!("{:?}", self.merkle_proof))?;
        dict.set_item("merkle_root", format!("{:?}", self.merkle_root))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}swap_id: {:?}", self.swap_id).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}merkle_proof: {:?}", self.merkle_proof).unwrap();
        writeln!(out, "{prefix}merkle_root: {:?}", self.merkle_root).unwrap();
        Ok(())
    }
}
