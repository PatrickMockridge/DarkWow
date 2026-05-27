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

use dwow_sdk::{
    crypto::poseidon_hash,
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    wasm, ContractCall,
};

use crate::{
    error::GameRoomError,
    model::{
        ContributeEntropyParamsV1, ContributeEntropyUpdateV1, EntropyContribution, EntropyMode,
        GameRoom, PlayerAccount,
    },
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn process_contribute_entropy_instruction(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: ContributeEntropyParamsV1 = dwow_serial::deserialize(&self_.data[1..])?;

    msg!("[Entropy] Contribute entropy to room {:?}", params.room_id);

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &dwow_serial::serialize(&params.room_id))?
    else {
        msg!("[Entropy] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let mut room: GameRoom =
        dwow_serial::deserialize(&room_data)?;

    // Validate entropy mode
    if room.config.entropy_mode != EntropyMode::TrustedSetup {
        msg!("[Entropy] Error: Room not using TrustedSetup mode");
        return Err(GameRoomError::UnauthorizedCaller.into())
    }

    // Check deadline
    let current_block = wasm::util::get_verifying_block_height()?;
    if current_block as u64 > room.entropy_deadline {
        msg!("[Entropy] Error: Entropy contribution deadline passed");
        return Err(GameRoomError::EntropyDeadlinePassed.into())
    }

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Get account
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = dwow_serial::serialize(&(params.room_id, caller.xy().0));
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[Entropy] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let mut account: PlayerAccount =
        dwow_serial::deserialize(&account_data)?;

    // Check if already contributed
    if account.entropy_contribution.is_some() {
        msg!("[Entropy] Error: Already contributed entropy");
        return Err(GameRoomError::EntropyNotContributed.into())
    }

    let mut new_combined_entropy = room.combined_entropy;

    // Process based on whether this is a commit or reveal
    match params.reveal {
        Some(reveal) => {
            // This is a reveal - verify it matches the commitment
            let expected_commitment = poseidon_hash([reveal, caller.xy().0, params.room_id]);
            if expected_commitment != params.commitment {
                msg!("[Entropy] Error: Reveal does not match commitment");
                return Err(GameRoomError::EntropyRevealMismatch.into())
            }

            // Update combined entropy
            match new_combined_entropy {
                None => new_combined_entropy = Some(reveal),
                Some(current) => new_combined_entropy = Some(poseidon_hash([current, reveal])),
            }

            msg!("[Entropy] Entropy revealed and mixed");
        }
        None => {
            // This is just a commitment - store it
            msg!("[Entropy] Entropy commitment received");
        }
    }

    // Store entropy contribution on account
    account.entropy_contribution = Some(EntropyContribution {
        commitment: params.commitment,
        revealed_nonce: params.reveal,
        contributed_at: current_block as u64,
    });
    wasm::db::db_set(accounts_db, &account_key, &dwow_serial::serialize(&account))?;

    // Update room
    room.total_entropy_contributions += 1;
    room.combined_entropy = new_combined_entropy;
    wasm::db::db_set(
        rooms_db,
        &dwow_serial::serialize(&params.room_id),
        &dwow_serial::serialize(&room),
    )?;

    msg!(
        "[Entropy] Contribution {} of {} received",
        room.total_entropy_contributions,
        room.config.required_entropy_contributions
    );

    let update = ContributeEntropyUpdateV1 {
        room_id: params.room_id,
        player: caller,
        combined_entropy: new_combined_entropy,
        contributions_count: room.total_entropy_contributions,
    };
    wasm::util::set_return_data(&dwow_serial::serialize(&update))
}

pub(crate) fn apply_contribute_entropy_update(
    _cid: dwow_sdk::crypto::ContractId,
    update: ContributeEntropyUpdateV1,
) -> ContractResult {
    msg!(
        "[Entropy] Update applied: player {:?} contributed, total {} contributions",
        update.player,
        update.contributions_count
    );
    Ok(())
}