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

/// [`relayer_endowment_model::DeployCapitalParamsV1`] python binding.
#[pyclass]
pub struct RelayerEndowmentDeployCapitalParamsV1(relayer_endowment_model::DeployCapitalParamsV1);
impl_py_methods!(RelayerEndowmentDeployCapitalParamsV1);

impl FunctionParams for relayer_endowment_model::DeployCapitalParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("instance_seed", format!("{:?}", self.instance_seed))?;
        dict.set_item("relayer_pub", self.relayer_pub.to_string())?;
        dict.set_item("amount", format!("{:?}", self.amount))?;
        dict.set_item("backer_cut_bp", format!("{:?}", self.backer_cut_bp))?;
        dict.set_item("signature_public", self.signature_public.to_string())?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("min_success_rate_bp", format!("{:?}", self.min_success_rate_bp))?;
        dict.set_item("max_slash_count", format!("{:?}", self.max_slash_count))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}instance_seed: {:?}", self.instance_seed).unwrap();
        writeln!(out, "{prefix}relayer_pub: {}", self.relayer_pub).unwrap();
        writeln!(out, "{prefix}amount: {:?}", self.amount).unwrap();
        writeln!(out, "{prefix}backer_cut_bp: {:?}", self.backer_cut_bp).unwrap();
        writeln!(out, "{prefix}signature_public: {}", self.signature_public).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}min_success_rate_bp: {:?}", self.min_success_rate_bp).unwrap();
        writeln!(out, "{prefix}max_slash_count: {:?}", self.max_slash_count).unwrap();
        Ok(())
    }
}
