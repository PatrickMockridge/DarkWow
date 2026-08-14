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

/// [`subscription_model::UpdateUsageParamsV1`] python binding.
#[pyclass]
pub struct SubscriptionUpdateUsageParamsV1(subscription_model::UpdateUsageParamsV1);
impl_py_methods!(SubscriptionUpdateUsageParamsV1);

impl FunctionParams for subscription_model::UpdateUsageParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("subscription_id", format!("{:?}", self.subscription_id))?;
        dict.set_item("subscriber_pub_x", format!("{:?}", self.subscriber_pub_x))?;
        dict.set_item("subscriber_pub_y", format!("{:?}", self.subscriber_pub_y))?;
        dict.set_item("subscriber_secret", format!("{:?}", self.subscriber_secret))?;
        dict.set_item("current_block", format!("{:?}", self.current_block))?;
        dict.set_item("nonce", format!("{:?}", self.nonce))?;
        dict.set_item("spent_nullifier", format!("{:?}", self.spent_nullifier))?;
        dict.set_item("merkle_proof", format!("{:?}", self.merkle_proof))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}subscription_id: {:?}", self.subscription_id).unwrap();
        writeln!(out, "{prefix}subscriber_pub_x: {:?}", self.subscriber_pub_x).unwrap();
        writeln!(out, "{prefix}subscriber_pub_y: {:?}", self.subscriber_pub_y).unwrap();
        writeln!(out, "{prefix}subscriber_secret: {:?}", self.subscriber_secret).unwrap();
        writeln!(out, "{prefix}current_block: {:?}", self.current_block).unwrap();
        writeln!(out, "{prefix}nonce: {:?}", self.nonce).unwrap();
        writeln!(out, "{prefix}spent_nullifier: {:?}", self.spent_nullifier).unwrap();
        writeln!(out, "{prefix}merkle_proof: {:?}", self.merkle_proof).unwrap();
        Ok(())
    }
}
