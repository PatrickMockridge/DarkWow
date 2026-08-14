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

use dwow_drain_protection_contract::model as drain_protection_model;
use dwow_sdk::hex::AsHex;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`drain_protection_model::ProposeParamsV1`] python binding.
#[pyclass]
pub struct DrainProtectionProposeParamsV1(drain_protection_model::ProposeParamsV1);
impl_py_methods!(DrainProtectionProposeParamsV1);

impl FunctionParams for drain_protection_model::ProposeParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("message_hash", format!("{:?}", self.message_hash))?;
        dict.set_item("multisig_group_id", format!("{:?}", self.multisig_group_id))?;
        dict.set_item("prover_pubkey", self.prover_pubkey.to_string())?;
        dict.set_item("vote_period_blocks", self.vote_period_blocks)?;
        dict.set_item("proof", self.proof.hex())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}message_hash: {:?}", self.message_hash).unwrap();
        writeln!(out, "{prefix}multisig_group_id: {:?}", self.multisig_group_id).unwrap();
        writeln!(out, "{prefix}prover_pubkey: {}", self.prover_pubkey).unwrap();
        writeln!(out, "{prefix}vote_period_blocks: {}", self.vote_period_blocks).unwrap();
        writeln!(out, "{prefix}proof: [{} bytes]", self.proof.len()).unwrap();
        Ok(())
    }
}
