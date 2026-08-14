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

/// [`subscription_model::RenewParamsV1`] python binding.
#[pyclass]
pub struct SubscriptionRenewParamsV1(subscription_model::RenewParamsV1);
impl_py_methods!(SubscriptionRenewParamsV1);

impl FunctionParams for subscription_model::RenewParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("subscription_id", format!("{:?}", self.subscription_id))?;
        dict.set_item("subscriber_secret", format!("{:?}", self.subscriber_secret))?;
        dict.set_item("new_lock_until_block", format!("{:?}", self.new_lock_until_block))?;
        dict.set_item("spent_nullifier", format!("{:?}", self.spent_nullifier))?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("merkle_proof", format!("{:?}", self.merkle_proof))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}subscription_id: {:?}", self.subscription_id).unwrap();
        writeln!(out, "{prefix}subscriber_secret: {:?}", self.subscriber_secret).unwrap();
        writeln!(out, "{prefix}new_lock_until_block: {:?}", self.new_lock_until_block).unwrap();
        writeln!(out, "{prefix}spent_nullifier: {:?}", self.spent_nullifier).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}merkle_proof: {:?}", self.merkle_proof).unwrap();
        Ok(())
    }
}
