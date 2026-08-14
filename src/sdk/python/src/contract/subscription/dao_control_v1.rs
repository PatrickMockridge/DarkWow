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

/// [`subscription_model::DaoControlParamsV1`] python binding.
#[pyclass]
pub struct SubscriptionDaoControlParamsV1(subscription_model::DaoControlParamsV1);
impl_py_methods!(SubscriptionDaoControlParamsV1);

impl FunctionParams for subscription_model::DaoControlParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        match self {
            Self::UpdatePlan(plan) => {
                dict.set_item("variant", "UpdatePlan")?;
                dict.set_item("plan", format!("{:?}", plan))?;
            }
            Self::SetPlanActive { plan_id, active } => {
                dict.set_item("variant", "SetPlanActive")?;
                dict.set_item("plan_id", format!("{:?}", plan_id))?;
                dict.set_item("active", format!("{:?}", active))?;
            }
            Self::EmergencyPause { pause, reason } => {
                dict.set_item("variant", "EmergencyPause")?;
                dict.set_item("pause", format!("{:?}", pause))?;
                dict.set_item("reason", format!("{:?}", reason))?;
            }
            Self::EndowmentWithdraw { amount, recipient } => {
                dict.set_item("variant", "EndowmentWithdraw")?;
                dict.set_item("amount", format!("{:?}", amount))?;
                dict.set_item("recipient", recipient.to_string())?;
            }
            Self::Slash { subscription_id, reason } => {
                dict.set_item("variant", "Slash")?;
                dict.set_item("subscription_id", format!("{:?}", subscription_id))?;
                dict.set_item("reason", format!("{:?}", reason))?;
            }
        }
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        match self {
            Self::UpdatePlan(plan) => {
                writeln!(out, "{prefix}variant: UpdatePlan").unwrap();
                writeln!(out, "{prefix}plan: {:?}", plan).unwrap();
            }
            Self::SetPlanActive { plan_id, active } => {
                writeln!(out, "{prefix}variant: SetPlanActive").unwrap();
                writeln!(out, "{prefix}plan_id: {:?}", plan_id).unwrap();
                writeln!(out, "{prefix}active: {:?}", active).unwrap();
            }
            Self::EmergencyPause { pause, reason } => {
                writeln!(out, "{prefix}variant: EmergencyPause").unwrap();
                writeln!(out, "{prefix}pause: {:?}", pause).unwrap();
                writeln!(out, "{prefix}reason: {:?}", reason).unwrap();
            }
            Self::EndowmentWithdraw { amount, recipient } => {
                writeln!(out, "{prefix}variant: EndowmentWithdraw").unwrap();
                writeln!(out, "{prefix}amount: {:?}", amount).unwrap();
                writeln!(out, "{prefix}recipient: {}", recipient).unwrap();
            }
            Self::Slash { subscription_id, reason } => {
                writeln!(out, "{prefix}variant: Slash").unwrap();
                writeln!(out, "{prefix}subscription_id: {:?}", subscription_id).unwrap();
                writeln!(out, "{prefix}reason: {:?}", reason).unwrap();
            }
        }
        Ok(())
    }
}
