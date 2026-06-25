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

use structopt::StructOpt;
use url::Url;

#[derive(Clone)]
pub struct RpcSettings {
    pub listen: Url,
    pub disabled_methods: Vec<String>,
}

impl RpcSettings {
    pub fn is_method_disabled(&self, method: &String) -> bool {
        self.disabled_methods.contains(method)
    }
    pub fn use_http(&self) -> bool {
        self.listen.scheme().starts_with("http+")
    }
    /// Returns true if the listen URL binds to localhost.
    pub fn is_localhost(&self) -> bool {
        matches!(self.listen.host_str(), Some("127.0.0.1") | Some("localhost") | Some("::1"))
    }
    /// Returns true if the listen URL binds to all interfaces (0.0.0.0).
    /// Accepted for Docker devnet where stratum/mm_rpc clients run in separate
    /// containers on the same bridge network.
    pub fn is_wildcard(&self) -> bool {
        matches!(self.listen.host_str(), Some("0.0.0.0"))
    }
}

impl Default for RpcSettings {
    fn default() -> Self {
        Self { listen: Url::parse("tcp://127.0.0.1:22222").unwrap(), disabled_methods: vec![] }
    }
}

// Defines the JSON-RPC settings.
#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
#[serde(rename = "rpc")]
pub struct RpcSettingsOpt {
    /// RPC server listen address
    #[structopt(long, default_value = "tcp://127.0.0.1:22222")]
    pub rpc_listen: Url,

    /// Disabled JSON-RPC methods
    #[structopt(long, use_delimiter = true)]
    pub rpc_disabled_methods: Option<Vec<String>>,
}

impl From<RpcSettingsOpt> for RpcSettings {
    fn from(opt: RpcSettingsOpt) -> Self {
        Self {
            listen: opt.rpc_listen,
            disabled_methods: opt.rpc_disabled_methods.unwrap_or_default(),
        }
    }
}
