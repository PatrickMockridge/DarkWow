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

use dwow_game_room_contract::model as game_room_model;
use dwow_sdk::hex::AsHex;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`game_room_model::CreateRoomParamsV1`] python binding.
#[pyclass]
pub struct GameRoomCreateRoomParamsV1(game_room_model::CreateRoomParamsV1);
impl_py_methods!(GameRoomCreateRoomParamsV1);

impl FunctionParams for game_room_model::CreateRoomParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("owner", self.owner.to_string())?;
        dict.set_item("asset_id", format!("{:?}", self.asset_id))?;
        dict.set_item("min_stake", self.min_stake)?;
        dict.set_item("max_stake", self.max_stake)?;
        dict.set_item("entropy_mode", format!("{:?}", self.entropy_mode))?;
        dict.set_item("confirmation_depth", self.confirmation_depth)?;
        dict.set_item("required_entropy_contributions", self.required_entropy_contributions)?;
        dict.set_item("entropy_contribution_deadline", self.entropy_contribution_deadline)?;
        dict.set_item("max_players", self.max_players)?;
        dict.set_item("nonce", format!("{:?}", self.nonce))?;
        dict.set_item("instance_seed", self.instance_seed.hex())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}owner: {}", self.owner).unwrap();
        writeln!(out, "{prefix}asset_id: {:?}", self.asset_id).unwrap();
        writeln!(out, "{prefix}min_stake: {}", self.min_stake).unwrap();
        writeln!(out, "{prefix}max_stake: {}", self.max_stake).unwrap();
        writeln!(out, "{prefix}entropy_mode: {:?}", self.entropy_mode).unwrap();
        writeln!(out, "{prefix}confirmation_depth: {}", self.confirmation_depth).unwrap();
        writeln!(out, "{prefix}required_entropy_contributions: {}", self.required_entropy_contributions).unwrap();
        writeln!(out, "{prefix}entropy_contribution_deadline: {}", self.entropy_contribution_deadline).unwrap();
        writeln!(out, "{prefix}max_players: {}", self.max_players).unwrap();
        writeln!(out, "{prefix}nonce: {:?}", self.nonce).unwrap();
        writeln!(out, "{prefix}instance_seed: [{} bytes]", self.instance_seed.len()).unwrap();
        Ok(())
    }
}
