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

/// [`stablecoin_model::RedeemStableParamsV1`] python binding.
#[pyclass]
pub struct StablecoinRedeemStableParamsV1(stablecoin_model::RedeemStableParamsV1);
impl_py_methods!(StablecoinRedeemStableParamsV1);

impl FunctionParams for stablecoin_model::RedeemStableParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("recipient_pub", format!("{:?}", self.recipient_pub))?;
        dict.set_item("redeem_amount", format!("{:?}", self.redeem_amount))?;
        dict.set_item("asset_id", format!("{:?}", self.asset_id))?;
        dict.set_item("receipt_spend_hook", format!("{:?}", self.receipt_spend_hook))?;
        dict.set_item("total_debt", format!("{:?}", self.total_debt))?;
        dict.set_item("total_collateral", format!("{:?}", self.total_collateral))?;
        dict.set_item("proof", format!("{:?}", self.proof))?;
        dict.set_item("fee", format!("{:?}", self.fee))?;
        dict.set_item("zk_public_inputs", format!("{:?}", self.zk_public_inputs))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}recipient_pub: {:?}", self.recipient_pub).unwrap();
        writeln!(out, "{prefix}redeem_amount: {:?}", self.redeem_amount).unwrap();
        writeln!(out, "{prefix}asset_id: {:?}", self.asset_id).unwrap();
        writeln!(out, "{prefix}receipt_spend_hook: {:?}", self.receipt_spend_hook).unwrap();
        writeln!(out, "{prefix}total_debt: {:?}", self.total_debt).unwrap();
        writeln!(out, "{prefix}total_collateral: {:?}", self.total_collateral).unwrap();
        writeln!(out, "{prefix}proof: {:?}", self.proof).unwrap();
        writeln!(out, "{prefix}fee: {:?}", self.fee).unwrap();
        writeln!(out, "{prefix}zk_public_inputs: {:?}", self.zk_public_inputs).unwrap();
        Ok(())
    }
}
