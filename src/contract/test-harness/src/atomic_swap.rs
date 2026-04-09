/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Atomic Swap contract test harness
//!
//! This module provides a test harness for the Atomic Swap contract,
//! a WASM-based cross-chain atomic swap using HTLC pattern.
//!
//! Flow:
//! 1. One party creates a swap with CreateSwapV1 (funds locked)
//! 2. Other party claims with ClaimV1 (reveals secret on DarkFi)
//! 3. Original party refunds after timelock with RefundV1 (if not claimed)

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{poseidon_hash, ContractId, PublicKey},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use tracing::debug;
use darkfi_atomic_swap_contract::{
    model::{CreateSwapParamsV1, CreateSwapUpdateV1, ClaimParamsV1, ClaimUpdateV1, RefundParamsV1, RefundUpdateV1, SwapId},
    ATOMIC_SWAP_CONTRACT_SWAPS_TREE,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Atomic Swap WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the atomic_swap contract.
    pub async fn deploy_atomic_swap(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        let atomic_swap_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed atomic_swap contract: {:?}",
            atomic_swap_contract_id
        );

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(atomic_swap_contract_id)
    }

    /// Create a `AtomicSwap::CreateSwapV1` transaction.
    ///
    /// Creates an atomic swap (HTLC) locking funds until claim or timelock expiry.
    pub async fn atomic_swap_create(
        &mut self,
        holder: &Holder,
        atomic_swap_contract_id: ContractId,
        hash: pallas::Base,
        timelock: u64,
        side: u8,
        external_chain: u8,
        external_receiver: pallas::Base,
        darkfi_receiver: PublicKey,
        amount: u64,
        token_id: pallas::Base,
        blind: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, CreateSwapParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateSwapParamsV1 {
            hash,
            timelock,
            side,
            external_chain,
            external_receiver,
            darkfi_receiver,
            amount,
            token_id,
            blind,
            commitment: pallas::Base::zero(),
        };

        let mut data = vec![0x01];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: atomic_swap_contract_id, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[holder_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            tx_builder.append(
                ContractCallLeaf { call: fee_call, proofs: fee_proofs },
                vec![],
            )?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute a `AtomicSwap::CreateSwapV1` transaction.
    pub async fn execute_atomic_swap_create_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateSwapParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("atomic_swap::create", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `AtomicSwap::ClaimV1` transaction.
    ///
    /// Claims the swap by revealing the secret.
    pub async fn atomic_swap_claim(
        &mut self,
        holder: &Holder,
        atomic_swap_contract_id: ContractId,
        swap_id: SwapId,
        secret: pallas::Base,
        nullifier: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, ClaimParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = ClaimParamsV1 { swap_id, secret, nullifier };

        let mut data = vec![0x02];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: atomic_swap_contract_id, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[holder_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            tx_builder.append(
                ContractCallLeaf { call: fee_call, proofs: fee_proofs },
                vec![],
            )?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute a `AtomicSwap::ClaimV1` transaction.
    pub async fn execute_atomic_swap_claim_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &ClaimParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("atomic_swap::claim", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `AtomicSwap::RefundV1` transaction.
    ///
    /// Refunds the swap after timelock expiration.
    pub async fn atomic_swap_refund(
        &mut self,
        holder: &Holder,
        atomic_swap_contract_id: ContractId,
        swap_id: SwapId,
        current_block: u64,
        nullifier: pallas::Base,
        recipient: PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, RefundParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = RefundParamsV1 { swap_id, current_block, nullifier, recipient };

        let mut data = vec![0x03];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: atomic_swap_contract_id, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[holder_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            tx_builder.append(
                ContractCallLeaf { call: fee_call, proofs: fee_proofs },
                vec![],
            )?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute a `AtomicSwap::RefundV1` transaction.
    pub async fn execute_atomic_swap_refund_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &RefundParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("atomic_swap::refund", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}