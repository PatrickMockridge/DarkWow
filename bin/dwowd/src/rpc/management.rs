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

use std::collections::HashSet;

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::debug;

use dwow_core::{
    net::P2pPtr,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
};

use crate::DwowNode;

/// JSON-RPC `RequestHandler` for node management
pub struct ManagementRpcHandler;

#[async_trait]
#[rustfmt::skip]
impl RequestHandler<ManagementRpcHandler> for DwowNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "dwowd::rpc::management_rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =======================
            // Node management methods
            // =======================
            "ping" => <DwowNode as RequestHandler<ManagementRpcHandler>>::pong(self, req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,
            // Account management
            "accounts.list" => self.accounts_list(req.id, req.params).await,
            "accounts.set_default" => self.accounts_set_default(req.id, req.params).await,
            "accounts.import" => self.accounts_import(req.id, req.params).await,
            "accounts.generate" => self.accounts_generate(req.id, req.params).await,
            "accounts.remove" => self.accounts_remove(req.id, req.params).await,
            "accounts.export" => self.accounts_export(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_state.management_rpc_connections.lock().await
    }
}

impl HandlerP2p for DwowNode {
    fn p2p(&self) -> P2pPtr {
        self.p2p_handler.p2p.clone()
    }
}

impl DwowNode {
    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated.
    //
    // Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.switch", "params": [true], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.p2p_handler.p2p.dnet_enable();
        } else {
            self.p2p_handler.p2p.dnet_disable();
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to P2P dnet events.
    // Once a subscription is established, `dwowd` will send JSON-RPC
    // notifications of new network events to the subscriber.
    //
    // --> {
    //       "jsonrpc": "2.0",
    //       "method": "dnet.subscribe_events",
    //       "params": [],
    //       "id": 1
    //     }
    // <-- {
    //       "jsonrpc": "2.0",
    //       "method": "dnet.subscribe_events",
    //       "params": [
    //         {
    //           "chan": {"Channel": "Info"},
    //           "cmd": "command",
    //           "time": 1767016282
    //         }
    //       ]
    //     }
    pub async fn dnet_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.rpc_state.subscribers.get("dnet").unwrap().clone().into()
    }

    // ========================================================================
    // Account management RPC methods
    // ========================================================================

    /// List all accounts. Returns JSON array with index, address, label, is_default.
    /// --> {"jsonrpc": "2.0", "method": "accounts.list", "params": [], "id": 1}
    pub async fn accounts_list(&self, id: u16, params: JsonValue) -> JsonResult {
        let _ = params;
        let mgr = self.account_manager.read().await;
        let mut accounts = Vec::new();
        for (i, a) in mgr.accounts().iter().enumerate() {
            let mut obj = std::collections::HashMap::new();
            obj.insert("index".to_string(), JsonValue::Number(i as f64));
            obj.insert("address".to_string(), JsonValue::String(a.address(mgr.network)));
            obj.insert("label".to_string(), match &a.label {
                Some(l) => JsonValue::String(l.clone()),
                None => JsonValue::Null,
            });
            obj.insert("is_default".to_string(), JsonValue::Boolean(i == mgr.default_index()));
            accounts.push(JsonValue::Object(obj));
        }
        JsonResponse::new(JsonValue::Array(accounts), id).into()
    }

    /// Set the default account for mining.
    /// --> {"jsonrpc": "2.0", "method": "accounts.set_default", "params": [0], "id": 1}
    pub async fn accounts_set_default(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_number() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let index = *params[0].get::<f64>().unwrap() as usize;
        let mut mgr = self.account_manager.write().await;
        match mgr.set_default(index) {
            Ok(()) => {
                // Persist the change so it survives restart
                if let Err(e) = mgr.persist_to_sled(&self.sled_db) {
                    return JsonError::new(ErrorCode::InternalError, Some(format!("set_default succeeded but persist failed: {e}")), id).into();
                }
                JsonResponse::new(JsonValue::String(format!("Default account set to {}", index)), id).into()
            }
            Err(e) => JsonError::new(ErrorCode::InternalError, Some(e), id).into(),
        }
    }

    /// Import an account from a hex secret.
    /// --> {"jsonrpc": "2.0", "method": "accounts.import", "params": ["<hex>"], "id": 1}
    pub async fn accounts_import(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.is_empty() || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let hex_secret = params[0].get::<String>().unwrap();
        let mut mgr = self.account_manager.write().await;
        match mgr.import_hex(&hex_secret) {
            Ok(idx) => {
                if let Err(e) = mgr.persist_to_sled(&self.sled_db) {
                    return JsonError::new(ErrorCode::InternalError, Some(format!("import succeeded but persist failed: {e}")), id).into();
                }
                let addr = mgr.accounts()[idx].address(mgr.network);
                let mut obj = std::collections::HashMap::new();
                obj.insert("index".to_string(), JsonValue::Number(idx as f64));
                obj.insert("address".to_string(), JsonValue::String(addr));
                JsonResponse::new(JsonValue::Object(obj), id).into()
            }
            Err(e) => JsonError::new(ErrorCode::InternalError, Some(e), id).into(),
        }
    }

    /// Generate a new random account.
    /// --> {"jsonrpc": "2.0", "method": "accounts.generate", "params": [], "id": 1}
    pub async fn accounts_generate(&self, id: u16, _params: JsonValue) -> JsonResult {
        let mut mgr = self.account_manager.write().await;
        let idx = mgr.generate();
        if let Err(e) = mgr.persist_to_sled(&self.sled_db) {
            return JsonError::new(ErrorCode::InternalError, Some(format!("generate succeeded but persist failed: {e}")), id).into();
        }
        let addr = mgr.accounts()[idx].address(mgr.network);
        let mut obj = std::collections::HashMap::new();
        obj.insert("index".to_string(), JsonValue::Number(idx as f64));
        obj.insert("address".to_string(), JsonValue::String(addr));
        JsonResponse::new(JsonValue::Object(obj), id).into()
    }

    /// Remove an account by index.
    /// --> {"jsonrpc": "2.0", "method": "accounts.remove", "params": [0], "id": 1}
    pub async fn accounts_remove(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_number() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let index = *params[0].get::<f64>().unwrap() as usize;
        let mut mgr = self.account_manager.write().await;
        match mgr.remove(index) {
            Ok(()) => {
                if let Err(e) = mgr.persist_to_sled(&self.sled_db) {
                    return JsonError::new(ErrorCode::InternalError, Some(format!("remove succeeded but persist failed: {e}")), id).into();
                }
                JsonResponse::new(JsonValue::String(format!("Account {} removed", index)), id).into()
            }
            Err(e) => JsonError::new(ErrorCode::InternalError, Some(e), id).into(),
        }
    }

    /// Export the secret hex for an account by index.
    /// WARNING: Returns the raw secret key. Only available on 127.0.0.1.
    /// --> {"jsonrpc": "2.0", "method": "accounts.export", "params": [0], "id": 1}
    pub async fn accounts_export(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_number() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let index = *params[0].get::<f64>().unwrap() as usize;
        let mgr = self.account_manager.read().await;
        match mgr.export_hex(index) {
            Ok(hex_secret) => {
                let mut obj = std::collections::HashMap::new();
                obj.insert("index".to_string(), JsonValue::Number(index as f64));
                obj.insert("secret_hex".to_string(), JsonValue::String(hex_secret));
                JsonResponse::new(JsonValue::Object(obj), id).into()
            }
            Err(e) => JsonError::new(ErrorCode::InternalError, Some(e), id).into(),
        }
    }
}
