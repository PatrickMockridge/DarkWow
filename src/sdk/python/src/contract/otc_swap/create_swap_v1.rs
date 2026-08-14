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

/// [`otc_swap_model::CreateSwapParamsV1`] python binding.
#[pyclass]
pub struct OtcSwapCreateSwapParamsV1(otc_swap_model::CreateSwapParamsV1);
impl_py_methods!(OtcSwapCreateSwapParamsV1);

impl FunctionParams for otc_swap_model::CreateSwapParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("alice_pubkey", format!("{:?}", self.alice_pubkey))?;
        dict.set_item("bob_pubkey", format!("{:?}", self.bob_pubkey))?;
        dict.set_item("send_value", format!("{:?}", self.send_value))?;
        dict.set_item("send_token_id", format!("{:?}", self.send_token_id))?;
        dict.set_item("recv_value", format!("{:?}", self.recv_value))?;
        dict.set_item("recv_token_id", format!("{:?}", self.recv_token_id))?;
        dict.set_item("timeout", format!("{:?}", self.timeout))?;
        dict.set_item("commitment", format!("{:?}", self.commitment))?;
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}alice_pubkey: {:?}", self.alice_pubkey).unwrap();
        writeln!(out, "{prefix}bob_pubkey: {:?}", self.bob_pubkey).unwrap();
        writeln!(out, "{prefix}send_value: {:?}", self.send_value).unwrap();
        writeln!(out, "{prefix}send_token_id: {:?}", self.send_token_id).unwrap();
        writeln!(out, "{prefix}recv_value: {:?}", self.recv_value).unwrap();
        writeln!(out, "{prefix}recv_token_id: {:?}", self.recv_token_id).unwrap();
        writeln!(out, "{prefix}timeout: {:?}", self.timeout).unwrap();
        writeln!(out, "{prefix}commitment: {:?}", self.commitment).unwrap();
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        Ok(())
    }
}
