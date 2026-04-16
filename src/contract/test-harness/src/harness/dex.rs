/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! DEX Test Harness
//!
//! Provides isolated testing for DEX atomic swap contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::SecretKey,
    pasta::pallas,
};

// DEX client modules - re-exported for convenience
pub use darkfi_dex_contract::client::accept_swap_v1::AcceptSwapCallData;
pub use darkfi_dex_contract::client::cancel_swap_v1::CancelSwapCallData;
pub use darkfi_dex_contract::client::create_swap_v1::CreateSwapCallData;
pub use darkfi_dex_contract::client::execute_swap_fee_v1::ExecuteSwapFeeCallData;
pub use darkfi_dex_contract::client::execute_swap_slippage_v1::ExecuteSwapSlippageCallData;
pub use darkfi_dex_contract::client::execute_swap_v1::ExecuteSwapCallData;

/// DEX Harness for atomic swap testing
pub struct DexHarness {
    /// CreateSwap_V1 ZkBinary
    create_swap_zkbin: ZkBinary,
    /// CreateSwap_V1 ProvingKey
    create_swap_pk: ProvingKey,
    /// AcceptSwap_V1 ZkBinary
    accept_swap_zkbin: ZkBinary,
    /// AcceptSwap_V1 ProvingKey
    accept_swap_pk: ProvingKey,
    /// ExecuteSwap_V1 ZkBinary
    execute_swap_zkbin: ZkBinary,
    /// ExecuteSwap_V1 ProvingKey
    execute_swap_pk: ProvingKey,
    /// CancelSwap_V1 ZkBinary
    cancel_swap_zkbin: ZkBinary,
    /// CancelSwap_V1 ProvingKey
    cancel_swap_pk: ProvingKey,
}

impl DexHarness {
    /// Create a new DEX harness with pre-loaded circuits
    pub fn new() -> Self {
        // Load circuit binaries
        let create_bin = include_bytes!("../../../dex/proof/create_swap_v1.zk.bin");
        let accept_bin = include_bytes!("../../../dex/proof/accept_swap_v1.zk.bin");
        let execute_bin = include_bytes!("../../../dex/proof/execute_swap_v1.zk.bin");
        let cancel_bin = include_bytes!("../../../dex/proof/cancel_swap_v1.zk.bin");

        let create_swap_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let accept_swap_zkbin = ZkBinary::decode(accept_bin, false).unwrap();
        let execute_swap_zkbin = ZkBinary::decode(execute_bin, false).unwrap();
        let cancel_swap_zkbin = ZkBinary::decode(cancel_bin, false).unwrap();

        // Build proving keys
        let create_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_swap_zkbin).unwrap(),
            &create_swap_zkbin,
        );
        let accept_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&accept_swap_zkbin).unwrap(),
            &accept_swap_zkbin,
        );
        let execute_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&execute_swap_zkbin).unwrap(),
            &execute_swap_zkbin,
        );
        let cancel_swap_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&cancel_swap_zkbin).unwrap(),
            &cancel_swap_zkbin,
        );

        let create_swap_pk = ProvingKey::build(create_swap_zkbin.k, &create_swap_circuit);
        let accept_swap_pk = ProvingKey::build(accept_swap_zkbin.k, &accept_swap_circuit);
        let execute_swap_pk = ProvingKey::build(execute_swap_zkbin.k, &execute_swap_circuit);
        let cancel_swap_pk = ProvingKey::build(cancel_swap_zkbin.k, &cancel_swap_circuit);

        Self {
            create_swap_zkbin,
            create_swap_pk,
            accept_swap_zkbin,
            accept_swap_pk,
            execute_swap_zkbin,
            execute_swap_pk,
            cancel_swap_zkbin,
            cancel_swap_pk,
        }
    }

    /// Get circuit namespaces
    pub fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateSwap_V1",
            "AcceptSwap_V1",
            "ExecuteSwap_V1",
            "CancelSwap_V1",
        ]
    }

    /// Create a swap proposal
    pub fn create_swap(
        &self,
        secret: pallas::Base,
        offer_token: pallas::Base,
        offer_amount: u64,
        request_token: pallas::Base,
        request_amount: u64,
        signature_secret: SecretKey,
    ) -> CreateSwapCallData {
        CreateSwapCallData::new(
            secret,
            offer_token,
            offer_amount,
            request_token,
            request_amount,
            signature_secret,
        )
    }

    /// Accept a swap proposal
    pub fn accept_swap(
        &self,
        swap_id: pallas::Base,
        proposer_lock_commitment: pallas::Base,
        secret: pallas::Base,
        offer_token: pallas::Base,
        offer_amount: u64,
        signature_secret: SecretKey,
    ) -> AcceptSwapCallData {
        AcceptSwapCallData::new(
            swap_id,
            proposer_lock_commitment,
            secret,
            offer_token,
            offer_amount,
            signature_secret,
        )
    }

    /// Execute an atomic swap
    pub fn execute_swap(
        &self,
        alice_secret: pallas::Base,
        alice_token: pallas::Base,
        alice_amount: u64,
        alice_lock: pallas::Base,
        bob_secret: pallas::Base,
        bob_token: pallas::Base,
        bob_amount: u64,
        bob_lock: pallas::Base,
        fill_amount: u64,
    ) -> ExecuteSwapCallData {
        ExecuteSwapCallData::new(
            alice_secret,
            alice_token,
            alice_amount.into(),
            alice_lock,
            bob_secret,
            bob_token,
            bob_amount.into(),
            bob_lock,
            fill_amount.into(),
        )
    }

    /// Execute swap with fee
    pub fn execute_swap_fee(
        &self,
        alice_secret: pallas::Base,
        alice_token: pallas::Base,
        alice_amount: u64,
        alice_lock: pallas::Base,
        bob_secret: pallas::Base,
        bob_token: pallas::Base,
        bob_amount: u64,
        bob_lock: pallas::Base,
        fill_amount: u64,
        fee_bps: pallas::Base,
    ) -> ExecuteSwapFeeCallData {
        ExecuteSwapFeeCallData::new(
            alice_secret,
            alice_token,
            alice_amount.into(),
            alice_lock,
            bob_secret,
            bob_token,
            bob_amount.into(),
            bob_lock,
            fill_amount.into(),
            fee_bps,
        )
    }

    /// Execute swap with slippage tolerance
    pub fn execute_swap_slippage(
        &self,
        alice_secret: pallas::Base,
        alice_token: pallas::Base,
        alice_amount: u64,
        alice_lock: pallas::Base,
        bob_secret: pallas::Base,
        bob_token: pallas::Base,
        bob_amount: u64,
        bob_lock: pallas::Base,
        fill_amount: u64,
        slippage_bps: pallas::Base,
    ) -> ExecuteSwapSlippageCallData {
        ExecuteSwapSlippageCallData::new(
            alice_secret,
            alice_token,
            alice_amount.into(),
            alice_lock,
            bob_secret,
            bob_token,
            bob_amount.into(),
            bob_lock,
            fill_amount.into(),
            slippage_bps,
        )
    }

    /// Cancel a swap
    pub fn cancel_swap(
        &self,
        swap_id: pallas::Base,
        lock_commitment: pallas::Base,
        secret: pallas::Base,
        token: pallas::Base,
        amount: u64,
    ) -> CancelSwapCallData {
        CancelSwapCallData::new(swap_id, lock_commitment, secret, token, amount.into())
    }
}

impl Default for DexHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl super::ContractHarness for DexHarness {
    fn name(&self) -> &str {
        "dex"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateSwap_V1",
            "AcceptSwap_V1",
            "ExecuteSwap_V1",
            "CancelSwap_V1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateSwap_V1" => Some(&self.create_swap_zkbin),
            "AcceptSwap_V1" => Some(&self.accept_swap_zkbin),
            "ExecuteSwap_V1" => Some(&self.execute_swap_zkbin),
            "CancelSwap_V1" => Some(&self.cancel_swap_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateSwap_V1" => Some(&self.create_swap_pk),
            "AcceptSwap_V1" => Some(&self.accept_swap_pk),
            "ExecuteSwap_V1" => Some(&self.execute_swap_pk),
            "CancelSwap_V1" => Some(&self.cancel_swap_pk),
            _ => None,
        }
    }
}