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

use dwow_stablecoin_contract::model as stablecoin_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`stablecoin_model::WithdrawCollateralParams`] python binding.
#[pyclass]
pub struct StablecoinRemoveCollateralParamsV1(stablecoin_model::WithdrawCollateralParams);
impl_py_methods!(StablecoinRemoveCollateralParamsV1);

impl FunctionParams for stablecoin_model::WithdrawCollateralParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("withdrawal_nullifier", format!("{:?}", self.withdrawal_nullifier))?;
        dict.set_item("new_commitment", format!("{:?}", self.new_commitment))?;
        dict.set_item("withdraw_amount", format!("{:?}", self.withdraw_amount))?;
        dict.set_item("proof", format!("{:?}", self.proof))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        dict.set_item("zk_public_inputs", format!("{:?}", self.zk_public_inputs))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}withdrawal_nullifier: {:?}", self.withdrawal_nullifier).unwrap();
        writeln!(out, "{prefix}new_commitment: {:?}", self.new_commitment).unwrap();
        writeln!(out, "{prefix}withdraw_amount: {:?}", self.withdraw_amount).unwrap();
        writeln!(out, "{prefix}proof: {:?}", self.proof).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        writeln!(out, "{prefix}zk_public_inputs: {:?}", self.zk_public_inputs).unwrap();
        Ok(())
    }
}
