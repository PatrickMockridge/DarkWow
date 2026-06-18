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

//! Promissory Note Client API
//!
//! This module implements the client-side API for Promissory Note contract interaction.
//!
//! Key design: Value commitments use Pedersen (additively homomorphic)
//! for cross-proof value conservation in the entrypoint.

use dwow_sdk::{
    crypto::{
        keypair::SecretKey,
        pasta_prelude::PrimeField,
        pedersen_commitment_u64, poseidon_hash, BaseBlind, Blind, MerkleNode, PublicKey,
    },
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

use crate::model::{Coin, Output};

/// ZK circuit binary constants
pub mod zkbins;

/// `PromissoryNote::TokenMintV1` API - create new token type
pub mod token_mint_v1;

/// `PromissoryNote::RedeemV1` API - redeem coin, create receipt
pub mod redeem_v1;

/// `PromissoryNote::MintV1` API - mint tokens of existing type
pub mod mint_v1;

/// `PromissoryNote::BurnV1` API
pub mod burn_v1;

/// `PromissoryNote::TransferV1` API
pub mod transfer_v1;

/// PromissoryNote holds the inner attributes of a Coin.
///
/// Note that value_blind is pallas::Scalar (Pedersen blinding), not pallas::Base.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct PromissoryNote {
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: pallas::Base,
    /// Spend hook used for protocol-owned liquidity
    pub spend_hook: pallas::Base,
    /// User data used by protocol when spend hook is enabled
    pub user_data: pallas::Base,
    /// Blinding factor for the coin
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value (Pedersen commitment)
    pub value_blind: pallas::Scalar,
    /// Blinding factor for the token ID
    pub token_blind: pallas::Base,
    /// Attached memo (arbitrary data)
    pub memo: Vec<u8>,
}

/// Verify a received coin by decrypting the AEAD note and checking all commitments.
///
/// This is the recipient-side verification path: given an Output from a TransferV1
/// or OtcSwapV1 transaction, the recipient uses their `SecretKey` to:
///
/// 1. **Decrypt** the AEAD note (only the intended recipient can do this — the
///    Diffie-Hellman shared secret requires the recipient's secret key).
/// 2. **Verify the coin commitment** matches the decrypted attributes.
/// 3. **Verify the value commitment** matches the decrypted value and blind.
///
/// On success, returns the verified `PromissoryNote` with all coin attributes.
/// On failure (wrong recipient, corrupted data, mismatched commitments), returns an error.
pub fn verify_received_capability(output: &Output, secret: &SecretKey) -> Result<PromissoryNote, dwow_sdk::error::ContractError> {
    // 1. Decrypt the AEAD note. Only the intended recipient can do this —
    //    the AEAD encryption uses Diffie-Hellman with the recipient's public key.
    let note: PromissoryNote = output.note.decrypt(secret)?;

    // 2. Derive the recipient's address (field element) from their public key.
    //    The coin commitment uses poseidon_hash([public_key_x]) as the "public_key" field,
    //    not the EC point itself — promissory_note keeps public keys as Poseidon-derived elements
    //    for ZK circuit simplicity.
    let recipient_pub = PublicKey::from_secret(*secret);
    let recipient_address = poseidon_hash([recipient_pub.x()]);

    // 3. Verify coin commitment matches the decrypted attributes.
    //    This proves the coin was correctly formed and the note wasn't tampered with.
    let expected_coin = Coin::from_attributes(
        recipient_address,
        note.value,
        note.token_id,
        note.spend_hook,
        note.user_data,
        note.coin_blind,
    );
    if expected_coin != output.coin {
        return Err(dwow_sdk::error::ContractError::Custom(
            crate::error::PromissoryNoteError::ValueMismatch as u32,
        ));
    }

    // 4. Verify Pedersen value_commit matches the decrypted value and blind.
    let value_blind = Blind(note.value_blind);
    let expected_value_commit = pedersen_commitment_u64(note.value, value_blind);
    if expected_value_commit != output.value_commit {
        return Err(dwow_sdk::error::ContractError::Custom(
            crate::error::PromissoryNoteError::ValueMismatch as u32,
        ));
    }

    Ok(note)
}

use dwow_sdk::contract_client::{ContractClient, WalletStateProvider};

/// PromissoryNote contract client — implements ContractClient for the wallet's
/// generic dispatch. Lives in the contract crate, NOT the wallet.
///
/// PN is the first genesis capability contract. Architecturally it is identical
/// to all other capability contracts — the wallet treats it through the same
/// generic ContractClient path. Its strong mutual dependencies (25+ DeFi
/// contracts depend on it) do not grant it special wallet-side treatment.
pub struct PromissoryNoteClient;

impl ContractClient for PromissoryNoteClient {
    fn contract_name(&self) -> &'static str { "promissory_note" }

    fn function_selector(&self, function: &str) -> Option<u8> {
        match function {
            "TokenMintV1" => Some(0x00),
            "RedeemV1" => Some(0x01),
            "MintV1" => Some(0x02),
            "BurnV1" => Some(0x03),
            "TransferV1" => Some(0x04),
            "OtcSwapV1" => Some(0x05),
            _ => None,
        }
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        vec!["TokenMintV1", "RedeemV1", "MintV1", "BurnV1", "TransferV1", "OtcSwapV1"]
    }

    fn detect_transferred(
        &self,
        call_data: &[u8],
        held_capabilities: &[dwow_sdk::contract_client::CapabilityInfo],
    ) -> Vec<String> {
        use dwow_sdk::crypto::poseidon_hash;
        use dwow_serial::deserialize_partial;
        let mut transferred = vec![];

        // Decode TransferParamsV1 — BurnV1 (0x03) and TransferV1 (0x04) both
        // use TransferParamsV1 for their input structure.
        let params = match deserialize_partial::<crate::model::TransferParamsV1>(call_data) {
            Ok((p, _)) => p,
            Err(_) => return transferred,
        };

        for input in &params.inputs {
            for cap in held_capabilities {
                // Match signature_public = poseidon_hash(secret)
                if let Ok(secret_bytes) = bs58::decode(&cap.secret).into_vec() {
                    if secret_bytes.len() == 32 {
                        let arr: [u8; 32] = match secret_bytes.try_into() {
                            Ok(a) => a,
                            Err(_) => continue,
                        };
                        if let Ok(secret) = dwow_sdk::crypto::keypair::SecretKey::from_bytes(arr) {
                            if poseidon_hash([secret.inner()]) == input.signature_public {
                                transferred.push(cap.capability_id.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }
        transferred
    }

    fn build(&self, function: &str, params: &str, wallet_state: &dyn WalletStateProvider) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        match function {
            "TransferV1" => Self::build_transfer_from_state(params, wallet_state),
            "BurnV1" => Self::build_burn_from_state(params, wallet_state),
            "RedeemV1" => Self::build_redeem_from_state(params, wallet_state),
            "TokenMintV1" => Self::build_token_mint_from_state(params, wallet_state),
            "MintV1" => Self::build_mint_from_state(params, wallet_state),
            "OtcSwapV1" => Self::build_otc_swap_from_state(params, wallet_state),
            _ => Err(format!("PromissoryNote: unsupported function '{}'", function)),
        }
    }
}

// ============================================================================
// JSON-driven dispatch helpers — parse manifest params, query wallet state,
// construct typed inputs, delegate to concrete builder methods.
// ============================================================================

/// Decode a bs58 string to a pallas::Base field element.
fn decode_bs58_field(s: &str) -> std::result::Result<pallas::Base, String> {
    let bytes = bs58::decode(s).into_vec()
        .map_err(|e| format!("bs58 decode: {}", e))?;
    let len = bytes.len();
    let arr: [u8; 32] = bytes.try_into()
        .map_err(|_| format!("Invalid field length: {}", len))?;
    pallas::Base::from_repr(arr)
        .into_option()
        .ok_or_else(|| "Invalid field element".to_string())
}

/// Decode a bs58 string to SecretKey.
fn decode_bs58_secret(s: &str) -> std::result::Result<SecretKey, String> {
    let bytes = bs58::decode(s).into_vec()
        .map_err(|e| format!("bs58 decode: {}", e))?;
    let len = bytes.len();
    let arr: [u8; 32] = bytes.try_into()
        .map_err(|_| format!("Invalid secret length: {}", len))?;
    SecretKey::from_bytes(arr)
        .map_err(|_| "Invalid secret key".to_string())
}

/// Decode a Merkle proof (bs58 siblings) to Vec<MerkleNode>.
fn decode_merkle_path(siblings: &[String]) -> std::result::Result<Vec<MerkleNode>, String> {
    siblings.iter().map(|s| {
        let bytes: [u8; 32] = bs58::decode(s).into_vec()
            .map_err(|e| format!("bs58: {}", e))?
            .try_into()
            .map_err(|_| "Invalid Merkle node length".to_string())?;
        MerkleNode::from_bytes(bytes)
            .ok_or_else(|| "Invalid Merkle node".to_string())
    }).collect()
}

/// Decode a JSON u64 value, supporting both integer and string-encoded amounts.
fn parse_json_u64(v: &serde_json::Value, key: &str) -> std::result::Result<u64, String> {
    match &v[key] {
        serde_json::Value::Number(n) => n.as_u64()
            .ok_or_else(|| format!("{}: not a valid u64", key)),
        serde_json::Value::String(s) => s.parse::<u64>()
            .map_err(|_| format!("{}: not a valid u64 string", key)),
        _ => Err(format!("{}: missing or wrong type", key)),
    }
}

fn parse_json_string<'a>(v: &'a serde_json::Value, key: &str) -> std::result::Result<&'a str, String> {
    v[key].as_str()
        .ok_or_else(|| format!("{}: missing or not a string", key))
}

fn parse_json_optional_string(v: &serde_json::Value, key: &str) -> Option<String> {
    v[key].as_str().map(|s| s.to_string())
}

impl PromissoryNoteClient {
    /// Build TransferV1 from JSON params + wallet state.
    fn build_transfer_from_state(
        params_json: &str,
        wallet_state: &dyn WalletStateProvider,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let v: serde_json::Value = serde_json::from_str(params_json)
            .map_err(|e| format!("invalid JSON: {}", e))?;

        let token_id_str = parse_json_string(&v, "token_id")?;
        let recipient_str = parse_json_string(&v, "recipient")?;
        let amount = parse_json_u64(&v, "amount")?;
        let spend_hook_str = parse_json_optional_string(&v, "spend_hook");
        let user_data_str = parse_json_optional_string(&v, "user_data");

        // Select a coin with sufficient value
        let coins = wallet_state.held_capabilities_for_token(token_id_str)?;
        let coin = coins.iter()
            .find(|c| c.value >= amount)
            .ok_or_else(|| format!("No unspent coin with value >= {}", amount))?;

        let proof = wallet_state.get_merkle_proof(&coin.cap_id)?;
        let secret = decode_bs58_secret(&coin.secret)?;
        let cap_blind_val = decode_bs58_field(&coin.cap_blind)?;
        let token_id = decode_bs58_field(&coin.token_id)?;
        let spend_hook = spend_hook_str
            .map(|s| decode_bs58_field(&s)).transpose()?
            .unwrap_or(pallas::Base::zero());
        let user_data = user_data_str
            .map(|s| decode_bs58_field(&s)).transpose()?
            .unwrap_or(pallas::Base::zero());
        let merkle_path = decode_merkle_path(&proof.siblings)?;

        // Parse recipient public key
        let recipient_bytes = bs58::decode(recipient_str).into_vec()
            .map_err(|e| format!("recipient bs58: {}", e))?;
        let recipient_pub = PublicKey::from_bytes(
            recipient_bytes.try_into()
                .map_err(|_| "Invalid recipient pubkey length".to_string())?
        ).map_err(|_| "Invalid recipient public key".to_string())?;

        let input = transfer_v1::TransferCallInput {
            value: coin.value,
            token_id,
            spend_hook,
            user_data,
            coin_blind: cap_blind_val,
            leaf_position: proof.leaf_position,
            merkle_path,
            secret: secret.inner(),
            ephemeral_signature_secret: SecretKey::random(&mut rand::rngs::OsRng).inner(),
        };

        // Build change output
        let change_blind = BaseBlind::random(&mut rand::rngs::OsRng);
        let change_output = transfer_v1::TransferCallOutput {
            recipient: poseidon_hash([recipient_pub.x()]),
            recipient_pub: recipient_pub.clone(),
            value: coin.value - amount,
            token_id,
            spend_hook,
            user_data,
            coin_blind: change_blind.inner(),
        };

        let output = transfer_v1::TransferCallOutput {
            recipient: poseidon_hash([recipient_pub.x()]),
            recipient_pub,
            value: amount,
            token_id,
            spend_hook,
            user_data,
            coin_blind: BaseBlind::random(&mut rand::rngs::OsRng).inner(),
        };

        smol::block_on(Self::build_transfer(
            vec![input],
            vec![output, change_output],
        ))
    }

    /// Build BurnV1 from JSON params + wallet state.
    fn build_burn_from_state(
        params_json: &str,
        wallet_state: &dyn WalletStateProvider,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let v: serde_json::Value = serde_json::from_str(params_json)
            .map_err(|e| format!("invalid JSON: {}", e))?;

        let coin_ids: Vec<String> = v["coin_ids"]
            .as_array()
            .ok_or_else(|| "coin_ids: missing or not an array".to_string())?
            .iter()
            .map(|v| v.as_str().map(|s| s.to_string())
                 .ok_or_else(|| "coin_ids: array element not a string".to_string()))
            .collect::<std::result::Result<_, _>>()?;

        let all_coins = wallet_state.held_capabilities_for_token("")?;  // all tokens
        let mut inputs = vec![];
        for coin_id in &coin_ids {
            let coin = all_coins.iter()
                .find(|c| &c.cap_id == coin_id)
                .ok_or_else(|| format!("Coin not found: {}", coin_id))?;
            let proof = wallet_state.get_merkle_proof(&coin.cap_id)?;
            let secret = decode_bs58_secret(&coin.secret)?;
            let coin_blind = decode_bs58_field(&coin.cap_blind)?;
            let token_id = decode_bs58_field(&coin.token_id)?;
            let spend_hook = coin.spend_hook.as_ref()
                .map(|s| decode_bs58_field(&s)).transpose()?
                .unwrap_or(pallas::Base::zero());
            let user_data = coin.user_data.as_ref()
                .map(|s| decode_bs58_field(&s)).transpose()?
                .unwrap_or(pallas::Base::zero());
            let merkle_path = decode_merkle_path(&proof.siblings)?;

            inputs.push(burn_v1::BurnCallInput {
                value: coin.value,
                token_id,
                spend_hook,
                user_data,
                coin_blind,
                leaf_position: proof.leaf_position,
                merkle_path,
                secret: secret.inner(),
                ephemeral_signature_secret: SecretKey::random(&mut rand::rngs::OsRng).inner(),
            });
        }

        smol::block_on(Self::build_burn(inputs))
    }

    /// Build RedeemV1 from JSON params + wallet state.
    fn build_redeem_from_state(
        params_json: &str,
        wallet_state: &dyn WalletStateProvider,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let v: serde_json::Value = serde_json::from_str(params_json)
            .map_err(|e| format!("invalid JSON: {}", e))?;

        let coin_id = parse_json_string(&v, "coin_id")?.to_string();
        let spend_hook_str = parse_json_optional_string(&v, "spend_hook");

        let all_coins = wallet_state.held_capabilities_for_token("")?;
        let coin = all_coins.iter()
            .find(|c| c.cap_id == coin_id)
            .ok_or_else(|| format!("Coin not found: {}", coin_id))?;

        let proof = wallet_state.get_merkle_proof(&coin.cap_id)?;
        let secret = decode_bs58_secret(&coin.secret)?;
        let coin_blind = decode_bs58_field(&coin.cap_blind)?;
        let token_id = decode_bs58_field(&coin.token_id)?;
        let spend_hook_in = coin.spend_hook.as_ref()
            .map(|s| decode_bs58_field(&s)).transpose()?
            .unwrap_or(pallas::Base::zero());
        let user_data_in = coin.user_data.as_ref()
            .map(|s| decode_bs58_field(&s)).transpose()?
            .unwrap_or(pallas::Base::zero());
        let merkle_path = decode_merkle_path(&proof.siblings)?;
        let spend_hook_out = spend_hook_str
            .map(|s| decode_bs58_field(&s)).transpose()?
            .unwrap_or(spend_hook_in);

        let input = redeem_v1::RedeemCallInput {
            value: coin.value,
            token_id,
            spend_hook: spend_hook_in,
            user_data: user_data_in,
            coin_blind,
            leaf_position: proof.leaf_position,
            merkle_path,
            secret: secret.inner(),
            ephemeral_signature_secret: SecretKey::random(&mut rand::rngs::OsRng).inner(),
        };

        let recipient_pub = PublicKey::from_secret(secret);
        let receipt_coin_blind = BaseBlind::random(&mut rand::rngs::OsRng);
        let output = redeem_v1::RedeemCallOutput {
            recipient: poseidon_hash([recipient_pub.x()]),
            recipient_pub,
            token_id,
            spend_hook: spend_hook_out,
            user_data: pallas::Base::zero(),
            coin_blind: receipt_coin_blind.inner(),
        };

        smol::block_on(Self::build_redeem(input, output))
    }

    /// Build TokenMintV1 from JSON params + wallet state.
    fn build_token_mint_from_state(
        params_json: &str,
        wallet_state: &dyn WalletStateProvider,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let v: serde_json::Value = serde_json::from_str(params_json)
            .map_err(|e| format!("invalid JSON: {}", e))?;

        let supply_str = parse_json_string(&v, "supply")?;
        let name = parse_json_string(&v, "name").unwrap_or("token");
        let decimals = v["decimals"].as_u64().unwrap_or(8);

        // Generate mint authority keypair
        let mint_authority = SecretKey::random(&mut rand::rngs::OsRng);
        let mint_authority_public = poseidon_hash([mint_authority.inner()]);
        let token_blind = BaseBlind::random(&mut rand::rngs::OsRng);
        let token_user_data = pallas::Base::zero();

        // Derive token_id
        let token_id = poseidon_hash([mint_authority_public, token_user_data, token_blind.inner()]);

        // Parse supply amount
        let mint_amount: u64 = supply_str.parse()
            .map_err(|_| format!("supply: not a valid u64: {}", supply_str))?;

        // Get recipient from wallet
        let addr = wallet_state.default_address()?;
        let recipient_bytes = bs58::decode(&addr).into_vec()
            .map_err(|e| format!("address bs58: {}", e))?;
        let recipient_pk = PublicKey::from_bytes(
            recipient_bytes.try_into().map_err(|_| "Invalid address length".to_string())?
        ).map_err(|_| "Invalid address".to_string())?;
        let recipient = poseidon_hash([recipient_pk.x()]);
        let coin_blind = BaseBlind::random(&mut rand::rngs::OsRng);

        let input = token_mint_v1::TokenMintCallInput {
            token_auth_parent: mint_authority_public,
            token_user_data,
            token_blind: token_blind.inner(),
            recipient,
            value: mint_amount,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: coin_blind.inner(),
        };

        smol::block_on(Self::build_token_mint(input))
    }

    /// Build MintV1 from JSON params + wallet state.
    fn build_mint_from_state(
        params_json: &str,
        wallet_state: &dyn WalletStateProvider,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let v: serde_json::Value = serde_json::from_str(params_json)
            .map_err(|e| format!("invalid JSON: {}", e))?;

        let token_id_str = parse_json_string(&v, "token_id")?;
        let recipient_str = parse_json_string(&v, "recipient")?;
        let value = parse_json_u64(&v, "value")?;

        let token_id = decode_bs58_field(token_id_str)?;

        // Get recipient
        let recipient_bytes = bs58::decode(recipient_str).into_vec()
            .map_err(|e| format!("recipient bs58: {}", e))?;
        let recipient_pk = PublicKey::from_bytes(
            recipient_bytes.try_into().map_err(|_| "Invalid recipient length".to_string())?
        ).map_err(|_| "Invalid recipient public key".to_string())?;

        let coin_blind = BaseBlind::random(&mut rand::rngs::OsRng);
        let recipient_base = poseidon_hash([recipient_pk.x()]);

        // Get mint authority from wallet secret
        let secret = decode_bs58_secret(&wallet_state.get_secret()?)?;

        let input = mint_v1::MintCallInput {
            mint_secret: secret.inner(),
            token_leaf_pos: 0,
            token_path: vec![],
            recipient: recipient_base,
            value,
            token_id,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: coin_blind.inner(),
        };

        smol::block_on(Self::build_mint(input))
    }

    /// Build OtcSwapV1 from JSON params + wallet state.
    fn build_otc_swap_from_state(
        params_json: &str,
        wallet_state: &dyn WalletStateProvider,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let v: serde_json::Value = serde_json::from_str(params_json)
            .map_err(|e| format!("invalid JSON: {}", e))?;

        let our_swap_json = parse_json_string(&v, "our_swap")?;
        let their_swap_json = parse_json_string(&v, "their_swap")?;

        let our: serde_json::Value = serde_json::from_str(our_swap_json)
            .map_err(|e| format!("our_swap JSON: {}", e))?;
        let their: serde_json::Value = serde_json::from_str(their_swap_json)
            .map_err(|e| format!("their_swap JSON: {}", e))?;

        let our_coin_id = parse_json_string(&our, "coin_id")?;
        let their_coin_id = parse_json_string(&their, "coin_id")?;
        let our_recipient_str = parse_json_string(&our, "recipient")?;
        let their_recipient_str = parse_json_string(&their, "recipient")?;

        let all_coins = wallet_state.held_capabilities_for_token("")?;

        let our_coin = all_coins.iter().find(|c| c.cap_id == our_coin_id)
            .ok_or_else(|| format!("Our coin not found: {}", our_coin_id))?;
        let their_coin = all_coins.iter().find(|c| c.cap_id == their_coin_id)
            .ok_or_else(|| format!("Their coin not found: {}", their_coin_id))?;

        let our_proof = wallet_state.get_merkle_proof(&our_coin.cap_id)?;
        let their_proof = wallet_state.get_merkle_proof(&their_coin.cap_id)?;
        let our_secret = decode_bs58_secret(&our_coin.secret)?;
        let their_secret = decode_bs58_secret(&their_coin.secret)?;
        let our_blind = decode_bs58_field(&our_coin.cap_blind)?;
        let their_blind = decode_bs58_field(&their_coin.cap_blind)?;
        let our_token = decode_bs58_field(&our_coin.token_id)?;
        let their_token = decode_bs58_field(&their_coin.token_id)?;

        let our_recipient_bytes = bs58::decode(our_recipient_str).into_vec()
            .map_err(|e| format!("our recipient bs58: {}", e))?;
        let our_recipient_pk = PublicKey::from_bytes(
            our_recipient_bytes.try_into().map_err(|_| "Invalid our recipient length".to_string())?
        ).map_err(|_| "Invalid our recipient".to_string())?;

        let their_recipient_bytes = bs58::decode(their_recipient_str).into_vec()
            .map_err(|e| format!("their recipient bs58: {}", e))?;
        let their_recipient_pk = PublicKey::from_bytes(
            their_recipient_bytes.try_into().map_err(|_| "Invalid their recipient length".to_string())?
        ).map_err(|_| "Invalid their recipient".to_string())?;

        let our_input = transfer_v1::TransferCallInput {
            value: our_coin.value,
            token_id: our_token,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: our_blind,
            leaf_position: our_proof.leaf_position,
            merkle_path: decode_merkle_path(&our_proof.siblings)?,
            secret: our_secret.inner(),
            ephemeral_signature_secret: SecretKey::random(&mut rand::rngs::OsRng).inner(),
        };

        let their_input = transfer_v1::TransferCallInput {
            value: their_coin.value,
            token_id: their_token,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: their_blind,
            leaf_position: their_proof.leaf_position,
            merkle_path: decode_merkle_path(&their_proof.siblings)?,
            secret: their_secret.inner(),
            ephemeral_signature_secret: SecretKey::random(&mut rand::rngs::OsRng).inner(),
        };

        let output_for_them = transfer_v1::TransferCallOutput {
            recipient: poseidon_hash([their_recipient_pk.x()]),
            recipient_pub: their_recipient_pk,
            value: our_coin.value,
            token_id: our_token,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: BaseBlind::random(&mut rand::rngs::OsRng).inner(),
        };

        let output_for_us = transfer_v1::TransferCallOutput {
            recipient: poseidon_hash([our_recipient_pk.x()]),
            recipient_pub: our_recipient_pk,
            value: their_coin.value,
            token_id: their_token,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: BaseBlind::random(&mut rand::rngs::OsRng).inner(),
        };

        smol::block_on(Self::build_transfer(
            vec![our_input, their_input],
            vec![output_for_them, output_for_us],
        ))
    }
}  // impl PromissoryNoteClient (JSON-driven dispatch helpers)

use dwow_core::{
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
};
use dwow_serial::AsyncEncodable;

impl PromissoryNoteClient {
    /// Build a TransferV1 call. Encapsulates ZK binary loading + ProvingKey
    /// construction + builder invocation — the wallet provides only typed inputs.
    pub async fn build_transfer(
        inputs: Vec<transfer_v1::TransferCallInput>,
        outputs: Vec<transfer_v1::TransferCallOutput>,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let burn_zkbin = ZkBinary::decode(
            zkbins::PROMISSORY_NOTE_CONTRACT_ZKAS_BURN_V1_BIN, false,
        ).map_err(|e| format!("decode burn zkbin: {:?}", e))?;
        let blind_output_zkbin = ZkBinary::decode(
            zkbins::PROMISSORY_NOTE_CONTRACT_ZKAS_BLIND_OUTPUT_V1_BIN, false,
        ).map_err(|e| format!("decode blind_output zkbin: {:?}", e))?;

        let burn_pk = ProvingKey::build(0, &ZkCircuit::new(
            empty_witnesses(&burn_zkbin).map_err(|e| format!("burn witnesses: {:?}", e))?,
            &burn_zkbin,
        ));
        let blind_output_pk = ProvingKey::build(0, &ZkCircuit::new(
            empty_witnesses(&blind_output_zkbin).map_err(|e| format!("blind_output witnesses: {:?}", e))?,
            &blind_output_zkbin,
        ));

        let builder = transfer_v1::TransferCallBuilder {
            inputs,
            outputs,
            burn_zkbin,
            burn_pk,
            blind_output_zkbin,
            blind_output_pk,
        };
        let debris = builder.build()
            .map_err(|e| format!("build transfer: {:?}", e))?;

        let mut call_data = vec![];
        debris.params.encode_async(&mut call_data).await
            .map_err(|e| format!("encode params: {:?}", e))?;

        let proof_bytes: Vec<Vec<u8>> = debris.proofs.iter()
            .map(|p| p.as_ref().to_vec())
            .collect();
        Ok((call_data, proof_bytes))
    }

    /// Build a BurnV1 call.
    pub async fn build_burn(
        inputs: Vec<burn_v1::BurnCallInput>,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let burn_zkbin = ZkBinary::decode(
            zkbins::PROMISSORY_NOTE_CONTRACT_ZKAS_BURN_V1_BIN, false,
        ).map_err(|e| format!("decode burn zkbin: {:?}", e))?;

        let burn_pk = ProvingKey::build(0, &ZkCircuit::new(
            empty_witnesses(&burn_zkbin).map_err(|e| format!("burn witnesses: {:?}", e))?,
            &burn_zkbin,
        ));

        let builder = burn_v1::BurnCallBuilder { inputs, burn_zkbin, burn_pk };
        let debris = builder.build()
            .map_err(|e| format!("build burn: {:?}", e))?;

        let mut call_data = vec![];
        debris.params.encode_async(&mut call_data).await
            .map_err(|e| format!("encode params: {:?}", e))?;

        let proof_bytes: Vec<Vec<u8>> = debris.proofs.iter()
            .map(|p| p.as_ref().to_vec())
            .collect();
        Ok((call_data, proof_bytes))
    }

    /// Build a RedeemV1 call.
    pub async fn build_redeem(
        input: redeem_v1::RedeemCallInput,
        output: redeem_v1::RedeemCallOutput,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let burn_zkbin = ZkBinary::decode(
            zkbins::PROMISSORY_NOTE_CONTRACT_ZKAS_BURN_V1_BIN, false,
        ).map_err(|e| format!("decode burn zkbin: {:?}", e))?;
        let redeem_zkbin = ZkBinary::decode(
            zkbins::PROMISSORY_NOTE_CONTRACT_ZKAS_REDEEM_V1_BIN, false,
        ).map_err(|e| format!("decode redeem zkbin: {:?}", e))?;

        let burn_pk = ProvingKey::build(0, &ZkCircuit::new(
            empty_witnesses(&burn_zkbin).map_err(|e| format!("burn witnesses: {:?}", e))?,
            &burn_zkbin,
        ));
        let redeem_pk = ProvingKey::build(0, &ZkCircuit::new(
            empty_witnesses(&redeem_zkbin).map_err(|e| format!("redeem witnesses: {:?}", e))?,
            &redeem_zkbin,
        ));

        let builder = redeem_v1::RedeemCallBuilder {
            input, output, burn_zkbin, burn_pk, redeem_zkbin, redeem_pk,
        };
        let debris = builder.build()
            .map_err(|e| format!("build redeem: {:?}", e))?;

        let mut call_data = vec![];
        debris.params.encode_async(&mut call_data).await
            .map_err(|e| format!("encode params: {:?}", e))?;

        let proof_bytes: Vec<Vec<u8>> = debris.proofs.iter()
            .map(|p| p.as_ref().to_vec())
            .collect();
        Ok((call_data, proof_bytes))
    }

    /// Build a TokenMintV1 call.
    pub async fn build_token_mint(
        input: token_mint_v1::TokenMintCallInput,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let token_mint_zkbin = ZkBinary::decode(
            zkbins::PROMISSORY_NOTE_CONTRACT_ZKAS_TOKEN_MINT_V1_BIN, false,
        ).map_err(|e| format!("decode token_mint zkbin: {:?}", e))?;

        let token_mint_pk = ProvingKey::build(0, &ZkCircuit::new(
            empty_witnesses(&token_mint_zkbin).map_err(|e| format!("token_mint witnesses: {:?}", e))?,
            &token_mint_zkbin,
        ));

        let builder = token_mint_v1::TokenMintCallBuilder { input, token_mint_zkbin, token_mint_pk };
        let debris = builder.build()
            .map_err(|e| format!("build token_mint: {:?}", e))?;

        let mut call_data = vec![];
        debris.params.encode_async(&mut call_data).await
            .map_err(|e| format!("encode params: {:?}", e))?;

        let proof_bytes: Vec<Vec<u8>> = debris.proofs.iter()
            .map(|p| p.as_ref().to_vec())
            .collect();
        Ok((call_data, proof_bytes))
    }

    /// Build a MintV1 call.
    pub async fn build_mint(
        input: mint_v1::MintCallInput,
    ) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let mint_zkbin = ZkBinary::decode(
            zkbins::PROMISSORY_NOTE_CONTRACT_ZKAS_MINT_V1_BIN, false,
        ).map_err(|e| format!("decode mint zkbin: {:?}", e))?;

        let mint_pk = ProvingKey::build(0, &ZkCircuit::new(
            empty_witnesses(&mint_zkbin).map_err(|e| format!("mint witnesses: {:?}", e))?,
            &mint_zkbin,
        ));

        let builder = mint_v1::MintCallBuilder { input, mint_zkbin, mint_pk };
        let debris = builder.build()
            .map_err(|e| format!("build mint: {:?}", e))?;

        let mut call_data = vec![];
        debris.params.encode_async(&mut call_data).await
            .map_err(|e| format!("encode params: {:?}", e))?;

        let proof_bytes: Vec<Vec<u8>> = debris.proofs.iter()
            .map(|p| p.as_ref().to_vec())
            .collect();
        Ok((call_data, proof_bytes))
    }
}
