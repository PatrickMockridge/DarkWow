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

/// [`dao_escrow_model::PayPremiumParamsV1`] python binding.
#[pyclass]
pub struct DaoEscrowPayPremiumParamsV1(dao_escrow_model::PayPremiumParamsV1);
impl_py_methods!(DaoEscrowPayPremiumParamsV1);

impl FunctionParams for dao_escrow_model::PayPremiumParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("dao_escrow_bulla", format!("{:?}", self.dao_escrow_bulla))?;
        dict.set_item("membership_note", format!("{:?}", self.membership_note))?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("value", format!("{:?}", self.value))?;
        dict.set_item("token_id", format!("{:?}", self.token_id))?;
        dict.set_item("expiry", format!("{:?}", self.expiry))?;
        dict.set_item("membership_blind", format!("{:?}", self.membership_blind))?;
        dict.set_item("value_blind", format!("{:?}", self.value_blind))?;
        dict.set_item("member_pubkey", self.member_pubkey.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}dao_escrow_bulla: {:?}", self.dao_escrow_bulla).unwrap();
        writeln!(out, "{prefix}membership_note: {:?}", self.membership_note).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}value: {:?}", self.value).unwrap();
        writeln!(out, "{prefix}token_id: {:?}", self.token_id).unwrap();
        writeln!(out, "{prefix}expiry: {:?}", self.expiry).unwrap();
        writeln!(out, "{prefix}membership_blind: {:?}", self.membership_blind).unwrap();
        writeln!(out, "{prefix}value_blind: {:?}", self.value_blind).unwrap();
        writeln!(out, "{prefix}member_pubkey: {}", self.member_pubkey).unwrap();
        Ok(())
    }
}
