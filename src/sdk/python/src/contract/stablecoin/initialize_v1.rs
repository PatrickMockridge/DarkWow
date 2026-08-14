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

/// [`stablecoin_model::InitializeParams`] python binding.
#[pyclass]
pub struct StablecoinInitializeParamsV1(stablecoin_model::InitializeParams);
impl_py_methods!(StablecoinInitializeParamsV1);

impl FunctionParams for stablecoin_model::InitializeParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("model", format!("{:?}", self.model))?;
        dict.set_item("min_collateralization_ratio", format!("{:?}", self.min_collateralization_ratio))?;
        dict.set_item("liquidation_threshold", format!("{:?}", self.liquidation_threshold))?;
        dict.set_item("liquidation_penalty", format!("{:?}", self.liquidation_penalty))?;
        dict.set_item("base_rate", format!("{:?}", self.base_rate))?;
        dict.set_item("pi_kp", format!("{:?}", self.pi_kp))?;
        dict.set_item("pi_ki", format!("{:?}", self.pi_ki))?;
        dict.set_item("twap_window", format!("{:?}", self.twap_window))?;
        dict.set_item("price_deviation_threshold", format!("{:?}", self.price_deviation_threshold))?;
        dict.set_item("collateral_params", format!("{:?}", self.collateral_params))?;
        dict.set_item("dead_man_switch", format!("{:?}", self.dead_man_switch))?;
        dict.set_item("token_authority_pub", format!("{:?}", self.token_authority_pub))?;
        dict.set_item("create_token", format!("{:?}", self.create_token))?;
        dict.set_item("token_symbol", format!("{:?}", self.token_symbol))?;
        dict.set_item("deployer_auth", format!("{:?}", self.deployer_auth))?;
        dict.set_item("promissory_note_contract_id", format!("{:?}", self.promissory_note_contract_id))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}model: {:?}", self.model).unwrap();
        writeln!(out, "{prefix}min_collateralization_ratio: {:?}", self.min_collateralization_ratio).unwrap();
        writeln!(out, "{prefix}liquidation_threshold: {:?}", self.liquidation_threshold).unwrap();
        writeln!(out, "{prefix}liquidation_penalty: {:?}", self.liquidation_penalty).unwrap();
        writeln!(out, "{prefix}base_rate: {:?}", self.base_rate).unwrap();
        writeln!(out, "{prefix}pi_kp: {:?}", self.pi_kp).unwrap();
        writeln!(out, "{prefix}pi_ki: {:?}", self.pi_ki).unwrap();
        writeln!(out, "{prefix}twap_window: {:?}", self.twap_window).unwrap();
        writeln!(out, "{prefix}price_deviation_threshold: {:?}", self.price_deviation_threshold).unwrap();
        writeln!(out, "{prefix}collateral_params: {:?}", self.collateral_params).unwrap();
        writeln!(out, "{prefix}dead_man_switch: {:?}", self.dead_man_switch).unwrap();
        writeln!(out, "{prefix}token_authority_pub: {:?}", self.token_authority_pub).unwrap();
        writeln!(out, "{prefix}create_token: {:?}", self.create_token).unwrap();
        writeln!(out, "{prefix}token_symbol: {:?}", self.token_symbol).unwrap();
        writeln!(out, "{prefix}deployer_auth: {:?}", self.deployer_auth).unwrap();
        writeln!(out, "{prefix}promissory_note_contract_id: {:?}", self.promissory_note_contract_id).unwrap();
        Ok(())
    }
}
