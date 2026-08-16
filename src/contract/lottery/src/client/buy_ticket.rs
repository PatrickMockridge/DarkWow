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

//! BuyTicketV1 Client API

use dwow_sdk::crypto::{ContractId, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use pasta_curves::group::Group;

use crate::model::{derive_ticket_id, BuyTicketParamsV1};

/// Generate a ticket purchase transaction
pub fn create_buy_ticket_tx(
    wallet_secret: SecretKey,
    contract_id: ContractId,
    lottery_id: pallas::Base,
    numbers: &[u8],
    nonce: pallas::Base,
    token_id: pallas::Base,
    value: u64,
    secret_key: &pallas::Base,
    instance_seed: [u8; 32],
) -> Result<BuyTicketParamsV1, Box<dyn std::error::Error>> {
    // Sort numbers for consistent ordering
    let mut sorted_numbers = numbers.to_vec();
    sorted_numbers.sort_unstable();

    // Create commitment using iterative hashing: PoseidonHash(...PoseidonHash(lottery_id, n1), n2..., nonce)
    let mut state = lottery_id;
    for &n in &sorted_numbers {
        state = dwow_sdk::crypto::poseidon_hash([state, pallas::Base::from(n as u64)]);
    }
    let commitment = dwow_sdk::crypto::poseidon_hash([state, nonce]);

    // Derive player instance key
    let instance_secret = wallet_secret.derive_instance(&contract_id, &instance_seed)?;
    let player_pub = PublicKey::from_secret(instance_secret);

    // Sign the commitment
    let signature = sign_commitment(&commitment, secret_key)?;

    // Create params
    let params = BuyTicketParamsV1 {
        player_pub,
        commitment,
        token_id,
        value,
        value_commit: pallas::Point::identity(),
        signature,
        instance_seed,
        lottery_id,
        nonce,
    };

    Ok(params)
}

/// Sign a commitment with a secret key
fn sign_commitment(
    commitment: &pallas::Base,
    secret_key: &pallas::Base,
) -> Result<pallas::Base, Box<dyn std::error::Error>> {
    // For now, simple signature using Poseidon
    // In production, use proper Schnorr signature
    Ok(dwow_sdk::crypto::poseidon_hash([*commitment, *secret_key]))
}

/// Derive ticket ID from parameters (with derived key)
pub fn derive_lottery_ticket_id(
    wallet_secret: SecretKey,
    contract_id: ContractId,
    lottery_id: pallas::Base,
    nonce: pallas::Base,
    value: u64,
    instance_seed: [u8; 32],
) -> Result<pallas::Base, Box<dyn std::error::Error>> {
    let instance_secret = wallet_secret.derive_instance(&contract_id, &instance_seed)?;
    let player_pub = PublicKey::from_secret(instance_secret);
    Ok(derive_ticket_id(lottery_id, &player_pub, value, nonce))
}
