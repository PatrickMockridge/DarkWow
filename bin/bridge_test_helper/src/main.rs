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

//! Bridge Test Helper
//!
//! Generates ZK proofs and submits bridge contract calls via JSON-RPC to a
//! running dwowd node. Used by the test_pipeline.sh `bridge` mode to exercise
//! the full bridge lifecycle: deposit → withdraw → relayer accept → execute.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use dwow_core::{
    rpc::{
        client::RpcClient,
        jsonrpc::JsonRequest,
    },
    util::encoding::base64,
};
use dwow_sdk::{
    blockchain::BlockVersion,
    crypto::{
        pasta_prelude::PrimeField, poseidon_hash, ContractId,
        IntentNullifier, PublicKey, SecretKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use smol::Executor;
use structopt::StructOpt;
use tinyjson::JsonValue;
use url::Url;

use dwow_bridge_contract::model::*;
use dwow_relayer_endowment_contract::model::InitializeParamsV1;
use dwow_contract_test_harness::harness::bridge::BridgeHarness;

// ──────────────────────────────────────────────────────────────────────────────
// Deterministic contract IDs for the bridge test pipeline
// ──────────────────────────────────────────────────────────────────────────────

fn bridge_contract_id() -> ContractId {
    ContractId::from_base(poseidon_hash([
        *dwow_sdk::crypto::contract_id::CONTRACT_ID_PREFIX,
        pallas::Base::zero(),
        pallas::Base::from(10),
    ]))
}

fn relayer_endowment_contract_id() -> ContractId {
    ContractId::from_base(poseidon_hash([
        *dwow_sdk::crypto::contract_id::CONTRACT_ID_PREFIX,
        pallas::Base::zero(),
        pallas::Base::from(11),
    ]))
}

// ──────────────────────────────────────────────────────────────────────────────
// CLI
// ──────────────────────────────────────────────────────────────────────────────

/// Bridge test helper — generates ZK proofs and submits bridge contract calls.
#[derive(StructOpt)]
enum Command {
    /// Generate a test keypair for the relayer
    GenerateKeypair,

    /// Deploy bridge and relayer_endowment WASM contracts via contract.deploy RPC
    DeployBridge {
        /// Path to bridge WASM file
        #[structopt(long)]
        bridge_wasm: PathBuf,
        /// Path to relayer_endowment WASM file
        #[structopt(long)]
        endowment_wasm: PathBuf,
    },

    /// Initialize the bridge contract (InitializeV1, opcode 0x00)
    InitBridge,

    /// Initialize the relayer endowment account
    InitEndowment {
        /// Relayer public key (hex-encoded, 32 bytes)
        #[structopt(long)]
        relayer_pub: String,
    },

    /// Register a relayer with the bridge (RegisterRelayerV1, opcode 0x0a)
    RegisterRelayer {
        /// Relayer public key (hex-encoded, 32 bytes)
        #[structopt(long)]
        relayer_pub: String,
    },

    /// Simulate a deposit with a real ZK proof (DepositV1, opcode 0x01)
    SimulateDeposit {
        /// Deposit secret (hex-encoded pallas::Base, 32 bytes)
        #[structopt(long)]
        secret: String,
        /// Deposit amount in smallest unit
        #[structopt(long)]
        amount: u64,
        /// Recipient public key (hex-encoded, 32 bytes)
        #[structopt(long)]
        recipient_pub: String,
        /// External chain: ethereum, monero, zcash, aztec, litecoin
        #[structopt(long, default_value = "ethereum")]
        chain: String,
    },

    /// Simulate a withdrawal with a real ZK proof (WithdrawV1, opcode 0x02)
    SimulateWithdraw {
        /// Withdraw secret (hex-encoded pallas::Base, 32 bytes)
        #[structopt(long)]
        secret: String,
        /// Withdraw amount in smallest unit
        #[structopt(long)]
        amount: u64,
    },

    /// Accept a pending withdrawal as a relayer (AcceptWithdrawalV1, opcode 0x0b)
    AcceptWithdrawal {
        /// Withdrawal nullifier (hex-encoded, 32 bytes)
        #[structopt(long)]
        nullifier: String,
        /// Relayer public key (hex-encoded, 32 bytes)
        #[structopt(long)]
        relayer_pub: String,
        /// Max fee in basis points
        #[structopt(long, default_value = "500")]
        max_fee_bp: u64,
    },

    /// Execute a guaranteed withdrawal (ExecuteGuaranteedWithdrawV1, opcode 0x05)
    ExecuteWithdrawal {
        /// Withdrawal nullifier (hex-encoded, 32 bytes)
        #[structopt(long)]
        nullifier: String,
    },
}

#[derive(StructOpt)]
struct Opt {
    /// JSON-RPC endpoint URL (e.g. tcp://127.0.0.1:31345)
    #[structopt(long, default_value = "tcp://127.0.0.1:31345")]
    url: String,

    /// Block time in seconds (for polling inclusion)
    #[structopt(long, default_value = "120")]
    block_time: u64,

    /// Total timeout in seconds for waiting on block inclusion
    #[structopt(long, default_value = "300")]
    timeout: u64,

    #[structopt(subcommand)]
    command: Command,
}

// ──────────────────────────────────────────────────────────────────────────────
// RPC helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Parse a hex string to a 32-byte array.
fn hex_to_bytes32(hex: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex).context("Invalid hex")?;
    if bytes.len() != 32 {
        return Err(anyhow!("Expected 32 bytes, got {}", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Parse a chain name to an ExternalChain variant.
fn parse_chain(name: &str) -> Result<ExternalChain> {
    match name.to_lowercase().as_str() {
        "ethereum" => Ok(ExternalChain::Ethereum),
        "monero" => Ok(ExternalChain::Monero),
        "zcash" => Ok(ExternalChain::Zcash),
        "aztec" => Ok(ExternalChain::Aztec),
        "litecoin" => Ok(ExternalChain::Litecoin),
        _ => Err(anyhow!(
            "Unknown chain '{}'. Valid: ethereum, monero, zcash, aztec, litecoin",
            name
        )),
    }
}

/// Parse a hex string to a pallas::Base field element.
fn hex_to_pallas(hex: &str) -> Result<pallas::Base> {
    let bytes = hex_to_bytes32(hex)?;
    pallas::Base::from_repr(bytes)
        .into_option()
        .ok_or_else(|| anyhow!("Invalid pallas::Base field element"))
}

/// Connect to the JSON-RPC endpoint.
async fn connect(url: &Url, ex: Arc<Executor<'_>>) -> Result<RpcClient> {
    RpcClient::new(url.clone(), ex)
        .await
        .context("Failed to connect to RPC endpoint")
}

/// Get current block height via blockchain.last_confirmed_block RPC.
async fn get_block_height(client: &RpcClient) -> Result<u64> {
    let req = JsonRequest::new("blockchain.last_confirmed_block", JsonValue::Array(vec![]));
    let result = client.request(req).await?;

    if let Some(height) = result.get::<f64>() {
        Ok(*height as u64)
    } else if let Some(_b) = result.get::<bool>() {
        Ok(0)
    } else {
        Err(anyhow!("Unexpected response format for last_confirmed_block: {:?}", result))
    }
}

/// Deploy a WASM contract via contract.deploy RPC.
async fn deploy_contract_rpc(
    client: &RpcClient,
    wasm: &[u8],
    contract_id: &ContractId,
) -> Result<()> {
    let wasm_b64 = base64::encode(wasm);
    let contract_id_str = contract_id.to_string();

    let params = JsonValue::from(std::collections::HashMap::from([
        ("wasm".to_string(), JsonValue::String(wasm_b64)),
        ("contract_id".to_string(), JsonValue::String(contract_id_str)),
    ]));

    let req = JsonRequest::new("contract.deploy", params);
    let result = client.request(req).await?;

    let result_obj = result
        .get::<std::collections::HashMap<String, JsonValue>>()
        .ok_or_else(|| anyhow!("Unexpected deploy response: {:?}", result))?;

    let status = result_obj
        .get("status")
        .and_then(|v| v.get::<String>())
        .ok_or_else(|| anyhow!("Missing status in deploy response"))?;

    if status != "deployed" {
        return Err(anyhow!("Deploy failed with status: {}", status));
    }

    Ok(())
}

/// Build a linear transaction with a single contract call and submit via
/// tx.submit_linear RPC. Waits for block inclusion by polling block height.
async fn submit_contract_call(
    client: &RpcClient,
    contract_id: &ContractId,
    call_data: Vec<u8>,
    block_time: u64,
    timeout: u64,
) -> Result<String> {
let contract_call = dwow_chain::ContractCall {
        contract_id: *contract_id,
        data: call_data,
    };

    let tx = dwow_chain::Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![contract_call],
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    };

    // Serialize to JSON and base64-encode
    let tx_json = serde_json::to_vec(&tx).context("Failed to serialize transaction")?;
    let tx_b64 = base64::encode(&tx_json);

    // Submit to linear mempool
    let params = JsonValue::Array(vec![JsonValue::String(tx_b64)]);
    let req = JsonRequest::new("tx.submit_linear", params);
    let result = client.request(req).await?;

    let tx_hash = result
        .get::<String>()
        .ok_or_else(|| anyhow!("Unexpected submit_linear response: {:?}", result))?
        .clone();

    eprintln!("Transaction submitted: {}", tx_hash);

    // Wait for block inclusion by polling block height
    let start = std::time::Instant::now();
    let last_height = get_block_height(client).await.unwrap_or(0);

    while start.elapsed().as_secs() < timeout {
        smol::Timer::after(Duration::from_secs(block_time)).await;

        match get_block_height(client).await {
            Ok(height) if height > last_height => {
                eprintln!("New block confirmed at height {}", height);
                return Ok(tx_hash);
            }
            Ok(_) => {} // same height, keep waiting
            Err(e) => eprintln!("Warning: failed to get block height: {}", e),
        }
    }

    Err(anyhow!("Timeout waiting for block inclusion after {} seconds", timeout))
}

// ──────────────────────────────────────────────────────────────────────────────
// Command implementations
// ──────────────────────────────────────────────────────────────────────────────

async fn cmd_generate_keypair() -> Result<()> {
    let secret = SecretKey::random(&mut OsRng);
    let secret_repr = hex::encode(secret.inner().to_repr());
    let public = PublicKey::from_secret(secret);

    println!("public_key:  {}", hex::encode(public.to_bytes()));
    println!("secret_key:  {}", secret_repr);

    Ok(())
}

async fn cmd_deploy_bridge(
    client: &RpcClient,
    bridge_wasm: &PathBuf,
    endowment_wasm: &PathBuf,
) -> Result<()> {
    let bridge_wasm_bytes =
        std::fs::read(bridge_wasm).context("Failed to read bridge WASM")?;
    let endowment_wasm_bytes =
        std::fs::read(endowment_wasm).context("Failed to read endowment WASM")?;

    let bridge_id = bridge_contract_id();
    let endowment_id = relayer_endowment_contract_id();

    eprintln!("Deploying bridge contract: {}", bridge_id);
    deploy_contract_rpc(client, &bridge_wasm_bytes, &bridge_id).await?;
    eprintln!("Bridge contract deployed: {}", bridge_id);

    eprintln!("Deploying relayer_endowment contract: {}", endowment_id);
    deploy_contract_rpc(client, &endowment_wasm_bytes, &endowment_id).await?;
    eprintln!("Relayer endowment contract deployed: {}", endowment_id);

    // Output contract IDs for the pipeline to capture
    println!("bridge_contract_id: {}", bridge_id);
    println!("endowment_contract_id: {}", endowment_id);

    Ok(())
}

async fn cmd_init_bridge(
    client: &RpcClient,
    block_time: u64,
    timeout: u64,
) -> Result<()> {
    let bridge_id = bridge_contract_id();
    // InitializeV1 takes no params — just the opcode byte
    let call_data = vec![0x00u8];

    eprintln!("Initializing bridge contract...");
    submit_contract_call(client, &bridge_id, call_data, block_time, timeout).await?;
    eprintln!("Bridge initialized");

    Ok(())
}

async fn cmd_init_endowment(
    client: &RpcClient,
    relayer_pub_hex: &str,
    block_time: u64,
    timeout: u64,
) -> Result<()> {
    let endowment_id = relayer_endowment_contract_id();
    let pub_bytes = hex_to_bytes32(relayer_pub_hex)?;
    let signature_public = PublicKey::from_bytes(pub_bytes)
        .map_err(|_| anyhow!("Invalid public key"))?;

    let params = InitializeParamsV1 {
        instance_seed: [0u8; 32],
        default_backer_cut_bp: 2000, // 20%
        signature_public,
    };

    let mut call_data = vec![0x00u8]; // InitializeV1
    call_data.extend(params.encode());

    eprintln!("Initializing relayer endowment...");
    submit_contract_call(client, &endowment_id, call_data, block_time, timeout).await?;
    eprintln!("Relayer endowment initialized");

    Ok(())
}

async fn cmd_register_relayer(
    client: &RpcClient,
    relayer_pub_hex: &str,
    block_time: u64,
    timeout: u64,
) -> Result<()> {
    let bridge_id = bridge_contract_id();
    let relayer_pub = PublicKey::from_bytes(hex_to_bytes32(relayer_pub_hex)?)
        .map_err(|e| anyhow!("Invalid relayer pubkey: {e:?}"))?;

    let params = RegisterRelayerParams { relayer_pub };

    let mut call_data = vec![0x0au8]; // RegisterRelayerV1
    call_data.extend(params.encode());

    eprintln!("Registering relayer...");
    submit_contract_call(client, &bridge_id, call_data, block_time, timeout).await?;
    eprintln!("Relayer registered");

    Ok(())
}

async fn cmd_simulate_deposit(
    client: &RpcClient,
    secret_hex: &str,
    amount: u64,
    recipient_pub_hex: &str,
    chain: ExternalChain,
    block_time: u64,
    timeout: u64,
) -> Result<()> {
    let bridge_id = bridge_contract_id();
    let secret = hex_to_pallas(secret_hex)?;
    let recipient_bytes = hex_to_bytes32(recipient_pub_hex)?;
    let recipient_public = PublicKey::from_bytes(recipient_bytes)
        .map_err(|_| anyhow!("Invalid recipient public key"))?;

    // Dummy external chain data for the test
    let bridge_nonce = 0u64;
    let external_block_hash = pallas::Base::from(0xdead);
    let merkle_root = pallas::Base::from(0xbeef);

    let leaf = dwow_sdk::crypto::MerkleNode::from_bytes(merkle_root.to_repr())
        .ok_or_else(|| anyhow!("Invalid merkle node"))?;
    let merkle_path = vec![leaf];

    let harness = BridgeHarness::spawn();
    let result = harness
        .deposit(
            secret,
            amount,
            recipient_public,
            bridge_nonce,
            external_block_hash,
            merkle_root,
            0, // leaf_pos
            merkle_path,
            chain,
            0, // fee
        )
        .map_err(|e| anyhow!("Failed to generate deposit proof: {e}"))?;

    eprintln!("Deposit ZK proof generated");
    eprintln!(
        "Commitment: {}",
        hex::encode(result.public_inputs.commitment.to_repr())
    );

    // call_data already has opcode 0x01 prepended by BridgeHarness
    submit_contract_call(client, &bridge_id, result.call_data, block_time, timeout).await?;
    eprintln!("Deposit submitted");

    // Output commitment for pipeline tracking
    println!(
        "commitment: {}",
        hex::encode(result.public_inputs.commitment.to_repr())
    );

    Ok(())
}

async fn cmd_simulate_withdraw(
    client: &RpcClient,
    secret_hex: &str,
    amount: u64,
    block_time: u64,
    timeout: u64,
) -> Result<()> {
    let bridge_id = bridge_contract_id();
    let secret = hex_to_pallas(secret_hex)?;

    // Derive bridge_address: bridge_pub = secret * G, address = poseidon(bridge_pub.x, 0)
    let bridge_secret = SecretKey::from_bytes(secret.to_repr())
        .map_err(|_| anyhow!("Failed to create secret key"))?;
    let bridge_pub = PublicKey::from_secret(bridge_secret);
    let (bx, _by) = bridge_pub.xy().expect("pk not identity");
    let bridge_address = poseidon_hash([bx, pallas::Base::zero()]);

    // Test merkle values
    let recipient_hash = pallas::Base::from(0xcafe);
    let merkle_root = pallas::Base::from(0xbeef);
    let merkle_proof = [
        pallas::Base::from(1),
        pallas::Base::from(2),
        pallas::Base::from(3),
        pallas::Base::from(4),
    ];

    let harness = BridgeHarness::spawn();
    let result = harness
        .withdraw(
            secret,
            amount,
            recipient_hash,
            bridge_address,
            merkle_root,
            merkle_proof,
            0, // leaf_index
            0, // fee
            0, // token_minimum
        )
        .map_err(|e| anyhow!("Failed to generate withdraw proof: {e}"))?;

    eprintln!("Withdraw ZK proof generated");
    eprintln!(
        "Nullifier: {}",
        hex::encode(result.public_inputs.nullifier.to_repr())
    );

    // call_data already has opcode 0x02 prepended by BridgeHarness
    submit_contract_call(client, &bridge_id, result.call_data, block_time, timeout).await?;
    eprintln!("Withdrawal submitted");

    // Output nullifier for later use in accept/execute
    println!(
        "nullifier: {}",
        hex::encode(result.public_inputs.nullifier.to_repr())
    );

    Ok(())
}

async fn cmd_accept_withdrawal(
    client: &RpcClient,
    nullifier_hex: &str,
    relayer_pub_hex: &str,
    max_fee_bp: u64,
    block_time: u64,
    timeout: u64,
) -> Result<()> {
    let bridge_id = bridge_contract_id();
    let nullifier_bytes = hex_to_bytes32(nullifier_hex)?;
    let nullifier = IntentNullifier::from_bytes(nullifier_bytes)
        .map_err(|e| anyhow!("Invalid nullifier: {e:?}"))?;
    let relayer_pub = PublicKey::from_bytes(hex_to_bytes32(relayer_pub_hex)?)
        .map_err(|e| anyhow!("Invalid relayer pubkey: {e:?}"))?;

    let params = AcceptWithdrawalParams {
        nullifier,
        relayer_pub,
        max_fee_bp,
    };

    let mut call_data = vec![0x0bu8]; // AcceptWithdrawalV1
    call_data.extend(params.encode());

    eprintln!("Accepting withdrawal...");
    submit_contract_call(client, &bridge_id, call_data, block_time, timeout).await?;
    eprintln!("Withdrawal accepted");

    Ok(())
}

async fn cmd_execute_withdrawal(
    client: &RpcClient,
    nullifier_hex: &str,
    block_time: u64,
    timeout: u64,
) -> Result<()> {
    let bridge_id = bridge_contract_id();
    let nullifier_bytes = hex_to_bytes32(nullifier_hex)?;
    let nullifier = IntentNullifier::from_bytes(nullifier_bytes)
        .map_err(|e| anyhow!("Invalid nullifier: {e:?}"))?;

    // For the test helper, pool_stake_proof and relayer_sig are empty stubs
    let params = ExecuteGuaranteedWithdrawParams {
        nullifier,
        pool_stake_proof: vec![],
        relayer_sig: vec![],
        execution_data: b"bridge_test_helper".to_vec(),
    };

    let mut call_data = vec![0x05u8]; // ExecuteGuaranteedWithdrawV1
    call_data.extend(params.encode());

    eprintln!("Executing withdrawal...");
    submit_contract_call(client, &bridge_id, call_data, block_time, timeout).await?;
    eprintln!("Withdrawal executed");

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Main
// ──────────────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let ex = Arc::new(Executor::new());

    smol::block_on(ex.run(async {
        match &opt.command {
            Command::GenerateKeypair => {
                cmd_generate_keypair().await?;
            }
            _ => {
                let url: Url = opt.url.parse().context("Invalid URL")?;
                let client = connect(&url, ex.clone()).await?;

                match &opt.command {
                    Command::DeployBridge { bridge_wasm, endowment_wasm } => {
                        cmd_deploy_bridge(&client, bridge_wasm, endowment_wasm).await?;
                    }
                    Command::InitBridge => {
                        cmd_init_bridge(&client, opt.block_time, opt.timeout).await?;
                    }
                    Command::InitEndowment { relayer_pub } => {
                        cmd_init_endowment(&client, relayer_pub, opt.block_time, opt.timeout)
                            .await?;
                    }
                    Command::RegisterRelayer { relayer_pub } => {
                        cmd_register_relayer(&client, relayer_pub, opt.block_time, opt.timeout)
                            .await?;
                    }
                    Command::SimulateDeposit { secret, amount, recipient_pub, chain } => {
                        let chain = parse_chain(chain)?;
                        cmd_simulate_deposit(
                            &client, secret, *amount, recipient_pub, chain, opt.block_time, opt.timeout,
                        )
                        .await?;
                    }
                    Command::SimulateWithdraw { secret, amount } => {
                        cmd_simulate_withdraw(&client, secret, *amount, opt.block_time, opt.timeout)
                            .await?;
                    }
                    Command::AcceptWithdrawal { nullifier, relayer_pub, max_fee_bp } => {
                        cmd_accept_withdrawal(
                            &client, nullifier, relayer_pub, *max_fee_bp, opt.block_time,
                            opt.timeout,
                        )
                        .await?;
                    }
                    Command::ExecuteWithdrawal { nullifier } => {
                        cmd_execute_withdrawal(&client, nullifier, opt.block_time, opt.timeout)
                            .await?;
                    }
                    _ => unreachable!(),
                }
            }
        }

        Ok(())
    }))
}
