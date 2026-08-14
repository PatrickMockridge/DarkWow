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

use dwow_subscription_contract::model as subscription_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`subscription_model::SubscribeParamsV1`] python binding.
#[pyclass]
pub struct SubscriptionSubscribeParamsV1(subscription_model::SubscribeParamsV1);
impl_py_methods!(SubscriptionSubscribeParamsV1);

impl FunctionParams for subscription_model::SubscribeParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("plan_id", format!("{:?}", self.plan_id))?;
        dict.set_item("subscriber_pubkey", self.subscriber_pubkey.to_string())?;
        dict.set_item("commitment", format!("{:?}", self.commitment))?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("merkle_proof", format!("{:?}", self.merkle_proof))?;
        dict.set_item("merkle_root", format!("{:?}", self.merkle_root))?;
        dict.set_item("dao_escrow_bulla", format!("{:?}", self.dao_escrow_bulla))?;
        dict.set_item("dao_membership_note", format!("{:?}", self.dao_membership_note))?;
        dict.set_item("dao_escrow_merkle_root", format!("{:?}", self.dao_escrow_merkle_root))?;
        dict.set_item("dao_merkle_proof", format!("{:?}", self.dao_merkle_proof))?;
        dict.set_item("dao_leaf_pos", format!("{:?}", self.dao_leaf_pos))?;
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}plan_id: {:?}", self.plan_id).unwrap();
        writeln!(out, "{prefix}subscriber_pubkey: {}", self.subscriber_pubkey).unwrap();
        writeln!(out, "{prefix}commitment: {:?}", self.commitment).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}merkle_proof: {:?}", self.merkle_proof).unwrap();
        writeln!(out, "{prefix}merkle_root: {:?}", self.merkle_root).unwrap();
        writeln!(out, "{prefix}dao_escrow_bulla: {:?}", self.dao_escrow_bulla).unwrap();
        writeln!(out, "{prefix}dao_membership_note: {:?}", self.dao_membership_note).unwrap();
        writeln!(out, "{prefix}dao_escrow_merkle_root: {:?}", self.dao_escrow_merkle_root).unwrap();
        writeln!(out, "{prefix}dao_merkle_proof: {:?}", self.dao_merkle_proof).unwrap();
        writeln!(out, "{prefix}dao_leaf_pos: {:?}", self.dao_leaf_pos).unwrap();
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        Ok(())
    }
}
