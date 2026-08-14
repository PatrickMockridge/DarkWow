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

use dwow_stablecoin_contract::model as stablecoin_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`stablecoin_model::GovernanceReportParams`] python binding.
#[pyclass]
pub struct StablecoinGovernanceReportParamsV1(stablecoin_model::GovernanceReportParams);
impl_py_methods!(StablecoinGovernanceReportParamsV1);

impl FunctionParams for stablecoin_model::GovernanceReportParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("token_id", format!("{:?}", self.token_id))?;
        dict.set_item("total_collateral", format!("{:?}", self.total_collateral))?;
        dict.set_item("total_debt", format!("{:?}", self.total_debt))?;
        dict.set_item("total_redeemed", format!("{:?}", self.total_redeemed))?;
        dict.set_item("outstanding", format!("{:?}", self.outstanding))?;
        dict.set_item("collateral_ratio_bps", format!("{:?}", self.collateral_ratio_bps))?;
        dict.set_item("interest_accrued", format!("{:?}", self.interest_accrued))?;
        dict.set_item("report_timestamp", format!("{:?}", self.report_timestamp))?;
        dict.set_item("reporter_pub", format!("{:?}", self.reporter_pub))?;
        dict.set_item("proof", format!("{:?}", self.proof))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}token_id: {:?}", self.token_id).unwrap();
        writeln!(out, "{prefix}total_collateral: {:?}", self.total_collateral).unwrap();
        writeln!(out, "{prefix}total_debt: {:?}", self.total_debt).unwrap();
        writeln!(out, "{prefix}total_redeemed: {:?}", self.total_redeemed).unwrap();
        writeln!(out, "{prefix}outstanding: {:?}", self.outstanding).unwrap();
        writeln!(out, "{prefix}collateral_ratio_bps: {:?}", self.collateral_ratio_bps).unwrap();
        writeln!(out, "{prefix}interest_accrued: {:?}", self.interest_accrued).unwrap();
        writeln!(out, "{prefix}report_timestamp: {:?}", self.report_timestamp).unwrap();
        writeln!(out, "{prefix}reporter_pub: {:?}", self.reporter_pub).unwrap();
        writeln!(out, "{prefix}proof: {:?}", self.proof).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        Ok(())
    }
}
