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

use dwow_dex_contract::model as dex_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`dex_model::ExecuteSwapFeeParams`] python binding.
#[pyclass]
pub struct DexExecuteSwapFeeParamsV1(dex_model::ExecuteSwapFeeParams);
impl_py_methods!(DexExecuteSwapFeeParamsV1);

impl FunctionParams for dex_model::ExecuteSwapFeeParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("swap_id", format!("{:?}", self.swap_id))?;
        dict.set_item("alice_secret", format!("{:?}", self.alice_secret))?;
        dict.set_item("bob_secret", format!("{:?}", self.bob_secret))?;
        dict.set_item("alice_lock", format!("{:?}", self.alice_lock))?;
        dict.set_item("bob_lock", format!("{:?}", self.bob_lock))?;
        dict.set_item("alice_nullifier", format!("{:?}", self.alice_nullifier))?;
        dict.set_item("bob_nullifier", format!("{:?}", self.bob_nullifier))?;
        dict.set_item("fee_bps", format!("{:?}", self.fee_bps))?;
        dict.set_item("proof", format!("{:?}", self.proof))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        dict.set_item("tx_binding", format!("{:?}", self.tx_binding))?;
        dict.set_item("tx_nonce", format!("{:?}", self.tx_nonce))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}swap_id: {:?}", self.swap_id).unwrap();
        writeln!(out, "{prefix}alice_secret: {:?}", self.alice_secret).unwrap();
        writeln!(out, "{prefix}bob_secret: {:?}", self.bob_secret).unwrap();
        writeln!(out, "{prefix}alice_lock: {:?}", self.alice_lock).unwrap();
        writeln!(out, "{prefix}bob_lock: {:?}", self.bob_lock).unwrap();
        writeln!(out, "{prefix}alice_nullifier: {:?}", self.alice_nullifier).unwrap();
        writeln!(out, "{prefix}bob_nullifier: {:?}", self.bob_nullifier).unwrap();
        writeln!(out, "{prefix}fee_bps: {:?}", self.fee_bps).unwrap();
        writeln!(out, "{prefix}proof: {:?}", self.proof).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        writeln!(out, "{prefix}tx_binding: {:?}", self.tx_binding).unwrap();
        writeln!(out, "{prefix}tx_nonce: {:?}", self.tx_nonce).unwrap();
        Ok(())
    }
}
