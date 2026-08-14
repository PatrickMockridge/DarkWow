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

/// [`bridge_model::DepositParams`] python binding.
#[pyclass]
pub struct BridgeDepositParamsV1(bridge_model::DepositParams);
impl_py_methods!(BridgeDepositParamsV1);

impl FunctionParams for bridge_model::DepositParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("commitment", format!("{:?}", self.commitment))?;
        dict.set_item("recipient_pub", format!("{:?}", self.recipient_pub))?;
        dict.set_item("bridge_nonce", format!("{:?}", self.bridge_nonce))?;
        dict.set_item("chain", format!("{:?}", self.chain))?;
        dict.set_item("external_block_hash", format!("{:?}", self.external_block_hash))?;
        dict.set_item("merkle_proof", format!("{:?}", self.merkle_proof))?;
        dict.set_item("external_state_root", format!("{:?}", self.external_state_root))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        dict.set_item("proof", format!("{:?}", self.proof))?;
        dict.set_item("chain_proof", format!("{:?}", self.chain_proof))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}commitment: {:?}", self.commitment).unwrap();
        writeln!(out, "{prefix}recipient_pub: {:?}", self.recipient_pub).unwrap();
        writeln!(out, "{prefix}bridge_nonce: {:?}", self.bridge_nonce).unwrap();
        writeln!(out, "{prefix}chain: {:?}", self.chain).unwrap();
        writeln!(out, "{prefix}external_block_hash: {:?}", self.external_block_hash).unwrap();
        writeln!(out, "{prefix}merkle_proof: {:?}", self.merkle_proof).unwrap();
        writeln!(out, "{prefix}external_state_root: {:?}", self.external_state_root).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        writeln!(out, "{prefix}proof: {:?}", self.proof).unwrap();
        writeln!(out, "{prefix}chain_proof: {:?}", self.chain_proof).unwrap();
        Ok(())
    }
}
