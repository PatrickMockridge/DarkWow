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

use dwow_darkbet_exchange_contract::model as darkbet_exchange_model;
use dwow_sdk::hex::AsHex;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`darkbet_exchange_model::AddLiquidityParamsV1`] python binding.
#[pyclass]
pub struct DarkbetAddLiquidityParamsV1(darkbet_exchange_model::AddLiquidityParamsV1);
impl_py_methods!(DarkbetAddLiquidityParamsV1);

impl FunctionParams for darkbet_exchange_model::AddLiquidityParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("market_id", format!("{:?}", self.market_id))?;
        dict.set_item("amount", self.amount)?;
        dict.set_item("provider", self.provider.to_string())?;
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("signature", format!("{:?}", self.signature))?;
        dict.set_item("instance_seed", self.instance_seed.hex())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}market_id: {:?}", self.market_id).unwrap();
        writeln!(out, "{prefix}amount: {}", self.amount).unwrap();
        writeln!(out, "{prefix}provider: {}", self.provider).unwrap();
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}signature: {:?}", self.signature).unwrap();
        writeln!(out, "{prefix}instance_seed: [{} bytes]", self.instance_seed.len()).unwrap();
        Ok(())
    }
}
