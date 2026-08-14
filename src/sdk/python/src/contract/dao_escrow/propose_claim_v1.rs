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

use dwow_dao_escrow_contract::model as dao_escrow_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`dao_escrow_model::ProposeClaimParamsV1`] python binding.
#[pyclass]
pub struct DaoEscrowProposeClaimParamsV1(dao_escrow_model::ProposeClaimParamsV1);
impl_py_methods!(DaoEscrowProposeClaimParamsV1);

impl FunctionParams for dao_escrow_model::ProposeClaimParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("dao_escrow_bulla", format!("{:?}", self.dao_escrow_bulla))?;
        dict.set_item("claim_id", format!("{:?}", self.claim_id))?;
        dict.set_item("value", format!("{:?}", self.value))?;
        dict.set_item("description_hash", format!("{:?}", self.description_hash))?;
        dict.set_item("recipient_pubkey", self.recipient_pubkey.to_string())?;
        dict.set_item("proposer_pubkey", self.proposer_pubkey.to_string())?;
        dict.set_item("claim_type", format!("{:?}", self.claim_type))?;
        dict.set_item("capability_proof", format!("{:?}", self.capability_proof))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}dao_escrow_bulla: {:?}", self.dao_escrow_bulla).unwrap();
        writeln!(out, "{prefix}claim_id: {:?}", self.claim_id).unwrap();
        writeln!(out, "{prefix}value: {:?}", self.value).unwrap();
        writeln!(out, "{prefix}description_hash: {:?}", self.description_hash).unwrap();
        writeln!(out, "{prefix}recipient_pubkey: {}", self.recipient_pubkey).unwrap();
        writeln!(out, "{prefix}proposer_pubkey: {}", self.proposer_pubkey).unwrap();
        writeln!(out, "{prefix}claim_type: {:?}", self.claim_type).unwrap();
        writeln!(out, "{prefix}capability_proof: {:?}", self.capability_proof).unwrap();
        Ok(())
    }
}
