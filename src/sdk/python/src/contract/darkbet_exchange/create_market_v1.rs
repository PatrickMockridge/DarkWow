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

/// [`darkbet_exchange_model::CreateMarketParamsV1`] python binding.
#[pyclass]
pub struct DarkbetCreateMarketParamsV1(darkbet_exchange_model::CreateMarketParamsV1);
impl_py_methods!(DarkbetCreateMarketParamsV1);

impl FunctionParams for darkbet_exchange_model::CreateMarketParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("description", self.description.clone())?;
        dict.set_item("outcomes", self.outcomes.clone())?;
        dict.set_item("oracle_id", format!("{:?}", self.oracle_id))?;
        dict.set_item("commission_bp", self.commission_bp)?;
        dict.set_item("market_type", self.market_type)?;
        dict.set_item("protocol_fee", self.protocol_fee)?;
        dict.set_item("lp_fee", self.lp_fee)?;
        dict.set_item("duration_blocks", self.duration_blocks)?;
        dict.set_item("creator_pub", self.creator_pub.to_string())?;
        dict.set_item("signature", format!("{:?}", self.signature))?;
        dict.set_item("instance_seed", self.instance_seed.hex())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}description: {}", self.description).unwrap();
        writeln!(out, "{prefix}outcomes: {:?}", self.outcomes).unwrap();
        writeln!(out, "{prefix}oracle_id: {:?}", self.oracle_id).unwrap();
        writeln!(out, "{prefix}commission_bp: {}", self.commission_bp).unwrap();
        writeln!(out, "{prefix}market_type: {}", self.market_type).unwrap();
        writeln!(out, "{prefix}protocol_fee: {}", self.protocol_fee).unwrap();
        writeln!(out, "{prefix}lp_fee: {}", self.lp_fee).unwrap();
        writeln!(out, "{prefix}duration_blocks: {}", self.duration_blocks).unwrap();
        writeln!(out, "{prefix}creator_pub: {}", self.creator_pub).unwrap();
        writeln!(out, "{prefix}signature: {:?}", self.signature).unwrap();
        writeln!(out, "{prefix}instance_seed: [{} bytes]", self.instance_seed.len()).unwrap();
        Ok(())
    }
}
