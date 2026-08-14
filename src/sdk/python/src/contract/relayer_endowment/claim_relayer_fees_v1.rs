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

use dwow_relayer_endowment_contract::model as relayer_endowment_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`relayer_endowment_model::ClaimFeesParamsV1`] python binding.
#[pyclass]
pub struct RelayerEndowmentClaimRelayerFeesParamsV1(relayer_endowment_model::ClaimFeesParamsV1);
impl_py_methods!(RelayerEndowmentClaimRelayerFeesParamsV1);

impl FunctionParams for relayer_endowment_model::ClaimFeesParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("deployment_id", format!("{:?}", self.deployment_id))?;
        dict.set_item("backer_pub_x", format!("{:?}", self.backer_pub_x))?;
        dict.set_item("backer_pub_y", format!("{:?}", self.backer_pub_y))?;
        dict.set_item("fee_share", format!("{:?}", self.fee_share))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}deployment_id: {:?}", self.deployment_id).unwrap();
        writeln!(out, "{prefix}backer_pub_x: {:?}", self.backer_pub_x).unwrap();
        writeln!(out, "{prefix}backer_pub_y: {:?}", self.backer_pub_y).unwrap();
        writeln!(out, "{prefix}fee_share: {:?}", self.fee_share).unwrap();
        Ok(())
    }
}
