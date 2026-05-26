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

//! JSON-RPC interface for Tau_Pallas
//!
//! This module provides the JSON-RPC API methods for tau_pallas.
//! These can be exposed via a JSON-RPC server or called directly.
//!
//! Phase 3 adds labor market integration methods.

use std::collections::HashMap;
use std::path::PathBuf;

use dwow_sdk::crypto::pasta_prelude::PrimeField;
use tinyjson::JsonValue;
use tracing::debug;

use crate::{
    capability::{self, verify_capability_offchain, CapabilityProof},
    error::{TauPallasError, TauPallasResult},
    task_info::{Comment, TaskInfo, VerificationMode},
    util::set_event,
    month_tasks::MonthTasks,
    rpc_client::DarkfidClient,
};

/// Default workspace name
const DEFAULT_WORKSPACE: &str = "darkfi-dev";

/// RPC handler state
pub struct RpcHandler {
    /// Path to the dataset storage
    dataset_path: PathBuf,
    /// Current workspace
    workspace: String,
    /// Optional Darkfid RPC client for on-chain operations
    rpc_client: Option<DarkfidClient>,
}

impl RpcHandler {
    /// Create a new RPC handler
    pub fn new(dataset_path: PathBuf) -> Self {
        Self { dataset_path, workspace: DEFAULT_WORKSPACE.to_string(), rpc_client: None }
    }

    /// Create a new RPC handler with a Darkfid RPC client for on-chain operations
    pub fn with_rpc_client(dataset_path: PathBuf, rpc_client: DarkfidClient) -> Self {
        Self { dataset_path, workspace: DEFAULT_WORKSPACE.to_string(), rpc_client: Some(rpc_client) }
    }

    /// Check if on-chain operations are available
    pub fn has_rpc_client(&self) -> bool {
        self.rpc_client.is_some()
    }

    /// Handle a JSON-RPC request
    pub async fn handle_request(&self, method: &str, params: JsonValue) -> TauPallasResult<JsonValue> {
        match method {
            // Basic task operations
            "add" => self.add(params).await,
            "get_ref_ids" => self.get_ref_ids(params).await,
            "get_task_by_ref_id" => self.get_task_by_ref_id(params).await,
            "modify" => self.modify(params).await,
            "set_state" => self.set_state(params).await,
            "set_comment" => self.set_comment(params).await,
            "switch_ws" => self.switch_ws(params).await,
            "get_ws" => self.get_ws(params).await,

            // O-Cap task operations
            "claim_task" => self.claim_task(params).await,
            "set_task_capability" => self.set_task_capability(params).await,

            // Phase 3: Labor market integration
            "link_task_to_job" => self.link_task_to_job(params).await,
            "submit_task_deliverable" => self.submit_task_deliverable(params).await,
            "register_capability" => self.register_capability(params).await,

            _ => Err(TauPallasError::InvalidData(format!("Unknown method: {}", method))),
        }
    }

    // ==================== Basic Task Operations ====================

    /// RPCAPI:
    /// Add new task and returns `true` upon success.
    ///
    /// --> {"jsonrpc": "2.0", "method": "add",
    ///      "params": [{"title": "..", "desc": "..", "tags": [], "assign": [],
    ///                 "project": [], "due": null, "rank": null}],
    ///      "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn add(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::add() params {params:?}");

        if !params[0].is_object() {
            return Err(TauPallasError::InvalidData("Invalid parameters".to_string()))
        }

        let params = params[0].get::<HashMap<String, JsonValue>>().unwrap();

        let due = match params["due"] {
            JsonValue::Null => None,
            JsonValue::Number(numba) => Some(dwow_core::util::time::Timestamp::from_u64(numba as u64)),
            _ => return Err(TauPallasError::InvalidData("Invalid parameter \"due\"".to_string())),
        };

        let rank = match params["rank"] {
            JsonValue::Null => None,
            JsonValue::Number(numba) => Some(numba as f32),
            _ => return Err(TauPallasError::InvalidData("Invalid parameter \"rank\"".to_string())),
        };

        let tags = params["tags"]
            .get::<Vec<JsonValue>>()
            .unwrap()
            .iter()
            .filter_map(|v| v.get::<String>().cloned())
            .collect::<Vec<_>>();

        let assigns = params["assign"]
            .get::<Vec<JsonValue>>()
            .unwrap()
            .iter()
            .filter_map(|v| v.get::<String>().cloned())
            .collect::<Vec<_>>();

        let projects = params["project"]
            .get::<Vec<JsonValue>>()
            .unwrap()
            .iter()
            .filter_map(|v| v.get::<String>().cloned())
            .collect::<Vec<_>>();

        let created_at = match params["created_at"] {
            JsonValue::Number(numba) => Some(numba as u64),
            _ => return Err(TauPallasError::InvalidData("Invalid parameter \"created_at\"".to_string())),
        };

        let mut new_task = TaskInfo::new(
            self.workspace.clone(),
            params["title"].get::<String>().unwrap(),
            params["desc"].get::<String>().unwrap(),
            "owner",
            due,
            rank,
            dwow_core::util::time::Timestamp::from_u64(created_at.unwrap()),
        )?;

        new_task.set_project(&projects);
        new_task.set_assign(&assigns);
        new_task.set_tags(&tags);

        new_task.save(&self.dataset_path)?;

        Ok(new_task.ref_id.clone().into())
    }

    /// RPCAPI:
    /// List task ref_ids for the current workspace.
    ///
    /// --> {"jsonrpc": "2.0", "method": "get_ref_ids", "params": [], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": ["ref_id1", ...], "id": 1}
    pub async fn get_ref_ids(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let _params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::get_ref_ids()");

        let tasks = MonthTasks::load_current_tasks(&self.dataset_path, self.workspace.clone(), false)?;

        let task_ref_ids: Vec<JsonValue> =
            tasks.iter().map(|task| JsonValue::String(task.get_ref_id())).collect();

        Ok(JsonValue::Array(task_ref_ids))
    }

    /// RPCAPI:
    /// Get a task by ref_id.
    ///
    /// --> {"jsonrpc": "2.0", "method": "get_task_by_ref_id", "params": [ref_id], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": task, "id": 1}
    pub async fn get_task_by_ref_id(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::get_task_by_ref_id() params {params:?}");

        if params.len() != 1 || !params[0].is_string() {
            return Err(TauPallasError::InvalidData("len of params should be 1".into()))
        }

        let task = self.load_task_by_ref_id(params[0].get::<String>().unwrap())?;
        let task: JsonValue = (&task).into();

        Ok(task)
    }

    /// RPCAPI:
    /// Modify task fields.
    ///
    /// --> {"jsonrpc": "2.0", "method": "modify", "params": [ref_id, {"title": "new"}], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn modify(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::modify() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_object() {
            return Err(TauPallasError::InvalidData("len of params should be 2".into()))
        }

        let task_ref_id = params[0].get::<String>().unwrap();
        let fields = params[1].get::<HashMap<String, JsonValue>>().unwrap();

        let mut task = self.load_task_by_ref_id(task_ref_id)?;

        if let Some(title) = fields.get("title").and_then(|v| v.get::<String>()).filter(|s| !s.is_empty()) {
            task.set_title(title);
            set_event(&mut task, "title", "user", title);
        }

        if let Some(desc) = fields.get("desc").and_then(|v| v.get::<String>()).filter(|s| !s.is_empty()) {
            task.set_desc(desc);
            set_event(&mut task, "desc", "user", desc);
        }

        task.save(&self.dataset_path)?;

        Ok(JsonValue::Boolean(true))
    }

    /// RPCAPI:
    /// Set task state (open, start, pause, stop).
    ///
    /// --> {"jsonrpc": "2.0", "method": "set_state", "params": [ref_id, "start"], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_state(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let states = ["stop", "start", "open", "pause"];

        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::set_state() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return Err(TauPallasError::InvalidData("len of params should be 2".into()))
        }

        let state = params[1].get::<String>().unwrap();

        let mut task = self.load_task_by_ref_id(params[0].get::<String>().unwrap())?;

        if states.contains(&state.as_str()) {
            task.set_state(state);
            set_event(&mut task, "state", "user", state);
        }

        task.save(&self.dataset_path)?;

        Ok(JsonValue::Boolean(true))
    }

    /// RPCAPI:
    /// Set comment on a task.
    ///
    /// --> {"jsonrpc": "2.0", "method": "set_comment", "params": [ref_id, "comment text"], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_comment(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::set_comment() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return Err(TauPallasError::InvalidData("len of params should be 2".into()))
        }

        let ref_id = params[0].get::<String>().unwrap();
        let comment_content = params[1].get::<String>().unwrap();

        let mut task = self.load_task_by_ref_id(ref_id)?;

        task.set_comment(Comment::new(comment_content, "user"));
        set_event(&mut task, "comment", "user", comment_content);

        task.save(&self.dataset_path)?;

        Ok(JsonValue::Boolean(true))
    }

    /// RPCAPI:
    /// Switch workspace.
    ///
    /// --> {"jsonrpc": "2.0", "method": "switch_ws", "params": ["workspace_name"], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn switch_ws(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::switch_ws() params {params:?}");

        if params.len() != 1 || !params[0].is_string() {
            return Err(TauPallasError::InvalidData("Invalid workspace".into()))
        }

        // Note: In a full implementation, this would modify self.workspace
        // For now, just return success
        let _ws = params[0].get::<String>().unwrap();

        Ok(JsonValue::Boolean(true))
    }

    /// RPCAPI:
    /// Get current workspace.
    ///
    /// --> {"jsonrpc": "2.0", "method": "get_ws", "params": [], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": "workspace_name", "id": 1}
    pub async fn get_ws(&self, _params: JsonValue) -> TauPallasResult<JsonValue> {
        Ok(JsonValue::String(self.workspace.clone()))
    }

    // ==================== O-Cap Task Operations ====================

    /// RPCAPI:
    /// Set required capability for a task.
    ///
    /// --> {"jsonrpc": "2.0", "method": "set_task_capability",
    ///      "params": [ref_id, capability_id_bs58, "offchain"], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_task_capability(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::set_task_capability() params {params:?}");

        if params.len() != 3 || !params[0].is_string() || !params[1].is_string() || !params[2].is_string() {
            return Err(TauPallasError::InvalidData("len of params should be 3".into()))
        }

        let ref_id = params[0].get::<String>().unwrap();
        let required_capability_id = params[1].get::<String>().unwrap();
        let verification_mode = params[2].get::<String>().unwrap();

        // Parse capability_id from bs58
        let cap_id_bytes = bs58::decode(required_capability_id).into_vec().map_err(|_| {
            TauPallasError::InvalidData("Invalid bs58 for required_capability_id".into())
        })?;
        let cap_id: [u8; 32] = cap_id_bytes.as_slice().try_into().map_err(|_| {
            TauPallasError::InvalidData("Invalid length for required_capability_id".into())
        })?;

        // Parse verification mode
        let mode = match verification_mode.as_str() {
            "onchain" => VerificationMode::OnChain,
            "offchain" => VerificationMode::OffChain,
            _ => {
                return Err(TauPallasError::InvalidData(
                    "verification_mode should be 'onchain' or 'offchain'".into(),
                ))
            }
        };

        let mut task = self.load_task_by_ref_id(&ref_id)?;

        task.required_capability_id = Some(cap_id);
        task.verification_mode = mode;
        set_event(
            &mut task,
            "capability_required",
            "user",
            &format!("{} ({})", required_capability_id, verification_mode),
        );

        task.save(&self.dataset_path)?;

        Ok(JsonValue::Boolean(true))
    }

    /// RPCAPI:
    /// Claim a task by presenting a capability proof.
    ///
    /// --> {"jsonrpc": "2.0", "method": "claim_task",
    ///      "params": [ref_id, capability_proof_json], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn claim_task(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::claim_task() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_object() {
            return Err(TauPallasError::InvalidData("len of params should be 2".into()))
        }

        let ref_id = params[0].get::<String>().unwrap();
        let proof_json = &params[1];

        let mut task = self.load_task_by_ref_id(&ref_id)?;

        // Check if task requires a capability
        let required_capability_id = match task.required_capability_id {
            Some(id) => id,
            None => {
                return Err(TauPallasError::MissingRequiredCapability(
                    "Task does not require a capability".to_string(),
                )
                .into())
            }
        };

        // Parse the capability proof
        let proof: CapabilityProof = capability::parse_capability_proof(proof_json)?;

        // For off-chain verification
        let verification_result = verify_capability_offchain(&proof, &required_capability_id)?;

        if !verification_result.valid {
            return Ok(JsonValue::Boolean(false))
        }

        // Set the assigned capability (not the worker's identity!)
        task.assigned_capability = Some(proof.capability_id);
        set_event(&mut task, "claimed", "user", "capability verified");

        task.save(&self.dataset_path)?;

        Ok(JsonValue::Boolean(true))
    }

    // ==================== Phase 3: Labor Market Integration ====================

    /// RPCAPI:
    /// Link a tau task to a labor market job for payment.
    ///
    /// --> {"jsonrpc": "2.0", "method": "link_task_to_job",
    ///      "params": [ref_id, labor_job_id_bs58, attestation_id_bs58,
    ///                 payment_token_bs58, payment_amount_u64], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn link_task_to_job(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::link_task_to_job() params {params:?}");

        if params.len() != 5 || !params[0].is_string() {
            return Err(TauPallasError::InvalidData(
                "len of params should be 5 (ref_id, labor_job_id, attestation_id, payment_token, amount)".into()
            ))
        }

        let ref_id = params[0].get::<String>().unwrap();
        let mut task = self.load_task_by_ref_id(&ref_id)?;

        // Parse labor_job_id
        let job_id_str = params[1].get::<String>().unwrap();
        let labor_job_id_bytes = bs58::decode(job_id_str).into_vec().map_err(|_| {
            TauPallasError::InvalidData("Invalid bs58 for labor_job_id".into())
        })?;
        let labor_job_id: [u8; 32] = labor_job_id_bytes.as_slice().try_into().map_err(|_| {
            TauPallasError::InvalidData("Invalid length for labor_job_id".into())
        })?;

        // Parse attestation_id
        let att_id_str = params[2].get::<String>().unwrap();
        let att_id_bytes = bs58::decode(att_id_str).into_vec().map_err(|_| {
            TauPallasError::InvalidData("Invalid bs58 for labor_attestation_id".into())
        })?;
        let labor_attestation_id: [u8; 32] = att_id_bytes.as_slice().try_into().map_err(|_| {
            TauPallasError::InvalidData("Invalid length for labor_attestation_id".into())
        })?;

        // Parse payment_token
        let token_str = params[3].get::<String>().unwrap();
        let token_bytes = bs58::decode(token_str).into_vec().map_err(|_| {
            TauPallasError::InvalidData("Invalid bs58 for payment_token".into())
        })?;
        let payment_token: [u8; 32] = token_bytes.as_slice().try_into().map_err(|_| {
            TauPallasError::InvalidData("Invalid length for payment_token".into())
        })?;

        // Parse payment_amount (as f64 since tinyjson doesn't support u64 directly)
        let payment_amount = match params[4].get::<f64>() {
            Some(f) => *f as u64,
            None => {
                return Err(TauPallasError::InvalidData(
                    "Invalid payment_amount - expected number".into()
                ).into())
            }
        };

        // Update task with labor market info
        task.labor_job_id = Some(labor_job_id);
        task.labor_attestation_id = Some(labor_attestation_id);
        task.payment_token = Some(payment_token);
        task.payment_amount = Some(payment_amount);

        set_event(
            &mut task,
            "linked_to_job",
            "user",
            &format!("job: {}, amount: {}", job_id_str, payment_amount),
        );

        task.save(&self.dataset_path)?;

        Ok(JsonValue::Boolean(true))
    }

    /// RPCAPI:
    /// Submit task deliverable to labor market for payment.
    ///
    /// --> {"jsonrpc": "2.0", "method": "submit_task_deliverable",
    ///      "params": [ref_id, work_proof_json], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": tx_hash, "id": 1}
    pub async fn submit_task_deliverable(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::submit_task_deliverable() params {params:?}");

        if params.len() != 2 || !params[0].is_string() {
            return Err(TauPallasError::InvalidData(
                "len of params should be 2 (ref_id, work_proof)".into()
            ))
        }

        let ref_id = params[0].get::<String>().unwrap();
        let mut task = self.load_task_by_ref_id(&ref_id)?;

        // Verify task has labor market link
        if task.labor_job_id.is_none() {
            return Err(TauPallasError::InvalidData(
                "Task is not linked to a labor market job".into(),
            )
            .into())
        }

        // Verify task is completed (has assigned_capability)
        if task.assigned_capability.is_none() {
            return Err(TauPallasError::InvalidData(
                "Task has not been claimed - cannot submit deliverable".into(),
            )
            .into())
        }

        // Build and broadcast deliverable transaction via darkfid RPC
        if let Some(ref client) = self.rpc_client {
            use crate::labor_market_client;

            // Parse work_proof JSON for deliverable params (bs58-encoded field elements)
            let work_proof = params[1].get::<HashMap<String, JsonValue>>().ok_or_else(|| {
                TauPallasError::InvalidData("Invalid work_proof JSON".into())
            })?;

            // Helper to parse bs58-encoded 32-byte value into pallas::Base
            let parse_bs58_base = |s: &str| -> TauPallasResult<dwow_sdk::pasta::pallas::Base> {
                let bytes = bs58::decode(s).into_vec()
                    .map_err(|_| TauPallasError::InvalidData("Invalid bs58 encoding".into()))?;
                let arr: [u8; 32] = bytes.as_slice().try_into()
                    .map_err(|_| TauPallasError::InvalidData("Invalid length for field element".into()))?;
                Ok(dwow_sdk::pasta::pallas::Base::from_repr(arr)
                    .unwrap_or(dwow_sdk::pasta::pallas::Base::zero()))
            };

            let job_id_str = work_proof.get("job_id")
                .and_then(|v| v.get::<String>())
                .ok_or_else(|| TauPallasError::InvalidData("Missing job_id in work_proof".into()))?;
            let claim_id_str = work_proof.get("claim_id")
                .and_then(|v| v.get::<String>())
                .ok_or_else(|| TauPallasError::InvalidData("Missing claim_id in work_proof".into()))?;
            let attestation_id_str = work_proof.get("attestation_id")
                .and_then(|v| v.get::<String>())
                .ok_or_else(|| TauPallasError::InvalidData("Missing attestation_id in work_proof".into()))?;

            let job_id = parse_bs58_base(job_id_str)?;
            let claim_id = parse_bs58_base(claim_id_str)?;
            let attestation_id = parse_bs58_base(attestation_id_str)?;

            // Build nullifier (prevents double-submission)
            let spent_nullifier = dwow_sdk::crypto::poseidon_hash([job_id, claim_id]);

            // Use zero pubkeys for worker (ZK proof carries the actual key binding)
            let zero = dwow_sdk::pasta::pallas::Base::zero();

            // Build deliverable call data
            let deliverable_calldata = labor_market_client::build_submit_deliverable_calldata(
                &[],
                job_id,
                claim_id,
                zero,
                zero,
                spent_nullifier,
            )?;

            // Build attestation verify claim child call
            let verify_claim_calldata = labor_market_client::build_verify_claim_calldata(
                claim_id,
                attestation_id,
                zero,
                zero,
                zero,
                zero,
            )?;

            // Build identity verify capability child call if task has a capability requirement
            let identity_calldata = if task.required_capability_id.is_some() {
                // Capability proof data should be provided in work_proof
                let cap_id_str = work_proof.get("capability_id")
                    .and_then(|v| v.get::<String>());
                let nullifier_str = work_proof.get("nullifier")
                    .and_then(|v| v.get::<String>());
                let predicate_result = *work_proof.get("predicate_result")
                    .and_then(|v| v.get::<f64>()).unwrap_or(&1.0) as u8;
                let issuer_pub_str = work_proof.get("issuer_pub")
                    .and_then(|v| v.get::<String>());
                let schema_hash_str = work_proof.get("schema_hash")
                    .and_then(|v| v.get::<String>());
                let capability_secret_str = work_proof.get("capability_secret")
                    .and_then(|v| v.get::<String>());

                if let (Some(cap_id), Some(nullifier), Some(issuer_pub), Some(schema_hash), Some(cap_secret)) =
                    (cap_id_str, nullifier_str, issuer_pub_str, schema_hash_str, capability_secret_str)
                {
                    use crate::identity_client::{build_verify_capability_calldata, ClientCapabilityProof};
                    let client_proof = ClientCapabilityProof {
                        capability_id: parse_bs58_base(cap_id)?.to_repr(),
                        nullifier: parse_bs58_base(nullifier)?.to_repr(),
                        predicate_result,
                        issuer_pub: parse_bs58_base(issuer_pub)?.to_repr(),
                        schema_hash: parse_bs58_base(schema_hash)?.to_repr(),
                        proof: vec![],
                        capability_secret: parse_bs58_base(cap_secret)?.to_repr(),
                        created_at: 0,
                    };
                    Some(build_verify_capability_calldata(&client_proof, [0u8; 32], 0)?)
                } else {
                    None
                }
            } else {
                None
            };

            // Build transaction with cross-contract child calls
            // Contract IDs come from the labor_job_id's contract context
            let tx = labor_market_client::build_submit_deliverable_tx(
                [0u8; 32],   // XXX: contract IDs must be configured or fetched from chain
                [0u8; 32],   // attestation contract id
                if identity_calldata.is_some() { Some([0u8; 32]) } else { None },
                deliverable_calldata,
                verify_claim_calldata,
                identity_calldata,
            )?;

            let tx_hash = client.broadcast_tx(&tx).await?;
            let tx_hash_str = tx_hash.to_string();

            set_event(
                &mut task,
                "deliverable_submitted",
                "worker",
                &format!("tx: {}", tx_hash_str),
            );
            task.save(&self.dataset_path)?;

            return Ok(JsonValue::String(tx_hash_str));
        }

        // No RPC client available — record locally only
        set_event(
            &mut task,
            "deliverable_submitted_offline",
            "worker",
            "No darkfid RPC client configured",
        );
        task.save(&self.dataset_path)?;

        Ok(JsonValue::String("deliverable_submitted_offline".to_string()))
    }

    /// RPCAPI:
    /// Register a new capability (PM creates capability for workers).
    ///
    /// --> {"jsonrpc": "2.0", "method": "register_capability",
    ///      "params": [capability_name, credential_requirement_json], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": capability_id_bs58, "id": 1}
    pub async fn register_capability(&self, params: JsonValue) -> TauPallasResult<JsonValue> {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        debug!(target: "tau_pallas", "RpcHandler::register_capability() params {params:?}");

        if params.len() != 2 || !params[0].is_string() || !params[1].is_object() {
            return Err(TauPallasError::InvalidData(
                "len of params should be 2 (capability_name, credential_requirement)".into()
            ))
        }

        let _capability_name = params[0].get::<String>().unwrap();
        let _credential_req = params[1].get::<HashMap<String, JsonValue>>().unwrap();

        // TODO: Implement actual capability registration via identity contract
        // Full implementation would:
        // 1. Create capability definition with name and requirements
        // 2. Register on identity contract via darkfid RPC
        // 3. Return the capability_id

        // Generate a placeholder capability_id from the name
        let capability_id = crate::util::gen_id(32);

        Ok(JsonValue::String(bs58::encode(capability_id).into_string()))
    }

    // ==================== Helper Methods ====================

    fn load_task_by_ref_id(&self, task_ref_id: &str) -> TauPallasResult<TaskInfo> {
        let tasks = MonthTasks::load_current_tasks(&self.dataset_path, self.workspace.clone(), false)?;
        tasks.into_iter().find(|t| t.get_ref_id() == task_ref_id).ok_or(TauPallasError::InvalidId.into())
    }
}