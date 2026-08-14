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

use dwow_escrow_contract::model as escrow_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`escrow_model::RefundEscrowParamsV1`] python binding.
#[pyclass]
pub struct EscrowRefundParamsV1(escrow_model::RefundEscrowParamsV1);
impl_py_methods!(EscrowRefundParamsV1);

impl FunctionParams for escrow_model::RefundEscrowParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("escrow_id", format!("{:?}", self.escrow_id))?;
        dict.set_item("buyer_secret", format!("{:?}", self.buyer_secret))?;
        dict.set_item("spent_nullifier", format!("{:?}", self.spent_nullifier))?;
        dict.set_item("current_block", format!("{:?}", self.current_block))?;
        dict.set_item("timeout", format!("{:?}", self.timeout))?;
        dict.set_item("recipient_pubkey", format!("{:?}", self.recipient_pubkey))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}escrow_id: {:?}", self.escrow_id).unwrap();
        writeln!(out, "{prefix}buyer_secret: {:?}", self.buyer_secret).unwrap();
        writeln!(out, "{prefix}spent_nullifier: {:?}", self.spent_nullifier).unwrap();
        writeln!(out, "{prefix}current_block: {:?}", self.current_block).unwrap();
        writeln!(out, "{prefix}timeout: {:?}", self.timeout).unwrap();
        writeln!(out, "{prefix}recipient_pubkey: {:?}", self.recipient_pubkey).unwrap();
        Ok(())
    }
}
