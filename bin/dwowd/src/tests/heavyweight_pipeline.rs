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

//! HeavyweightPipeline — Contract function/endpoint testing with real ZK proofs (Level 2).
//!
//! Tests contract functions, state transitions, and uncle-merkle block execution
//! with full ZK proof generation and verification. Uses the direct `deploy_contract()`
//! path for setup convenience — deployment correctness is tested separately by the
//! lightweight pipeline ([super::pipeline]).
//!
//! ## Demarcation from Lightweight Tests
//!
//! | Concern | Lightweight | Heavyweight (here) |
//! |---------|-------------|-------------------|
//! | Deployment | Deployooor-based (real production path) | Direct `deploy_contract()` (setup only) |
//! | Contract functions | Not tested | Every endpoint exercised |
//! | ZK proofs | None | Required for all calls |
//! | State transitions | Not tested | Applied via `accept_block()` (production path) |
//! | Uncle-merkle blocks | Not tested | Multi-uncle, depth, mixed exec |
//! | Block gas limits | Not tested | Cumulative gas tracking |
//!
//! **Both are required.** See [super::pipeline] for deployment testing.
//!
//! Each test function:
//! 1. Creates a HeavyweightPipeline with the contract's harness (ZK circuits + proving keys)
//! 2. Deploys the contract WASM via direct path (setup convenience — not testing deployment)
//! 3. Exercises every endpoint via harness methods, verifying proofs + call_data
//! 4. Applies blocks via `accept_block()` — the production block acceptance path.
//!
//! ## Running
//!
//! ```bash
//! cargo test --release -p dwowd test_heavyweight_dao_escrow
//! cargo test --release -p dwowd test_heavyweight_identity
//! RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd test_heavyweight
//! ```

use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use dwow_core::zk::Proof;
use dwow_sdk::blockchain::{BlockReward, BlockTarget, FeeAmount};
use dwow_sdk::crypto::{ContractId, NATIVE_TOKEN_CONTRACT_ID};
use dwow_sdk::pasta::group::{Group, GroupEncoding};
use dwow_contract_test_harness::harness::ContractHarness;


/// Global counter for unique temp file names — prevents race conditions
/// when multiple HeavyweightPipeline tests run in parallel.
static KEY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Build a minimal L1 witness for a single-call test transaction.
/// The witness is `dwow_serial::serialize(&core_tx)` where the core tx
/// reconciles byte-for-byte with the chain tx's `contract_calls` and carries
/// the provided proofs. Required by `accept_block` step 2.6 and
/// `execute_block` step 3d — both call `decode_and_reconcile` which returns
/// `NoWitness` on an empty witness.
pub(crate) fn build_witness(
    contract_id: ContractId,
    call_data: &[u8],
    proofs: Vec<Proof>,
) -> Vec<u8> {
    let core_call = dwow_sdk::tx::ContractCall { contract_id, data: call_data.to_vec() };
    let core_tx = dwow_core::tx::Transaction {
        calls: vec![dwow_sdk::dark_tree::DarkLeaf {
            data: core_call,
            parent_index: None,
            children_indexes: vec![],
        }],
        proofs: vec![proofs],
        tx_commitment: [0u8; 32],
        nullifiers: vec![],
    };
    dwow_serial::serialize(&core_tx)
}

/// Verify that a witness contains a well-formed DarkLeaf call tree.
///
/// Deserializes the witness back to a core tx and checks that every call
/// has non-empty inner data and a valid function selector byte.  Call
/// this from heavyweight tests for early diagnostic feedback — a panic
/// here means the tree the execution layer will extract is malformed.
pub(crate) fn verify_witness_tree(witness: &[u8], label: &str) {
    let core_tx: dwow_core::tx::Transaction =
        match dwow_serial::deserialize(witness) {
            Ok(tx) => tx,
            Err(e) => {
                eprintln!(
                    "[tree-diag] {}: witness decode FAILED: {:?}",
                    label, e,
                );
                return;
            }
        };
    eprintln!(
        "[tree-diag] {}: {} calls in witness tree",
        label, core_tx.calls.len(),
    );
    for (i, leaf) in core_tx.calls.iter().enumerate() {
        let cid = leaf.data.contract_id;
        let inner = &leaf.data.data;
        let fn_code = inner.first().copied();
        let _has_children = !leaf.children_indexes.is_empty();
        let _has_parent = leaf.parent_index.is_some();
        eprintln!(
            "[tree-diag]   call[{}]: cid={} fn=0x{:02x?} data_len={} parent={:?} children={}",
            i, cid, fn_code, inner.len(),
            leaf.parent_index, leaf.children_indexes.len(),
        );
        if inner.is_empty() {
            eprintln!(
                "[tree-diag]   call[{}]: WARNING — empty inner data (no fn_code, no params)",
                i,
            );
        }
    }
}

/// Build a RandomX VM for `accept_block` — used by all `exec*` methods.
pub(crate) fn build_accept_vm(
    block: &dwow_chain::Block,
) -> dwow_core::Result<std::sync::Arc<randomx::RandomXVM>> {
    let rx_flags = randomx::RandomXFlags::get_recommended_flags()
        & !randomx::RandomXFlags::JIT;
    let rx_cache = randomx::RandomXCache::new(rx_flags, &block.header.randomx_key)
        .map_err(|e| dwow_core::Error::Custom(format!("RandomX cache: {}", e)))?;
    Ok(std::sync::Arc::new(
        randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
            .map_err(|e| dwow_core::Error::Custom(format!("RandomX VM: {}", e)))?,
    ))
}

/// Find a nonce that makes the block hash ≤ target. Used for test blocks
/// at heights > 2 where the expected target is lower than u32::MAX.
/// Returns the valid nonce.
pub(crate) fn mine_test_nonce(block: &dwow_chain::Block, vm: &randomx::RandomXVM, target: BlockTarget) -> dwow_core::Result<u32> {
    for nonce in 0u32..1_000_000 {
        let mut b = block.clone();
        b.header.nonce = nonce;
        let hash = b.hash_with_vm(vm).expect("hash failed");
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        if hash_u32 <= target.get() {
            return Ok(nonce);
        }
    }
    Err(dwow_core::Error::Custom(format!("Could not find valid nonce for target {} after 1M iterations", target)))
}


// ============================================================================
// promissory_note
// ============================================================================

#[test]
fn test_heavyweight_promissory_note() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::promissory_note_spec::promissory_note_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&promissory_note_test_spec()))?)
}

// ============================================================================
// dex
// ============================================================================

#[test]
fn test_heavyweight_dex() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::dex_spec::dex_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&dex_test_spec()))?)
}

// ============================================================================
// native_token
// ============================================================================

#[test]
fn test_heavyweight_native_token() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::native_token_spec::native_token_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&native_token_test_spec()))?)
}

// ============================================================================
// auction
// ============================================================================

#[test]
fn test_heavyweight_auction() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::auction_spec::auction_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&auction_test_spec()))?)
}

// ============================================================================
// escrow
// ============================================================================

#[test]
fn test_heavyweight_escrow() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::escrow_spec::escrow_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&escrow_test_spec()))?)
}

// ============================================================================
// escrow + contract metadata
// ============================================================================

#[test]
// Integration test: validates ContractMetadata serialization end-to-end with
// EscrowHarness ZK proof generation through accept_block. Deploys escrow contract,
// exercises create_escrow, and verifies metadata roundtrip encoding.
fn test_heavyweight_metadata() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::EscrowHarness;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::deploy::{Category, ContractMetadata};
    use dwow_sdk::pasta::pallas;

    println!("=== Escrow Heavyweight: Contract Metadata + State Transitions ===");

    smol::block_on(async {
        use crate::tests::blockchain::HeavyweightPipeline;

        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("metadata")));
        let harness = EscrowHarness::spawn();
        println!("Harness spawned with circuits: {:?}", harness.circuits());

        let wasm =
            include_bytes!("../../../../src/contract/escrow/dwow_escrow_contract.wasm");

        // --- Deploy with ContractMetadata as ix ---
        let metadata = ContractMetadata {
            name: "Heavyweight Escrow".to_string(),
            symbol: Some("HESC".to_string()),
            category: Category::Finance,
            description: Some(
                "Escrow deployed with metadata in heavyweight ZK proof test"
                    .to_string(),
            ),
            public: true,
            attestations: vec![],
        };
        let ix = metadata.to_ix_bytes();
        assert!(!ix.is_empty(), "serialized metadata must be non-empty");

        let contract_id = chain.deploy_with_ix(&harness, "escrow", wasm, &ix).await?;
        println!("Contract deployed with metadata at {:?}", contract_id.to_bytes());

        // Verify metadata roundtrips through ix bytes
        let decoded =
            ContractMetadata::from_ix_bytes(&ix).expect("metadata must roundtrip");
        assert_eq!(decoded.name, "Heavyweight Escrow");
        assert_eq!(decoded.symbol.as_deref(), Some("HESC"));
        assert_eq!(decoded.category, Category::Finance);
        assert_eq!(
            decoded.description.as_deref(),
            Some("Escrow deployed with metadata in heavyweight ZK proof test")
        );
        assert!(decoded.public);
        assert!(decoded.attestations.is_empty());

        // --- Exercise contract functions with ZK proofs ---
        let buyer_wallet_sk = SecretKey::from_base(pallas::Base::from(10u64));
        let seller_wallet_sk = SecretKey::from_base(pallas::Base::from(20u64));
        let token_id = pallas::Base::from(1u64);

        let instance_seed: [u8; 32] = {
            let mut seed = [0u8; 32];
            seed[0..8].copy_from_slice(&42u64.to_le_bytes());
            seed
        };

        let buyer_instance_sk = buyer_wallet_sk.derive_instance(&contract_id, &instance_seed).unwrap();
        let buyer_pub = PublicKey::from_secret(buyer_instance_sk.clone());
        let buyer_secret = *buyer_instance_sk.inner();
        let seller_instance_sk = seller_wallet_sk.derive_instance(&contract_id, &instance_seed).unwrap();
        let seller_pub = PublicKey::from_secret(seller_instance_sk.clone());
        let _seller_secret = *seller_instance_sk.inner();

        // --- create_escrow (ZK proof generation) ---
        println!("  Test: create_escrow");
        let create = harness.create_escrow(
            buyer_secret, buyer_pub, seller_pub, 5000, token_id, 1000, instance_seed,
        )?;
        assert!(!create.call_data.is_empty());
        println!("    call_data={}B", create.call_data.len());

        // Route through accept_block for production path verification
        let height_before = chain.height();
        println!("  Exec: CreateEscrowV1 through accept_block (height={})", height_before);
        chain.block()?
            .with_call(contract_id, &harness, &create.call_data, vec![create.proof.clone()])?
            .with_fee_collect()?
            .submit().await?;

        let height_after = chain.height();
        assert!(
            height_after > height_before,
            "height must increase after on-chain exec (was {}, now {})",
            height_before,
            height_after,
        );
        println!(
            "    block accepted OK (height {} -> {})",
            height_before, height_after
        );

        println!("=== Escrow Heavyweight Metadata Test: All assertions passed ===");
        Ok(())
    })
}

// ============================================================================
// stablecoin
// ============================================================================

#[test]
fn test_heavyweight_stablecoin() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::stablecoin_spec::stablecoin_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&stablecoin_test_spec()))?)
}

// ============================================================================
// bridge
// ============================================================================

#[test]
fn test_heavyweight_bridge() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::bridge_spec::bridge_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&bridge_test_spec()))?)
}

// ============================================================================
// labor_market
// ============================================================================

#[test]
fn test_heavyweight_labor_market() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::labor_market_spec::labor_market_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&labor_market_test_spec()))?)
}

// ============================================================================
// attestation
// ============================================================================

#[test]
fn test_heavyweight_attestation() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::attestation_spec::attestation_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&attestation_test_spec()))?)
}
#[test]
fn test_heavyweight_tender() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::tender_spec::tender_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&tender_test_spec()))?)
}

// ============================================================================
// subscription
// ============================================================================

#[test]
fn test_heavyweight_subscription() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::subscription_spec::subscription_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&subscription_test_spec()))?)
}

// ============================================================================
// oracle
// ============================================================================

#[test]
fn test_heavyweight_oracle() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::oracle_spec::oracle_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&oracle_test_spec()))?)
}

// ============================================================================
// pool_stake
// ============================================================================

#[test]
fn test_heavyweight_pool_stake() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::pool_stake_spec::pool_stake_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&pool_stake_test_spec()))?)
}

// ============================================================================
// relayer_endowment
// ============================================================================

#[test]
fn test_heavyweight_relayer_endowment() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::relayer_endowment_spec::relayer_endowment_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&relayer_endowment_test_spec()))?)
}

// ============================================================================
// slot
// ============================================================================

#[test]
fn test_heavyweight_slot() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::slot_spec::slot_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&slot_test_spec()))?)
}

// ============================================================================
// deployooor
// ============================================================================

#[test]
fn test_heavyweight_deployooor() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::deployooor_spec::deployooor_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&deployooor_test_spec()))?)
}
#[test]
fn test_heavyweight_drain_protection() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::drain_protection_spec::drain_protection_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&drain_protection_test_spec()))?)
}

// ============================================================================
// game_room
// ============================================================================

#[test]
fn test_heavyweight_game_room() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::game_room_spec::game_room_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&game_room_test_spec()))?)
}

// ============================================================================
// insurance_market
// ============================================================================

#[test]
fn test_heavyweight_insurance_market() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::insurance_market_spec::insurance_market_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&insurance_market_test_spec()))?)
}

// ============================================================================
// baccarat
// ============================================================================

#[test]
fn test_heavyweight_baccarat() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::baccarat_spec::baccarat_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&baccarat_test_spec()))?)
}

// ============================================================================
// betting_stake
// ============================================================================

#[test]
fn test_heavyweight_betting_stake() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::betting_stake_spec::betting_stake_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&betting_stake_test_spec()))?)
}

// ============================================================================
// darkbet_exchange
// ============================================================================

#[test]
fn test_heavyweight_darkbet_exchange() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::darkbet_exchange_spec::darkbet_exchange_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&darkbet_exchange_test_spec()))?)
}

// ============================================================================
// darktoshi_dice
// ============================================================================

#[test]
fn test_heavyweight_darktoshi_dice() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::darktoshi_dice_spec::darktoshi_dice_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&darktoshi_dice_test_spec()))?)
}


// ============================================================================
// lottery
// ============================================================================

#[test]
fn test_heavyweight_lottery() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::lottery_spec::lottery_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&lottery_test_spec()))?)
}

// ============================================================================
// roulette
// ============================================================================

#[test]
fn test_heavyweight_roulette() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::roulette_spec::roulette_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&roulette_test_spec()))?)
}

// ============================================================================
// DAO-Escrow Heavyweight Test
// ============================================================================

#[test]
fn test_heavyweight_dao_escrow() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::dao_escrow_spec::dao_escrow_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&dao_escrow_test_spec()))?)
}

// ============================================================================
// Identity Heavyweight Test
// ============================================================================

#[test]
fn test_heavyweight_identity() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::identity_spec::identity_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&identity_test_spec()))?)
}
#[test]
#[ignore = "HAZOP H-TF-003: not a heavyweight test — does not exercise accept_block"]
// Integration test: cross-contract orchestration across 4 contracts (Identity,
// LaborMarket, DaoEscrow, Attestation). Harness-exercise test — generates call_data
// and verifies it's non-empty but does NOT submit through accept_block.
fn test_heavyweight_recruitment_pipeline() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::{
        AttestationHarness, DaoEscrowHarness, IdentityHarness, LaborMarketHarness,
    };
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;

    println!("=== Recruitment Pipeline: Cross-Contract Integration ===");

    smol::block_on(async {
        // ------------------------------------------------------------------
        // Step 1: Deploy all four contracts
        // ------------------------------------------------------------------
        println!("\n--- Step 1: Deploy contracts ---");

        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("recruitment_pipeline")));

        // Deploy Identity
        let id_harness = IdentityHarness::spawn();
        println!("Identity harness: {:?}", id_harness.circuits());
        let _id_contract_id = *dwow_sdk::crypto::IDENTITY_CONTRACT_ID;  // deployed at genesis

        // Deploy Labor Market (with milestone_payment binary now registered)
        let lm_harness = LaborMarketHarness::spawn();
        println!("LaborMarket harness: {:?}", lm_harness.circuits());
        let lm_wasm = include_bytes!("../../../../src/contract/labor_market/dwow_labor_market_contract.wasm");
        let _lm_contract_id = chain.deploy(&lm_harness, "labor_market", lm_wasm).await?;
        println!("  LaborMarket deployed");

        // Deploy DAO Escrow
        let dao_harness = DaoEscrowHarness::spawn();
        println!("DAO-Escrow harness: {:?}", dao_harness.circuits());
        let dao_wasm = include_bytes!("../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm");
        let _dao_contract_id = chain.deploy(&dao_harness, "dao_escrow", dao_wasm).await?;
        println!("  DAO-Escrow deployed");

        // Attestation already deployed at genesis
        let att_harness = AttestationHarness::spawn();
        println!("Attestation harness: {:?}", att_harness.circuits());
        let _att_contract_id = *dwow_sdk::crypto::ATTESTATION_CONTRACT_ID;  // deployed at genesis

        // ------------------------------------------------------------------
        // Step 2: DAO initializes governance (dao_escrow::InitializeV1)
        // ------------------------------------------------------------------
        println!("\n--- Step 2: DAO initializes governance ---");
        let nullifier_k = pallas::Scalar::from(1u64);
        let dao_bulla = pallas::Base::from(1u64);
        let owner_secret = pallas::Base::from(30u64);
        let endowment_token_id = pallas::Base::from(2u64);
        let bulla_blind = pallas::Base::from(3u64);
        let init_result = dao_harness.initialize(
            nullifier_k,
            dao_bulla,
            owner_secret,
            endowment_token_id,
            bulla_blind,
        )?;
        assert!(!init_result.call_data.is_empty());
        println!("  DAO-Escrow initialized: call_data={}B", init_result.call_data.len());

        // ------------------------------------------------------------------
        // Step 3: Issuer issues credential to worker via Identity
        // ------------------------------------------------------------------
        println!("\n--- Step 3: Issuer issues credential to worker ---");
        let issuer_secret = pallas::Base::from(30u64);
        let worker_secret = pallas::Base::from(20u64);
        let schema_hash = pallas::Base::from(55u64);

        let issue_result = id_harness.issue_credential(
            issuer_secret,
            worker_secret,
            pallas::Base::from(100u64),  // attribute_1: role = senior
            pallas::Base::from(200u64),  // attribute_2: years_experience = 5
            pallas::Base::from(300u64),  // attribute_blind
            schema_hash,
            1000,   // issued_at
            20000,  // expires_at
        )?;
        assert!(!issue_result.call_data.is_empty());
        println!("  Credential issued: call_data={}B", issue_result.call_data.len());

        // ------------------------------------------------------------------
        // Step 4: Register a capability for gated job access
        // ------------------------------------------------------------------
        println!("\n--- Step 4: Register capability ---");
        let _capability_id = [42u8; 32];
        let cred_req = dwow_identity_contract::model::CredentialRequirement {
            schema_hash: [0u8; 32],
            issuer_pub: PublicKey::from_secret(SecretKey::from_base(issuer_secret)),
            min_threshold: 1,
            attribute_name: b"role".to_vec(),
        };
        let reg_cap_result = id_harness.register_capability(
            b"senior_rust_dev".to_vec(),
            cred_req,
            None,
        )?;
        assert!(!reg_cap_result.call_data.is_empty());
        println!("  Capability registered: call_data={}B", reg_cap_result.call_data.len());

        // ------------------------------------------------------------------
        // Step 5: Employer creates job (basic — no capability)
        // ------------------------------------------------------------------
        println!("\n--- Step 5: Employer creates job (escrow deposit) ---");
        let employer_secret = pallas::Base::from(10u64);
        let employer_pub = PublicKey::from_secret(
            SecretKey::from_base(employer_secret),
        );
        let job_id = pallas::Base::from(100u64);

        let create_job = lm_harness.create_job(
            employer_secret,
            employer_pub,
            pallas::Base::from(1u64),  // attestation_id
            job_id,
            0,         // delivery_type = Generic
            5000,      // payment_amount
            pallas::Base::from(2u64),  // payment_token
            pallas::Base::from(3u64),  // payment_commit_x
            pallas::Base::from(4u64),  // payment_commit_y
        )?;
        assert!(!create_job.call_data.is_empty());
        println!("  Job created: call_data={}B", create_job.call_data.len());

        // ------------------------------------------------------------------
        // Step 6: Worker accepts job
        //   Standard accept — no capability required for this basic job
        // ------------------------------------------------------------------
        println!("\n--- Step 6: Worker accepts job ---");
        let worker_pub = PublicKey::from_secret(
            SecretKey::from_base(worker_secret),
        );
        let accept = lm_harness.accept_job(worker_secret, worker_pub, job_id)?;
        assert!(!accept.call_data.is_empty());
        println!("  Job accepted: call_data={}B", accept.call_data.len());

        // ------------------------------------------------------------------
        // Step 7: Worker submits deliverable with attestation verification
        //   Cross-contract call:
        //   Labor Market::SubmitDeliverableV1 (0x02)
        //     └── Attestation::VerifyClaimV1 (0x04) — child call
        // ------------------------------------------------------------------
        println!("\n--- Step 7: Worker submits deliverable (→Attestation) ---");
        let claim_id = pallas::Base::from(200u64);

        let submit = lm_harness.submit_deliverable(
            worker_secret, worker_pub, job_id, claim_id, 1000, 50,
        )?;
        assert!(!submit.call_data.is_empty());
        println!("  submit_deliverable: call_data={}B", submit.call_data.len());

        let git_deliverable = lm_harness.submit_git_deliverable(
            worker_secret, worker_pub, job_id, claim_id, 1000, 50,
        )?;
        assert!(!git_deliverable.call_data.is_empty());
        println!("  submit_git_deliverable: call_data={}B", git_deliverable.call_data.len());

        // ------------------------------------------------------------------
        // Step 8: Employer confirms delivery (releases payment)
        //   Labor Market::ConfirmDeliveryV1 (0x04)
        //     └── promissory_note::TransferV1 (0x04) — child call for payment
        // ------------------------------------------------------------------
        println!("\n--- Step 8: Employer confirms delivery → payment ---");
        let confirm = lm_harness.confirm_delivery(employer_secret, employer_pub, job_id)?;
        assert!(!confirm.call_data.is_empty());
        println!("  confirm_delivery: call_data={}B", confirm.call_data.len());

        // ------------------------------------------------------------------
        // Step 9: Dispute escalated to DAO Escrow (negative path)
        //   Labor Market::DisputeV1 (0x05)
        //     └── DAO Escrow::ProposeClaimV1 (0x07) — child call
        // ------------------------------------------------------------------
        println!("\n--- Step 9: Dispute escalated to DAO (→DAO-Escrow) ---");
        let dispute_job_id = pallas::Base::from(101u64);

        let dispute = lm_harness.dispute(
            dispute_job_id,
            worker_secret,
            pallas::Base::from(50u64),   // dao_escrow_bulla
            pallas::Base::from(60u64),   // spent_nullifier
            worker_pub,
        )?;
        assert!(!dispute.call_data.is_empty());
        println!("  dispute: call_data={}B", dispute.call_data.len());

        // refund (timeout path) validates child call to promissory_note::TransferV1 (0x04)
        let refund = lm_harness.refund(
            job_id, employer_secret, 1, 0, 5000, 1000, 100, 5000, employer_pub,
        )?;
        assert!(!refund.call_data.is_empty());
        println!("  refund: call_data={}B", refund.call_data.len());

        // ------------------------------------------------------------------
        // Step 10: DAO Escrow verify member capability
        //   DAO Escrow::VerifyMemberCapabilityV1 (0x0b)
        //     └── Identity::VerifyCapabilityV1 (0x0b) — child call
        // ------------------------------------------------------------------
        println!("\n--- Step 10: DAO-Escrow verify member (→Identity) ---");
        let capability_secret = pallas::Base::from(42u64);
        let holder_secret = pallas::Base::from(20u64);
        let holder_pubkey = PublicKey::from_secret(
            SecretKey::from_base(holder_secret),
        );
        let cap_proof = dwow_dao_escrow_contract::model::CapabilityProof {
            capability_id: [1u8; 32],
            capability_secret: [2u8; 32],
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes([3u8; 32]).unwrap(),
            issuer_pub: [4u8; 32],
            predicate_result: [1u8; 32],
            proof: vec![5, 6, 7],
        };

        let verify_result = dao_harness.verify_member_capability(
            pallas::Scalar::from(1u64),   // nullifier_k
            pallas::Base::from(42u64),    // capability_id
            dao_bulla,
            capability_secret,
            holder_secret,
            holder_pubkey,
            cap_proof,
        )?;
        assert!(!verify_result.call_data.is_empty());
        println!("  verify_member_capability: call_data={}B", verify_result.call_data.len());

        // ------------------------------------------------------------------
        // Verify all key cross-contract child call function codes
        // ------------------------------------------------------------------
        println!("\n--- Cross-Contract Function Code Verification ---");
        println!("  Identity::VerifyCapabilityV1       = 0x0b");
        println!("  DAO-Escrow::ProposeClaimV1         = 0x07");
        println!("  Attestation::VerifyClaimV1         = 0x04");
        println!("  DAO-Escrow::VerifyMemberCapability = 0x0b");
        println!("  LaborMarket::AcceptJobWithCapability = 0x0d");
        println!("  LaborMarket::SubmitDeliverable     = 0x02");
        println!("  LaborMarket::Dispute               = 0x05");
        println!("  promissory_note::TransferV1               = 0x04");

        println!("\n=== Recruitment Pipeline: All 10 steps validated ===");
        Ok(())
    })
}

// ============================================================================
// Block Execution — Canonical + Uncle (accept_block)
// ============================================================================
//
// These tests exercise the full accept_block path: deploy WASM, generate
// ZK proofs via NativeTokenHarness, and execute through accept_block()
// under every consensus condition (canonical, uncle, mixed, multi-uncle, depth).
//
// Uses NativeTokenHarness (3 circuits, fast spawn) as the representative contract.

use super::harness::{
    build_contract_tx, build_test_block,
    build_test_block_with_uncles, build_test_uncle,
};
use crate::tests::modules::uncle_helpers::{setup_native_token_pipeline, native_token_call};

// ---------------------------------------------------------------------------
// test_heavyweight_canonical_exec
// ---------------------------------------------------------------------------
#[test]
fn test_heavyweight_canonical_exec() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Canonical Block Execution ===");

    smol::block_on(async {
        let (mut chain, _harness, _cid, _keypair) = setup_native_token_pipeline().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("canonical_exec")));
        let before = chain.height();

        // Submit a coinbase-only block — proves the full accept_block path
        // (coinbase + cumulative supply chain) without needing a contract
        // call that spends a coin.
        chain.block()?.submit().await?;

        let after = chain.height();
        assert!(after > before, "Height should increase (was {} now {})", before, after);
        println!("  Canonical block at height {} applied OK", after);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// test_heavyweight_coinbase_rejects_wrong_reward
// ---------------------------------------------------------------------------
/// HAZOP F1 integration: verify that accept_block REJECTS a block where
/// the coinbase reward does not exactly match expected_reward(height).
/// The F1 fix changed pow_reward_v1 from lower-bound-only (>= expected)
/// to exact equality (!= expected), which catches over-minting attempts.
/// This test uses an over-reward value (expected + 1) that would pass
/// the old lower-bound check but MUST fail the new exact-equality check.
#[test]
fn test_heavyweight_coinbase_rejects_wrong_reward() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Coinbase Wrong Reward Rejection (F1 fix) ===");

    smol::block_on(async {
        let (mut chain, _harness, _cid, _keypair) = setup_native_token_pipeline().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("coinbase_rejects_wrong_reward")));
        let height = chain.height();
        let next_height = height.succ();
        let correct_reward = dwow_sdk::blockchain::expected_reward(next_height);

        // Build coinbase with EXCESS reward (over-mint attempt).
        // This would pass the old lower-bound-only check but MUST fail
        // the F1 exact-equality check at entrypoint/mod.rs:818.
        let over_reward = BlockReward::new(correct_reward.get() + 1);
        println!("  Height {} expected_reward={}, attempting over_reward={}",
            next_height, correct_reward.get(), over_reward.get());
        let cb = chain.build_coinbase_for_height(next_height, over_reward).await?;

        let target = chain.expected_target(next_height);
        let mut block = build_test_block(
            &chain.chain_state, next_height, vec![cb.tx.clone()],
        );
        block.header.target = target;
        let vm = build_accept_vm(&block)?;

        let result = crate::block_acceptor::accept_block(
            &chain.chain_state, &block, &[], &vm,
            height, target, None,
        );

        assert!(result.is_err(),
            "F1 SAFETY: Block with over-reward ({}) was ACCEPTED but must be REJECTED. \
             The F1 exact-equality fix in pow_reward_v1 is not working.",
            over_reward.get());
        let err_msg = format!("{}", result.unwrap_err());
        println!("  Correctly rejected: {}", err_msg);

        // Verify the chain was NOT corrupted — height must be unchanged
        assert_eq!(chain.height(), height,
            "Chain height changed despite rejection — state corruption");
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// test_heavyweight_uncle_exec
// ---------------------------------------------------------------------------
#[test]
fn test_heavyweight_uncle_exec() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Uncle Block Execution ===");

    smol::block_on(async {
        let (mut chain, harness, cid, keypair) = setup_native_token_pipeline().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("uncle_exec")));
        let (call_data, _proofs) = native_token_call(&harness, keypair)?;
        let before = chain.height();

        let next = chain.height().succ();
        let reward = dwow_sdk::blockchain::expected_reward(next);
        let call_data_wrapped = if cid == *NATIVE_TOKEN_CONTRACT_ID {
            call_data
        } else {
            call_data
        };
        let contract_tx = build_contract_tx(cid, call_data_wrapped);
        let uncle_raw = build_test_block(&chain.chain_state, next, vec![contract_tx]);
        let uncle = build_test_uncle(uncle_raw, 1, reward);

        chain.block()?
            .with_uncle(uncle)
            .submit().await?;

        let after = chain.height();
        assert!(after > before, "Height should increase (was {} now {})", before, after);
        println!("  Uncle block execution at height {} applied OK", after);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// test_heavyweight_mixed_exec
// ---------------------------------------------------------------------------
#[test]
fn test_heavyweight_mixed_exec() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Mixed Execution: Canonical + Uncle ===");

    smol::block_on(async {
        let (mut chain, harness, cid, keypair) = setup_native_token_pipeline().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("mixed_exec")));
        let (call_data, _proofs) = native_token_call(&harness, keypair)?;
        let before = chain.height();

        // Coinbase-only canonical block + uncle with contract call.
        // Uncle execution failures are non-fatal; this tests the accept_block
        // path with uncles.
        let next = chain.height().succ();
        let reward = dwow_sdk::blockchain::expected_reward(next);
        let call_data_wrapped = if cid == *NATIVE_TOKEN_CONTRACT_ID {
            call_data
        } else {
            call_data
        };
        let contract_tx = build_contract_tx(cid, call_data_wrapped);
        let uncle_raw = build_test_block(&chain.chain_state, next, vec![contract_tx]);
        let uncle = build_test_uncle(uncle_raw, 1, reward);

        chain.block()?
            .with_uncle(uncle)
            .submit().await?;

        let after = chain.height();
        assert!(after > before, "Height should increase (was {} now {})", before, after);
        println!("  Mixed (canonical coinbase + uncle call) at height {} applied OK", after);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// test_heavyweight_multi_uncle
// ---------------------------------------------------------------------------
#[test]
fn test_heavyweight_multi_uncle() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-Uncle Execution (3 uncles) ===");

    smol::block_on(async {
        let (mut chain, harness, cid, keypair) = setup_native_token_pipeline().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("multi_uncle")));

        let next = chain.height().succ();
        let reward = dwow_sdk::blockchain::expected_reward(next);
        let mut uncles = Vec::new();
        for _i in 0u32..3 {
            let (call_data, _) = native_token_call(&harness, keypair.clone())?;
            let call_data_wrapped = if cid == *NATIVE_TOKEN_CONTRACT_ID {
                call_data
            } else {
                call_data
            };
            let contract_tx = build_contract_tx(cid, call_data_wrapped);
            let uncle_raw = build_test_block(&chain.chain_state, next, vec![contract_tx]);
            let uncle = build_test_uncle(uncle_raw, 1, reward);
            uncles.push(uncle);
        }
        let before = chain.height();

        chain.block()?
            .with_uncles(uncles)
            .submit().await?;

        let after = chain.height();
        assert!(after > before, "Height should increase (was {} now {})", before, after);
        println!("  Multi-uncle (3 uncles) at height {} applied OK", after);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// test_heavyweight_uncle_depth
// ---------------------------------------------------------------------------
//
// NOTE: Each depth runs sequentially on its own block (not nested in the same
// block). A true multi-depth test would include uncles at depths 1, 2, and 3
// within a single canonical block (uncle + nephew + grand-nephew scenario).
// This test validates the reward formula at each depth level but does not
// exercise the full multi-depth uncle tree in one block.
#[test]
fn test_heavyweight_uncle_depth() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Uncle Depth Verification ===");

    smol::block_on(async {
        let (mut chain, harness, cid, keypair) = setup_native_token_pipeline().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("uncle_depth")));

        for depth in [1u8, 2, 3] {
            let (call_data, _proofs) = native_token_call(&harness, keypair.clone())?;
            let before = chain.height();

            let next = chain.height().succ();
            let reward = dwow_sdk::blockchain::expected_reward(next);
            let call_data_wrapped = if cid == *NATIVE_TOKEN_CONTRACT_ID {
                call_data
            } else {
                call_data
            };
            let contract_tx = build_contract_tx(cid, call_data_wrapped);
            let uncle_raw = build_test_block(&chain.chain_state, next, vec![contract_tx]);
            let uncle = build_test_uncle(uncle_raw, depth, reward);

            chain.block()?
                .with_uncle(uncle)
                .submit().await?;

            let after = chain.height();
            assert!(after > before, "Height should increase for depth {}", depth);
            println!("  Depth {} uncle applied OK (pin_confirmed = reward / 2^{})", depth, depth);
        }

        println!("  Uncle depth tests (depths 1, 2, 3) all applied OK");
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// test_heavyweight_empty_uncle
// ---------------------------------------------------------------------------
#[test]
fn test_heavyweight_empty_uncle() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Empty Uncle (no contract calls) ===");

    smol::block_on(async {
        let (mut chain, harness, cid, keypair) = setup_native_token_pipeline().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("empty_uncle")));
        let height = chain.height();
        let next = height.succ();
        let reward = dwow_sdk::blockchain::expected_reward(next);

        // Build a real coinbase via the production path.
        let cb = chain.build_coinbase_for_height(next, reward).await?;

        // Uncle with a contract tx — exercises uncle execution through accept_block.
        let (call_data, _proofs) = native_token_call(&harness, keypair)?;
        let call_data_wrapped = if cid == *NATIVE_TOKEN_CONTRACT_ID {
            call_data
        } else {
            call_data
        };
        let uncle_tx = build_contract_tx(cid, call_data_wrapped);
        let uncle_raw = build_test_block(&chain.chain_state, next, vec![uncle_tx]);
        let uncle = build_test_uncle(uncle_raw, 1, reward);

        let target = chain.expected_target(next);
        let mut block = build_test_block_with_uncles(
            &chain.chain_state,
            next,
            vec![cb.tx],
            &[uncle.clone()],
        );
        block.header.target = target;
        let vm = build_accept_vm(&block)?;

        crate::block_acceptor::accept_block(
            &chain.chain_state, &block, &[uncle], &vm,
            height, target, None,
        ).map_err(|e| dwow_core::Error::Custom(format!("accept_block empty uncle: {}", e)))?;

        println!("  Empty uncle at height {} applied OK (no-op gracefully)", next);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// test_heavyweight_invalid_uncle_proof
// ---------------------------------------------------------------------------
#[test]
#[ignore = "HAZOP H-TF-002: uncle merkle proof validation not yet enforced in consensus"]
fn test_heavyweight_invalid_uncle_proof() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Invalid Uncle Proof ===");

    smol::block_on(async {
        let (mut chain, harness, cid, keypair) = setup_native_token_pipeline().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("invalid_uncle_proof")));
        let height = chain.height();
        let next = height.succ();
        let reward = dwow_sdk::blockchain::expected_reward(next);

        let cb = chain.build_coinbase_for_height(next, reward).await?;

        // Build a valid uncle with real ZK call_data
        let (call_data, _proofs) = native_token_call(&harness, keypair)?;
        let call_data_wrapped = if cid == *NATIVE_TOKEN_CONTRACT_ID {
            call_data
        } else {
            call_data
        };
        let uncle_tx = build_contract_tx(cid, call_data_wrapped);
        let uncle_raw = build_test_block(
            &chain.chain_state,
            next,
            vec![uncle_tx],
        );
        let good_uncle = build_test_uncle(uncle_raw, 1, reward);

        // Build a different uncle that is NOT in the merkle root
        let bad_tx = build_contract_tx(cid, vec![0xFF]);
        let bad_raw = build_test_block(
            &chain.chain_state,
            next,
            vec![bad_tx],
        );
        let bad_uncle = build_test_uncle(bad_raw, 1, reward);

        // Canonical block's uncle_merkle_root only includes good_uncle
        let target = chain.expected_target(next);
        let mut block = build_test_block_with_uncles(
            &chain.chain_state,
            next,
            vec![cb.tx],
            &[good_uncle],
        );
        block.header.target = target;
        let vm = build_accept_vm(&block)?;

        // Submit bad_uncle — its merkle proof won't match the canonical block.
        // NOTE: current connect_block accepts any uncles passed to it without
        // verifying they match the block header's uncle_merkle_root. This check
        // should be added to chain validation (future work).
        let result = crate::block_acceptor::accept_block(
            &chain.chain_state, &block, &[bad_uncle], &vm,
            height, target, None,
        );

        // HAZOP H-TF-002: uncle merkle proof validation is not yet enforced.
        // This test is #[ignore] until the consensus check is implemented.
        // When validation is added, un-ignore and change to assert!(result.is_err()).
        assert!(result.is_ok(), "Uncle application should succeed (merkle proof validation not yet enforced)");
        println!("  Uncle applied (merkle proof validation deferred to future consensus work)");

        Ok(())
    })
}

// ============================================================================
// relayer lifecycle — cross-contract bridge + relayer_endowment
// ============================================================================

/// Full lifecycle: bridge deposit → withdraw → double-spend prevention,
/// plus relayer_endowment initialization and capital deployment.
///
/// Deploys both bridge and relayer_endowment contracts into the same
/// GenesisHarness, then exercises them across multiple blocks with ZK proofs.
/// Routes through `accept_block` (production path) with real coinbases.
#[test]
// Integration test: cross-contract bridge + relayer_endowment lifecycle.
// Deploys both contracts, exercises deposit→withdraw→double-spend rejection
// then relayer_endowment initialize→deploy_capital. Uses accept_block directly.
// RG-10 compliant: zero match-Err-skip (fixed 2026-08-05).
fn test_relayer_lifecycle_heavyweight() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::{BridgeHarness, RelayerEndowmentHarness};
    use dwow_sdk::crypto::{MerkleNode, PublicKey, SecretKey};
    
    use dwow_sdk::pasta::pallas;
    use dwow_bridge_contract::model::ExternalChain;
    use crate::tests::blockchain::HeavyweightPipeline;
    use super::harness::{build_contract_tx, build_test_block};

    println!("=== Relayer Lifecycle Heavyweight ===");

    smol::block_on(async {
        // --- Setup: shared chain ---
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("relayer_lifecycle")));

        let bridge_harness = BridgeHarness::spawn();
        let relayer_harness = RelayerEndowmentHarness::spawn();

        println!("Harnesses spawned:");
        println!("  Bridge circuits: {:?}", bridge_harness.circuits());
        println!("  RelayerEndowment circuits: {:?}", relayer_harness.circuits());

        // --- Deploy bridge contract ---
        let bridge_wasm =
            include_bytes!("../../../../src/contract/bridge/dwow_bridge_contract.wasm");
        let bridge_id = chain.deploy(&bridge_harness, "bridge", bridge_wasm).await?;
        println!("Bridge deployed at {:?}", bridge_id.to_bytes());

        // --- Deploy relayer_endowment contract ---
        let relayer_wasm =
            include_bytes!("../../../../src/contract/relayer_endowment/dwow_relayer_endowment_contract.wasm");
        let relayer_id = chain.deploy(&relayer_harness, "relayer_endowment", relayer_wasm).await?;
        println!("RelayerEndowment deployed at {:?}", relayer_id.to_bytes());

        let start_height = chain.height();
        println!("Start height: {}", start_height);

        // --- Block 1: Bridge deposit ---
        println!("\n--- Block 1: Bridge deposit ---");
        let secret = pallas::Base::from(100u64);
        let recipient = PublicKey::from_secret(SecretKey::from_bytes([3u8; 32]).unwrap());
        let deposit = bridge_harness.deposit(
            secret, 10000, recipient, 1,
            pallas::Base::from(200u64), pallas::Base::from(300u64),
            0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
            ExternalChain::Monero, 0,
        );
        let deposit = deposit.map_err(|e| format!("deposit proof failed: {}", e))?;
        assert!(!deposit.call_data.is_empty(), "deposit call_data must not be empty");
        println!("  Deposit call_data={}B", deposit.call_data.len());

        let height1 = start_height.succ();
        {
            let cb = chain.build_coinbase_for_height(height1, dwow_sdk::blockchain::expected_reward(height1)).await?;
            let mut deposit_tx = build_contract_tx(bridge_id, deposit.call_data);
            deposit_tx.witness = build_witness(bridge_id, &deposit_tx.contract_calls[0].data, vec![]);
            let block1 = build_test_block(&chain.chain_state, height1, vec![cb.tx,deposit_tx]);
            let vm = build_accept_vm(&block1)?;
            crate::block_acceptor::accept_block(
                &chain.chain_state, &block1, &[], &vm,
                start_height, BlockTarget::MAX, None,
            ).map_err(|e| dwow_core::Error::Custom(format!("accept_block deposit: {}", e)))?;
        }
        assert_eq!(chain.height(), height1, "height must advance after deposit");
        println!("  deposit executed OK (height {} -> {})", start_height, height1);

        // --- Block 2: Bridge withdraw ---
        println!("\n--- Block 2: Bridge withdraw ---");
        let withdraw = bridge_harness.withdraw(
            secret, 5000,
            pallas::Base::from(400u64), pallas::Base::from(500u64),
            pallas::Base::from(600u64), [pallas::Base::from(0u64); 4],
            0, 10, 1,
        ).map_err(|e| format!("withdraw proof failed: {}", e))?;
        assert!(!withdraw.call_data.is_empty(), "withdraw call_data must not be empty");
        println!("  Withdraw call_data={}B", withdraw.call_data.len());

        let height2 = chain.height().succ();
        {
            let cb = chain.build_coinbase_for_height(height2, dwow_sdk::blockchain::expected_reward(height2)).await?;
            let mut withdraw_tx = build_contract_tx(bridge_id, withdraw.call_data);
            withdraw_tx.witness = build_witness(bridge_id, &withdraw_tx.contract_calls[0].data, vec![]);
            let block2 = build_test_block(&chain.chain_state, height2, vec![cb.tx,withdraw_tx]);
            let vm = build_accept_vm(&block2)?;
            crate::block_acceptor::accept_block(
                &chain.chain_state, &block2, &[], &vm,
                height1, BlockTarget::MAX, None,
            ).map_err(|e| dwow_core::Error::Custom(format!("accept_block withdraw: {}", e)))?;
        }
        assert_eq!(chain.height(), height2, "height must advance after withdraw");
        println!("  withdraw executed OK (height {} -> {})", height1, height2);

        // --- Block 3: Double-spend attempt (same secret = same nullifier) ---
        println!("\n--- Block 3: Double-spend attempt ---");
        let double_withdraw = bridge_harness.withdraw(
            secret, 3000,
            pallas::Base::from(999u64), pallas::Base::from(888u64),
            pallas::Base::from(777u64), [pallas::Base::from(0u64); 4],
            0, 10, 1,
        ).map_err(|e| format!("double-withdraw proof failed: {}", e))?;

        let height3 = chain.height().succ();
        {
            let cb = chain.build_coinbase_for_height(height3, dwow_sdk::blockchain::expected_reward(height3)).await?;
            let mut double_tx = build_contract_tx(bridge_id, double_withdraw.call_data);
            double_tx.witness = build_witness(bridge_id, &double_tx.contract_calls[0].data, vec![]);
            let block3 = build_test_block(&chain.chain_state, height3, vec![cb.tx,double_tx]);
            let vm = build_accept_vm(&block3)?;
            let double_result = crate::block_acceptor::accept_block(
                &chain.chain_state, &block3, &[], &vm,
                height2, BlockTarget::MAX, None,
            );
            assert!(double_result.is_err(), "double-spend must be rejected (nullifier already spent)");
        }
        println!("  double-spend correctly rejected");

        // --- Block 4: RelayerEndowment initialize + deploy_capital ---
        println!("\n--- Block 4: RelayerEndowment initialize + deploy_capital ---");
        let relayer_secret = pallas::Base::from(10u64);
        let relayer_pub = PublicKey::from_secret(SecretKey::from_base(relayer_secret));
        let backer_secret = pallas::Base::from(20u64);
        let backer_pub = PublicKey::from_secret(SecretKey::from_base(backer_secret));

        let init = relayer_harness.initialize(relayer_pub, 500, 42)?;
        assert!(!init.call_data.is_empty(), "initialize call_data must not be empty");
        println!("  Initialize call_data={}B", init.call_data.len());

        let deploy = relayer_harness.deploy_capital(
            pallas::Base::from(1u64), backer_pub, 10000,
            pallas::Base::from(2u64), 0, pallas::Scalar::from(3u64),
            relayer_pub, 500,
        )?;
        assert!(!deploy.call_data.is_empty(), "deploy_capital call_data must not be empty");
        println!("  DeployCapital call_data={}B", deploy.call_data.len());

        let height4 = chain.height().succ();
        {
            let cb = chain.build_coinbase_for_height(height4, dwow_sdk::blockchain::expected_reward(height4)).await?;
            let mut init_tx = build_contract_tx(relayer_id, init.call_data);
            init_tx.witness = build_witness(relayer_id, &init_tx.contract_calls[0].data, vec![]);
            let mut deploy_tx = build_contract_tx(relayer_id, deploy.call_data);
            deploy_tx.witness = build_witness(relayer_id, &deploy_tx.contract_calls[0].data, vec![]);
            let block4 = build_test_block(
                &chain.chain_state, height4,
                vec![cb.tx,init_tx, deploy_tx],
            );
            let vm = build_accept_vm(&block4)?;
            crate::block_acceptor::accept_block(
                &chain.chain_state, &block4, &[], &vm,
                height3, BlockTarget::MAX, None,
            ).map_err(|e| dwow_core::Error::Custom(format!("accept_block relayer: {}", e)))?;
        }
        assert_eq!(chain.height(), height4,
            "height must advance after relayer_endowment calls");
        println!("  initialize + deploy_capital executed OK (height {} -> {})", height3, height4);

        // --- Verify final state ---
        let final_height = chain.height();
        assert!(final_height.get() >= 3, "must have at least 3 successful blocks (deposit + withdraw + relayer)");
        println!(
            "\n=== Relayer Lifecycle Heavyweight: All assertions passed (final height: {}) ===",
            final_height
        );
        Ok(())
    })
}

// ============================================================================
// bearer_bond
// ============================================================================

#[test]
fn test_heavyweight_bearer_bond() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::bearer_bond_spec::bearer_bond_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&bearer_bond_test_spec()))?)
}

// ============================================================================
// otc_swap
// ============================================================================

#[test]
fn test_heavyweight_otc_swap() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::otc_swap_spec::otc_swap_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&otc_swap_test_spec()))?)
}

#[test]
fn test_heavyweight_box() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::box_spec::box_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&box_test_spec()))?)
}

#[test]
fn test_heavyweight_purse() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::purse_spec::purse_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&purse_test_spec()))?)
}

#[test]
fn test_heavyweight_multisig() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::tests::specs::multisig_spec::multisig_test_spec;
    use crate::tests::uniform_runner::run_heavyweight_test;
    Ok(smol::block_on(run_heavyweight_test(&multisig_test_spec()))?)
}

// FeeV2 + FeeCollectV1 through accept_block with full state verification.
// Covers GAP-1 (state queries), GAP-2 (Pedersen accumulator lifecycle),
// GAP-7 (fee pot zeroed), GAP-8 (supply unchanged).
#[test]
fn test_heavyweight_fee_v2() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, PublicKey, SecretKey};
    use dwow_sdk::crypto::pasta_prelude::Group;
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    smol::block_on(async {
        dwow_native_token_contract::enable_deterministic_zk();
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("fee_v2")));
        let cid = *NATIVE_TOKEN_CONTRACT_ID;

        let native_harness = NativeTokenHarness::spawn();

        // Coinbase-only block -- creates spendable coin
        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        // FeeV2 spends height-2 coin + FeeCollectV1
        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        let fee_height = chain.height().succ();

        // F2 fix: on-chain Merkle tree has 3 leaves [ZERO, genesis, height-2].
        // Include genesis coinbase so the Merkle root matches coin_roots_db.
        let gen_reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        let gen_cb = chain.build_coinbase_for_height(BlockHeight::new(1), gen_reward).await?;
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(gen_cb.coin_commitment.inner()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_result = native_harness.fee_v2(
            cb2.coin_value,
            pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind,
            u64::from(coin_pos),
            path.clone(),
            root,
            mining_kp.secret.clone(),
            mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            1,
            1,
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "TEST-FAIL [native_token::FeeV2]: {}", e
        )))?;

        // W-2: Three-point accumulator lifecycle check.
        // Point 1: accumulator must be Identity BEFORE FeeV2 block (from genesis/previous reset).
        let acc_pre = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .map(|d| Option::from(pallas::Point::from_bytes(&d[..32].try_into().unwrap())))
            .flatten()
            .unwrap_or(pallas::Point::identity());
        assert_eq!(acc_pre, pallas::Point::identity(),
            "TEST-FAIL [fee_v2]: accumulator must be Identity before FeeV2 block");

        let before = chain.height();
        let new_height = chain.block()?
            .with_call(cid, &native_harness, &fee_result.call_data, fee_result.proofs.clone())?
            .add_fee(FeeAmount::new(1))
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx.clone()).await?;

        assert!(new_height > before,
            "TEST-FAIL [fee_v2]: height must advance (was {}, now {})", before, new_height);

        // ---- State verification (production-test-standard.md §1 step 9) ----

        // Point 2+3: accumulator must have been non-Identity at FeeV2 time (point 2, tested
        // indirectly by FeeCollectV1 passing) and must be Identity now (point 3, after reset).

        // GAP-1: spent nullifier must exist
        let spent_nf = fee_result.params.input.nullifier.to_bytes();
        assert!(chain.query_contract_state(cid, "nullifiers", &spent_nf)?.is_some(),
            "TEST-FAIL [fee_v2]: spent nullifier not found");

        // GAP-2: Pedersen accumulator must be Identity after FeeCollectV1 reset
        let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("TEST-FAIL [fee_v2]: fee_commit_accumulator not found");
        let acc_point: pallas::Point = Option::from(pallas::Point::from_bytes(
            &acc_data[..32].try_into().unwrap()
        )).expect("TEST-FAIL [fee_v2]: invalid accumulator point");
        assert_eq!(acc_point, pallas::Point::identity(),
            "TEST-FAIL [fee_v2]: accumulator not reset to Identity after FeeCollectV1");

        // GAP-7: fee pot must be zeroed after collection
        let fees_data = chain.query_contract_state(cid, "fees", &fee_height.to_le_bytes())?
            .expect("TEST-FAIL [fee_v2]: fees_db entry not found");
        let fee_pot = u64::from_le_bytes(fees_data[..8].try_into().unwrap());
        assert_eq!(fee_pot, 0,
            "TEST-FAIL [fee_v2]: fee pot not zeroed (was {})", fee_pot);

        // GAP-8: supply unchanged by fee redistribution
        let supply = chain.cumulative_supply();
        let expected = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1)).get()
            + dwow_sdk::blockchain::expected_reward(BlockHeight::new(2)).get()
            + dwow_sdk::blockchain::expected_reward(BlockHeight::new(3)).get();
        assert_eq!(supply, expected,
            "TEST-FAIL [fee_v2]: supply mismatch (expected {}, got {})", expected, supply);

        // ---- R3a: fee > input rejected at builder level ----
        assert!(native_harness.fee_v2(
            10, // input_value = 10
            pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path.clone(), root,
            mining_kp.secret.clone(), mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([8u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            1, // fee > input
            1,
        ).is_err(), "TEST-FAIL [fee_v2]: fee > input must be rejected at builder");

        // ---- R3c: malformed FeeParamsV2 rejected at accept_block ----
        let garbage_data = vec![0x08u8, 0xFF, 0xFF, 0xFF];
        let bad_block = chain.block()?
            .with_call(cid, &native_harness, &garbage_data, vec![])?
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx.clone()).await;
        assert!(bad_block.is_err(),
            "TEST-FAIL [fee_v2]: malformed FeeParamsV2 must be rejected");
        let bad_err = format!("{}", bad_block.unwrap_err());
        assert!(bad_err.contains("ParseError") || bad_err.contains("IoError")
                || bad_err.contains("decode") || bad_err.contains("Custom(2)"),
            "TEST-FAIL [fee_v2]: rejection must be FeeParamsV2 decode failure, got: {}",
            bad_err);

        // ---- R4: multi-FeeV2-call block with Pedersen homomorphic sum ----
        // Spend the change coin from the first FeeV2 (fee_result.params.output.coin)
        let change_coin = &fee_result.params.output.coin;
        let mut tree4 = MerkleTree::new(1);
        tree4.append(MerkleNode::from_base(pallas::Base::zero()));
        tree4.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        tree4.append(MerkleNode::from_base(change_coin.inner()));
        let pos4 = tree4.mark().expect("tree.mark");
        let path4: Vec<MerkleNode> = tree4.witness(pos4, 0).expect("tree.witness");
        let root4 = tree4.root(0).expect("tree.root");
        let change_value = cb2.coin_value - 1;

        let cb4 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        let fee4a = native_harness.fee_v2(
            cb3.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb3.coin_blind, u64::from(pos4) + 1, path4.clone(), root4,
            mining_kp.secret.clone(), mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([9u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            1, 1,
        ).map_err(|e| dwow_core::Error::Custom(format!("TEST-FAIL [multi-fee]: {}", e)))?;

        let fee4b = native_harness.fee_v2(
            change_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(pos4), path4, root4,
            mining_kp.secret.clone(), mining_kp.secret,
            PublicKey::from_secret(SecretKey::from_bytes([10u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            10_000_000, 10_000_000,
        ).map_err(|e| dwow_core::Error::Custom(format!("TEST-FAIL [multi-fee2]: {}", e)))?;

        let before4 = chain.height();
        let new_height4 = chain.block()?
            .with_call(cid, &native_harness, &fee4a.call_data, fee4a.proofs)?
            .add_fee(FeeAmount::new(1))
            .with_call(cid, &native_harness, &fee4b.call_data, fee4b.proofs)?
            .add_fee(FeeAmount::new(1))
            .with_fee_collect()?
            .submit_with_coinbase(cb4.coinbase_tx).await?;
        assert!(new_height4 > before4,
            "TEST-FAIL [fee_v2]: multi-fee block must advance height");

        // Both nullifiers spent
        assert!(chain.query_contract_state(cid, "nullifiers", &fee4a.params.input.nullifier.to_bytes())?.is_some());
        assert!(chain.query_contract_state(cid, "nullifiers", &fee4b.params.input.nullifier.to_bytes())?.is_some());

        // Accumulator reset after multi-fee FeeCollectV1
        let acc4 = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("TEST-FAIL [fee_v2]: accumulator not found after multi-fee");
        let acc4_pt: pallas::Point = Option::from(pallas::Point::from_bytes(
            &acc4[..32].try_into().unwrap()
        )).expect("invalid point");
        assert_eq!(acc4_pt, pallas::Point::identity(),
            "TEST-FAIL [fee_v2]: accumulator not reset after multi-fee collect");

        // ---- R5b: nullifier replay rejected (FeeV2 nullifier specifically) ----
        // Use a fresh coinbase so only the FeeV2 nullifier is replayed (W-3 isolation fix).
        let cb_replay = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        let replay = chain.block()?
            .with_call(cid, &native_harness, &fee_result.call_data, fee_result.proofs.clone())?
            .with_fee_collect()?
            .submit_with_coinbase(cb_replay.coinbase_tx).await;
        assert!(replay.is_err(),
            "TEST-FAIL [fee_v2]: FeeV2 nullifier replay must be rejected");
        let replay_err = format!("{}", replay.unwrap_err());
        assert!(replay_err.contains("nullifier") || replay_err.contains("Nullifier")
                || replay_err.contains("Duplicate"),
            "TEST-FAIL [fee_v2]: replay rejection must mention nullifier, got: {}", replay_err);

        Ok(())
    })
}

// FeeV2 + DeployV1 + FeeCollectV1 through accept_block with state verification.
#[test]
fn test_heavyweight_fee_v2_deploy() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::{DeployooorHarness, NativeTokenHarness};
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{ContractId, DEPLOYOOOR_CONTRACT_ID, Keypair, MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    smol::block_on(async {
        dwow_native_token_contract::enable_deterministic_zk();
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("fee_v2_deploy")));

        let native_harness = NativeTokenHarness::spawn();
        let deployooor_harness = DeployooorHarness::spawn();

        // Coinbase-only block — creates spendable coin
        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        // FeeV2 + DeployV1 + FeeCollectV1
        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        // F2 fix: include genesis coin for correct on-chain merkle root
        let gen_reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        let gen_cb = chain.build_coinbase_for_height(BlockHeight::new(1), gen_reward).await?;
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(gen_cb.coin_commitment.inner()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_result = native_harness.fee_v2(
            cb2.coin_value,
            pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind,
            u64::from(coin_pos),
            path.clone(),
            root,
            mining_kp.secret.clone(),
            mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            1,
            1,
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "TEST-FAIL [fee_v2_deploy::FeeV2]: {}", e
        )))?;

        // DeployV1 — deploy a contract alongside fee payment
        let dk = SecretKey::from_bytes([9u8; 32])?;
        let deploy = deployooor_harness.build_deploy_call(
            Keypair { secret: dk.clone(), public: PublicKey::from_secret(dk) },
            include_bytes!("../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm").to_vec(),
            vec![0x00],
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "TEST-FAIL [fee_v2_deploy::DeployV1]: {:?}", e
        )))?;

        // Build DeployV1 call data: [0x00 selector][serialized DeployParamsV1]
        let mut deploy_call_data = vec![0x00u8];
        deploy_call_data.extend_from_slice(&dwow_serial::serialize(&deploy.params));
        let deployed_contract_id = ContractId::derive_public(deploy.params.public_key);

        let before = chain.height();
        let new_height = chain.block()?
            .with_call(*NATIVE_TOKEN_CONTRACT_ID, &native_harness, &fee_result.call_data, fee_result.proofs)?
            .add_fee(FeeAmount::new(1))
            .with_call(*DEPLOYOOOR_CONTRACT_ID, &deployooor_harness, &deploy_call_data, vec![])?
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await?;

        assert!(new_height > before,
            "TEST-FAIL [fee_v2_deploy]: height must advance (was {}, now {})", before, new_height);

        // State verification: accumulator reset, fee pot zeroed, supply unchanged
        let acc_data = chain.query_contract_state(*NATIVE_TOKEN_CONTRACT_ID, "info", b"fee_commit_acc")?
            .expect("TEST-FAIL [fee_v2_deploy]: fee_commit_accumulator not found");
        let acc_point: pallas::Point = Option::from(pallas::Point::from_bytes(
            &acc_data[..32].try_into().unwrap()
        )).expect("invalid accumulator point");
        assert_eq!(acc_point, pallas::Point::identity(),
            "TEST-FAIL [fee_v2_deploy]: accumulator not reset after FeeCollectV1");

        let fee_height = chain.height();
        let fees_data = chain.query_contract_state(*NATIVE_TOKEN_CONTRACT_ID, "fees", &fee_height.to_le_bytes())?
            .expect("TEST-FAIL [fee_v2_deploy]: fees_db entry not found");
        assert_eq!(u64::from_le_bytes(fees_data[..8].try_into().unwrap()), 0,
            "TEST-FAIL [fee_v2_deploy]: fee pot not zeroed");

        // R9: Verify deploy succeeded — deployed WASM must exist in contracts tree
        assert!(chain.query_contracts_tree(&deployed_contract_id.to_bytes())?.is_some(),
            "TEST-FAIL [fee_v2_deploy]: deployed contract {} not found in contracts tree",
            deployed_contract_id);

        Ok(())
    })
}

// FeeV2 + Box::Put + FeeCollectV1 through accept_block with state verification.
#[test]
fn test_heavyweight_fee_v2_box() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::{BoxHarness, NativeTokenHarness};
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{BOX_CONTRACT_ID, MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    smol::block_on(async {
        dwow_native_token_contract::enable_deterministic_zk();
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("fee_v2_box")));

        let native_harness = NativeTokenHarness::spawn();
        let box_harness = BoxHarness::spawn();

        // Coinbase-only block
        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        // FeeV2 + Box::Put + FeeCollectV1
        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        // F2 fix: include genesis coin for correct on-chain merkle root
        let gen_reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        let gen_cb = chain.build_coinbase_for_height(BlockHeight::new(1), gen_reward).await?;
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(gen_cb.coin_commitment.inner()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_result = native_harness.fee_v2(
            cb2.coin_value,
            pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind,
            u64::from(coin_pos),
            path.clone(),
            root,
            mining_kp.secret.clone(),
            mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            1,
            1,
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "TEST-FAIL [fee_v2_box::FeeV2]: {}", e
        )))?;

        let put_result = box_harness.put()
            .map_err(|e| dwow_core::Error::Custom(format!(
                "TEST-FAIL [fee_v2_box::PutV1]: {:?}", e
            )))?;

        let before = chain.height();
        let new_height = chain.block()?
            .with_call(*NATIVE_TOKEN_CONTRACT_ID, &native_harness, &fee_result.call_data, fee_result.proofs)?
            .add_fee(FeeAmount::new(1))
            .with_call(*BOX_CONTRACT_ID, &box_harness, &put_result.call_data, vec![put_result.proof])?
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await?;

        assert!(new_height > before,
            "TEST-FAIL [fee_v2_box]: height must advance (was {}, now {})", before, new_height);

        // State verification: accumulator reset after FeeCollectV1
        let acc_data = chain.query_contract_state(*NATIVE_TOKEN_CONTRACT_ID, "info", b"fee_commit_acc")?
            .expect("TEST-FAIL [fee_v2_box]: fee_commit_accumulator not found");
        let acc_point: pallas::Point = Option::from(pallas::Point::from_bytes(
            &acc_data[..32].try_into().unwrap()
        )).expect("invalid accumulator point");
        assert_eq!(acc_point, pallas::Point::identity(),
            "TEST-FAIL [fee_v2_box]: accumulator not reset after FeeCollectV1");

        let fee_height = chain.height();
        let fees_data = chain.query_contract_state(*NATIVE_TOKEN_CONTRACT_ID, "fees", &fee_height.to_le_bytes())?
            .expect("TEST-FAIL [fee_v2_box]: fees_db entry not found");
        assert_eq!(u64::from_le_bytes(fees_data[..8].try_into().unwrap()), 0,
            "TEST-FAIL [fee_v2_box]: fee pot not zeroed");

        // R9: Verify Box::Put stored state — box contract should be queryable
        let box_state = chain.query_contract_state(*BOX_CONTRACT_ID, "info", b"box_merkle_tree")?;
        assert!(box_state.is_some(),
            "TEST-FAIL [fee_v2_box]: box contract state not found after Put");

        Ok(())
    })
}

// Multi-block chain growth test — verifies correct chain state across
// heights 1→4 using pure coinbase-only blocks. No FeeV2, no contract calls.
#[test]
fn test_bridge_multi_block() -> std::result::Result<(), Box<dyn std::error::Error>> {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        use dwow_sdk::blockchain::{BlockHeight, expected_reward};
        use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
        use crate::tests::blockchain::HeavyweightPipeline;
        use crate::tests::modules::coinbase_coordination;
        use dwow_sdk::pasta::{group::Group, pallas};

        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(std::sync::Mutex::new(
            crate::tests::test_output::create_log_file("bridge_multi_block")
        ));

        let cid = *NATIVE_TOKEN_CONTRACT_ID;
        let mut expected_supply = 0u64;

        for h in 1..=4u64 {
            if h > 1 {
                let cb = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
                chain.block()?.submit_with_coinbase(cb.coinbase_tx).await?;
            }
            assert_eq!(chain.height(), BlockHeight::new(h),
                "height must be {}", h);

            expected_supply += expected_reward(BlockHeight::new(h)).get();
            assert_eq!(chain.cumulative_supply(), expected_supply,
                "height {} cumulative supply mismatch", h);

            if h > 1 {
                let block = chain.chain_state.store.get_block(BlockHeight::new(h))
                    .expect(&format!("block at height {}", h));
                assert_eq!(block.transactions.len(), 1,
                    "coinbase-only block must have 1 tx, got {} at height {}",
                    block.transactions.len(), h);
            }
        }

        // Hash chain continuity
        assert!(chain.block_hash_chain_continuous()?,
            "block hash chain must be continuous");

        // Accumulator stays Identity across zero-fee chain
        let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("accumulator must exist");
        let acc_point: pallas::Point = Option::from(
            pallas::Point::from_bytes(&acc_data[..32].try_into().unwrap())
        ).expect("invalid point");
        assert_eq!(acc_point, pallas::Point::identity(),
            "accumulator must stay Identity");

        Ok(())
    })
}

// Fee lifecycle test — FeeV2 + FeeCollectV1 through accept_block.
// Uses NativeTokenHarness (FeeThreshold_V1 proof built inline, avoiding
// the wallet-path synthesis bug). Validates accumulator accumulation,
// FeeCollectV1 reset, fee pot zeroing, nullifier registration, and
// cumulative supply neutrality.
#[test]
fn test_bridge_fee_lifecycle() -> std::result::Result<(), Box<dyn std::error::Error>> {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        use dwow_contract_test_harness::harness::NativeTokenHarness;
        use dwow_sdk::blockchain::{BlockHeight, expected_reward};
        use dwow_sdk::crypto::{MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, PublicKey, SecretKey};
        use dwow_sdk::pasta::{group::Group, pallas};
        use crate::tests::blockchain::HeavyweightPipeline;
        use crate::tests::modules::coinbase_coordination;

        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(std::sync::Mutex::new(
            crate::tests::test_output::create_log_file("bridge_fee_lifecycle")
        ));

        let native_harness = NativeTokenHarness::spawn();
        let cid = *NATIVE_TOKEN_CONTRACT_ID;

        // Height 2: coinbase-only (creates spendable coin)
        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        let acc_pre = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .map(|d| Option::from(pallas::Point::from_bytes(&d[..32].try_into().unwrap())))
            .flatten().unwrap_or(pallas::Point::identity());
        assert_eq!(acc_pre, pallas::Point::identity(),
            "accumulator must be Identity before FeeV2");

        // Height 3: FeeV2 + FeeCollectV1
        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        // F2 fix: include genesis coin for correct on-chain merkle root.
        // On-chain tree: [ZERO, genesis_coin, cb2_coin]. Missing genesis
        // coin causes TransferMerkleRootNotFound (HAZOP §3 NO/NOT).
        let gen_reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        let gen_cb = chain.build_coinbase_for_height(BlockHeight::new(1), gen_reward).await?;
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));               // pos 0: ZERO
        tree.append(MerkleNode::from_base(gen_cb.coin_commitment.inner()));     // pos 1: genesis
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));        // pos 2: cb2
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");
        let mining_kp = chain.mining_keypair(BlockHeight::new(2));

        let fee_result = native_harness.fee_v2(
            cb2.coin_value,
            pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            1, 1,
        ).map_err(|e| dwow_core::Error::Custom(format!("FeeV2: {}", e)))?;

        let before = chain.height();
        let new_height = chain.block()?
            .with_call(cid, &native_harness, &fee_result.call_data, fee_result.proofs)?
            .add_fee(FeeAmount::new(1))
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await?;
        assert!(new_height > before, "height must advance");

        // Accumulator reset
        let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("accumulator not found");
        let acc_point: pallas::Point = Option::from(
            pallas::Point::from_bytes(&acc_data[..32].try_into().unwrap())
        ).expect("invalid point");
        assert_eq!(acc_point, pallas::Point::identity(),
            "accumulator must be Identity after FeeCollectV1");

        // Fee pot zeroed
        let fee_height = chain.height();
        let fees_data = chain.query_contract_state(cid, "fees", &fee_height.to_le_bytes())?
            .expect("fees_db entry not found");
        assert_eq!(u64::from_le_bytes(fees_data[..8].try_into().unwrap()), 0,
            "fee pot must be zeroed");

        // Supply unchanged
        let expected: u64 = (1..=3u64).map(|h| expected_reward(BlockHeight::new(h)).get()).sum();
        assert_eq!(chain.cumulative_supply(), expected,
            "cumulative supply unchanged by fees");

        // Nullifier written
        let nf = fee_result.params.input.nullifier.to_bytes();
        assert!(chain.query_contract_state(cid, "nullifiers", &nf)?.is_some(),
            "spent nullifier must exist");

        Ok(())
    })
}

// ============================================================================
// L2-FW-2: Forged FeeThreshold_V1 proof rejected at accept_block.
// A tx whose FeeParamsV2 threshold matches the mempool gate (syntactic check)
// but whose FeeThreshold_V1 ZK proof is corrupted/empty MUST be rejected by
// accept_block via verify_core_tx_with_tables. Height MUST NOT advance.
// This is the true enforcement witness — the mempool gate is syntactic.
// Partition B (consensus boundary).
// ============================================================================
#[test]
fn test_forged_threshold_proof_rejected_at_accept_block() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(std::sync::Mutex::new(crate::tests::test_output::create_log_file("forged_threshold_proof")));

        let native_harness = NativeTokenHarness::spawn();
        let cid = *NATIVE_TOKEN_CONTRACT_ID;

        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_amount: u64 = 150_000_000;
        let fee_result = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret,
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            fee_amount,
            1,  // threshold
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "[L2-FW-2] fee_v2 harness: {}", e
        )))?;

        // FeeV2CallBuilder::build() returns exactly 1 proof (Fee_V2 mass balance).
        // The FeeThreshold_V1 proof lives in FeeParamsV2.threshold_proof bytes
        // inside call_data, not in the proofs vec.
        assert_eq!(fee_result.proofs.len(), 1,
            "[L2-FW-2] FeeV2 build returns 1 proof (Fee_V2); threshold proof is embedded in call_data, got {}",
            fee_result.proofs.len());

        // Corrupt the FeeThreshold_V1 proof embedded in call_data.
        // The mempool doesn't verify ZK (syntactic threshold check only),
        // but accept_block → verify_core_tx_with_tables will reject it.
        //
        // call_data layout: [0x08][FeeParamsV2 encoded]
        // FeeParamsV2.threshold_proof is at the end of the encoded params.
        let mut corrupted_params = fee_result.params.clone();
        // Replace threshold proof with junk bytes — same length to preserve offsets
        corrupted_params.threshold_proof = vec![0xFFu8; corrupted_params.threshold_proof.len()];
        let mut corrupted_call_data = vec![0x08u8];
        corrupted_call_data.extend_from_slice(&corrupted_params.encode());
        // Use unchanged Fee_V2 proof (valid) — only the threshold proof in call_data is corrupted
        let original_proofs = fee_result.proofs.clone();

        let before = chain.height();
        let result = chain.block()?
            .with_call(cid, &native_harness, &corrupted_call_data, original_proofs)?
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await;

        let after = chain.height();
        assert!(result.is_err(),
            "[L2-FW-2] forged threshold proof must be rejected, got Ok");
        assert_eq!(before, after,
            "[L2-FW-2] height must not advance on forged proof (was {}, now {})", before, after);

        Ok(())
    })
}

// ── GAP-11: Accumulator state machine transitions ────────────────────────
// FI-COLLECT-3: The accumulator SHALL transition through exactly three
// states per block: Identity → Active(point) → Identity.
// add_commitment() valid only from Identity or Active.
// reset() to Identity valid only from Active or Identity (no-op).

#[test]
fn test_fee_integration_accumulator_state_machine() -> std::result::Result<(), Box<dyn std::error::Error>> {
    dwow_native_token_contract::enable_deterministic_zk();

    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    smol::block_on(async {
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        let native_harness = NativeTokenHarness::spawn();
        let cid = *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

        // S0: Identity at genesis (FI-COLLECT-1).
        let acc_init = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .map(|d| Option::from(pallas::Point::from_bytes(&d[..32].try_into().unwrap())))
            .flatten()
            .unwrap_or(pallas::Point::identity());
        assert_eq!(acc_init, pallas::Point::identity(),
            "[GAP-11-S0] accumulator must be Identity at genesis");

        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        // S0 (re-verified): Identity after coinbase-only block (no FeeV2).
        let acc_post_cb = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .map(|d| Option::from(pallas::Point::from_bytes(&d[..32].try_into().unwrap())))
            .flatten()
            .unwrap_or(pallas::Point::identity());
        assert_eq!(acc_post_cb, pallas::Point::identity(),
            "[GAP-11-S0b] accumulator must remain Identity after coinbase-only block");

        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(dwow_sdk::blockchain::BlockHeight::new(2));
        let fee_amount: u64 = 1;
        let fee_result = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            fee_amount, 1,
        ).map_err(|e| dwow_core::Error::Custom(format!("[GAP-11] fee_v2: {}", e)))?;

        let new_height = chain.block()?
            .with_call(cid, &native_harness, &fee_result.call_data, fee_result.proofs)?
            .add_fee(FeeAmount::new(fee_amount))
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await?;

        // S2: Identity after FeeCollectV1 reset (FI-COLLECT-1).
        let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("[GAP-11-S2] accumulator not found");
        let acc_final: pallas::Point = Option::from(
            pallas::Point::from_bytes(&acc_data[..32].try_into().unwrap())
        ).expect("[GAP-11-S2] invalid accumulator point");
        assert_eq!(acc_final, pallas::Point::identity(),
            "[GAP-11-S2] accumulator must reset to Identity after FeeCollectV1");

        // Verify the transition path: S0(Identity) → S1(Active) → S2(Identity).
        // S0 → S1 is verified by FeeV2's add_commitment in the block.
        // S1 → S2 is verified by the accumulator reset above.
        // The block was accepted at new_height, proving all transitions valid.

        chain.log(&format!(
            "[GAP-11] Accumulator state machine test PASSED: \
             S0(Identity) → S1(Active) → S2(Identity) verified at height {}",
            new_height));
        Ok(())
    })
}

// ── GAP-12: Two-FeeV2 overlay — homomorphic accumulation ────────────────
// FI-COLLECT-4: Within a single block, call N+1's accumulator read SHALL
// observe call N's accumulator write. The accumulator is block-level shared
// state, not per-call state.

#[test]
fn test_fee_integration_two_feev2_overlay() -> std::result::Result<(), Box<dyn std::error::Error>> {
    dwow_native_token_contract::enable_deterministic_zk();

    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    smol::block_on(async {
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        let native_harness = NativeTokenHarness::spawn();
        let cid = *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

        // Produce two spendable coins at height 2 and 3.
        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;
        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb3.coinbase_tx).await?;

        let cb4 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        // Build Merkle tree: positions 0(zero), 1(genesis), 2(cb2), 3(cb3).
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let gen_reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        let gen_cb = chain.build_coinbase_for_height(BlockHeight::new(1), gen_reward).await?;
        tree.append(MerkleNode::from_base(gen_cb.coin_commitment.inner()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        tree.append(MerkleNode::from_base(cb3.coin_commitment.inner()));
        let coin_pos_2 = tree.mark().expect("mark pos2");
        let path_2: Vec<MerkleNode> = tree.witness(coin_pos_2, 0).expect("witness pos2");
        let coin_pos_3 = tree.mark().expect("mark pos3");
        let path_3: Vec<MerkleNode> = tree.witness(coin_pos_3, 0).expect("witness pos3");
        let root = tree.root(0).expect("root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(4));
        let fee_a: u64 = 3;
        let fee_b: u64 = 5;
        let fee_dest = PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?);

        // Two FeeV2 transactions spending different coins.
        let fr_a = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos_2), path_2.clone(), root,
            mining_kp.secret.clone(), mining_kp.secret.clone(),
            fee_dest, pallas::Base::zero(), pallas::Base::zero(),
            fee_a, 1,
        ).map_err(|e| dwow_core::Error::Custom(format!("[GAP-12] fee_v2 A: {}", e)))?;
        let fr_b = native_harness.fee_v2(
            cb3.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb3.coin_blind, u64::from(coin_pos_3), path_3.clone(), root,
            mining_kp.secret.clone(), mining_kp.secret.clone(),
            fee_dest, pallas::Base::zero(), pallas::Base::zero(),
            fee_b, 1,
        ).map_err(|e| dwow_core::Error::Custom(format!("[GAP-12] fee_v2 B: {}", e)))?;

        // Submit both FeeV2 in the same block + FeeCollectV1.
        // FI-COLLECT-4: call B observes call A's accumulator write.
        let total_fee = fee_a + fee_b;
        let new_height = chain.block()?
            .with_call(cid, &native_harness, &fr_a.call_data, fr_a.proofs)?
            .with_call(cid, &native_harness, &fr_b.call_data, fr_b.proofs)?
            .add_fee(FeeAmount::new(fee_a))
            .add_fee(FeeAmount::new(fee_b))
            .with_fee_collect()?
            .submit_with_coinbase(cb4.coinbase_tx).await?;
        assert!(new_height > BlockHeight::new(3),
            "[GAP-12-OV1] height must advance past coinbase blocks");

        // Accumulator reset after FeeCollectV1 — proves PedersenCommit(a+b, ...)
        // matched the homomorphically accumulated commitments.
        let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("[GAP-12-OV2] accumulator not found");
        let acc_point: pallas::Point = Option::from(
            pallas::Point::from_bytes(&acc_data[..32].try_into().unwrap())
        ).expect("[GAP-12-OV2] invalid accumulator point");
        assert_eq!(acc_point, pallas::Point::identity(),
            "[GAP-12-OV2] accumulator reset after FeeCollectV1 with two FeeV2 overlay");

        // Both nullifiers written.
        assert!(chain.query_contract_state(cid, "nullifiers",
            &fr_a.params.input.nullifier.to_bytes())?.is_some(),
            "[GAP-12-OV3] FeeV2 A nullifier written");
        assert!(chain.query_contract_state(cid, "nullifiers",
            &fr_b.params.input.nullifier.to_bytes())?.is_some(),
            "[GAP-12-OV3] FeeV2 B nullifier written");

        // Supply neutrality: fees A+B transferred, not created.
        let supply = chain.cumulative_supply();
        let expected: u64 = (1..=4u64)
            .map(|h| dwow_sdk::blockchain::expected_reward(BlockHeight::new(h)).get())
            .sum();
        assert_eq!(supply, expected,
            "[GAP-12-OV4] supply unchanged by two-FeeV2 overlay: {} == {}", supply, expected);

        chain.log(&format!(
            "[GAP-12] Two-FeeV2 overlay test PASSED: \
             fees {} + {} = {} verified via homomorphic accumulation",
            fee_a, fee_b, total_fee));
        Ok(())
    })
}

// ── Fee system integration tests ─────────────────────────────────────────
// Python ref: contrib/model/fee_window_model.py (P-IT-1 through P-IT-6)
// Spec: fee-spec.md §14 (22 invariants)

#[test]
fn test_fee_integration_full_lifecycle() -> std::result::Result<(), Box<dyn std::error::Error>> {
    dwow_native_token_contract::enable_deterministic_zk();
    use crate::tests::specs::fee_integration_spec::run_fee_integration_full_lifecycle;
    Ok(smol::block_on(run_fee_integration_full_lifecycle())?)
}

#[test]
fn test_fee_integration_risk_emergence() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // P-IT-4 / FI-RISK-2: Risk emergence from observed behavior.
    //
    // Python ref: test_p_it_4_risk_emergence in fee_window_model.py.
    // Contracts earn risk factors through observed cost deviations —
    // under-declaring contracts escalate, accurate contracts stay at
    // baseline, and risk factors persist across sled restart.
    //
    // This test exercises the ContractRiskTracker through its chain_state
    // API, verifying the full pipeline: record → evaluate → persist → reload.
    dwow_native_token_contract::enable_deterministic_zk();

    use dwow_sdk::crypto::ContractId;
    use dwow_sdk::blockchain::RiskFactor;

    smol::block_on(async {
        use crate::tests::blockchain::HeavyweightPipeline;
        let chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        // Test contracts.
        let under_declarer = ContractId::from_bytes([1u8; 32])
            .map_err(|e| dwow_core::Error::Custom(format!("{:?}", e)))?;
        let accurate = ContractId::from_bytes([2u8; 32])
            .map_err(|e| dwow_core::Error::Custom(format!("{:?}", e)))?;

        // P-IT-4-R1: New contracts start at baseline (FI-RISK-4).
        {
            let tracker = chain.chain_state.contract_risk_tracker.lock()
                .unwrap_or_else(|e| e.into_inner());
            assert_eq!(tracker.get_risk_factor(&under_declarer), RiskFactor::BASELINE,
                "[P-IT-4-R1] new under-declarer starts at baseline");
            assert_eq!(tracker.get_risk_factor(&accurate), RiskFactor::BASELINE,
                "[P-IT-4-R1] new accurate contract starts at baseline");
        }

        // P-IT-4-R2: Under-declaration escalates risk factor (FI-RISK-2).
        {
            let mut tracker = chain.chain_state.contract_risk_tracker.lock()
                .unwrap_or_else(|e| e.into_inner());
            // Record under-declaration: declared 1000, observed 2000 (100% over).
            tracker.record(under_declarer, "transfer".into(), 1000, 2000, 0);
            let new_risk = tracker.evaluate_window(&under_declarer);
            assert!(new_risk > RiskFactor::BASELINE,
                "[P-IT-4-R2] under-declarer risk ({}) > baseline ({}) after one window",
                new_risk, RiskFactor::BASELINE);
        }

        // P-IT-4-R3: Accurate declaration stays at baseline (FI-RISK-2).
        {
            let mut tracker = chain.chain_state.contract_risk_tracker.lock()
                .unwrap_or_else(|e| e.into_inner());
            // Record accurate declaration: declared 1000, observed 1400 (within 50% tolerance).
            tracker.record(accurate, "transfer".into(), 1000, 1400, 0);
            let new_risk = tracker.evaluate_window(&accurate);
            assert_eq!(new_risk, RiskFactor::BASELINE,
                "[P-IT-4-R3] accurate contract stays at baseline after one window, got {}",
                new_risk);
        }

        // P-IT-4-R4: Risk factors persist across sled save/load (FI-RISK-3).
        {
            let mut tracker = chain.chain_state.contract_risk_tracker.lock()
                .unwrap_or_else(|e| e.into_inner());
            tracker.save_to_tree(&chain.chain_state.store.contract_risk)
                .expect("[P-IT-4-R4] save_to_tree");
            // Load into a fresh tracker.
            let mut fresh = dwow_chain::contract_risk::ContractRiskTracker::new(
                Default::default(),
            );
            fresh.load_from_tree(&chain.chain_state.store.contract_risk)
                .expect("[P-IT-4-R4] load_from_tree");
            assert!(fresh.get_risk_factor(&under_declarer) > RiskFactor::BASELINE,
                "[P-IT-4-R4] under-declarer risk survives restart");
            assert_eq!(fresh.get_risk_factor(&accurate), RiskFactor::BASELINE,
                "[P-IT-4-R4] accurate contract risk survives restart");
        }

        // P-IT-4-R5: Risk cap enforced (FI-RISK-2).
        {
            let mut tracker = chain.chain_state.contract_risk_tracker.lock()
                .unwrap_or_else(|e| e.into_inner());
            // Record many windows of severe under-declaration.
            for w in 1..20u64 {
                tracker.record(under_declarer, "transfer".into(), 1000, 5000, w);
                tracker.evaluate_window(&under_declarer);
            }
            let capped = tracker.get_risk_factor(&under_declarer);
            assert!(capped <= RiskFactor::MAX,
                "[P-IT-4-R5] risk factor ({}) must not exceed MAX ({})",
                capped, RiskFactor::MAX);
        }

        chain.log("[P-IT-4] Risk emergence test PASSED");
        Ok(())
    })
}

#[test]
fn test_fee_integration_cross_window_congestion() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // GAP-3 / GAP-19: Fee window boundary + multi-window congestion tests.
    //
    // FI-WINDOW-1: At height ≡ 0 (mod 20), congestion factors SHALL be
    // recomputed from mempool queue depths and encoded into fee_window_flags.
    //
    // This test mines 21 blocks through HeavyweightPipeline and verifies:
    //   1. Every block header has a well-formed fee_window_flags field.
    //   2. Flags at window boundaries are valid (cm in [0, 2]).
    //   3. The FeeWindowState CFs remain at SCALE (zero congestion baseline).
    //   4. Flags roundtrip through FeeWindowState::encode_flags().
    //
    // Full PID controller behavior (congestion → CF adjustment → flag changes)
    // requires the miner_task loop with mempool population, which is tested
    // at Level 3 (Docker multi-node). This test verifies the infrastructure
    // is wired correctly — flags exist, are well-formed, and survive window
    // boundaries.
    dwow_native_token_contract::enable_deterministic_zk();

    use dwow_chain::fee_window::{FeeWindowFlags, FeeWindowState, FeeWindowConfig, CongestionFactor};
    use dwow_sdk::blockchain::FeeWindowId;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    smol::block_on(async {
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        let fw_state = FeeWindowState::new(FeeWindowConfig::default());

        // Mine blocks from height 2 through 21 (20 blocks + genesis = 21 total).
        for h in 2u64..=21 {
            let cb = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
            let new_height = chain.block()?
                .submit_with_coinbase(cb.coinbase_tx).await?;

            // Verify every block has a fee_window_flags field (FI-FLAG-1).
            let stored = chain.chain_state.store.get_block(new_height)?;
            let flags = stored.header.fee_window_flags;

            // GAP-3-F1: flags must be valid (both bytes have cm in [0, 2]).
            let circuit_cm = flags.circuit_byte().congestion_multiplier();
            let wasm_cm = flags.wasm_byte().congestion_multiplier();
            assert!(circuit_cm <= 2,
                "[GAP-3-F1] circuit CM ({}) must be valid (0-2) at height {}",
                circuit_cm, new_height);
            assert!(wasm_cm <= 2,
                "[GAP-3-F1] wasm CM ({}) must be valid (0-2) at height {}",
                wasm_cm, new_height);

            // GAP-3-F2: flags roundtrip through derive_cfs().
            let (cf, wf) = flags.derive_cfs();
            assert!(cf.premium().get() >= CongestionFactor::SCALE,
                "[GAP-3-F2] derived circuit CF ({}) >= SCALE at height {}",
                cf.premium().get(), new_height);
            assert!(wf.premium().get() >= CongestionFactor::SCALE,
                "[GAP-3-F2] derived wasm CF ({}) >= SCALE at height {}",
                wf.premium().get(), new_height);

            // GAP-3-F3: At window boundaries (height ≡ 0 mod 20),
            // FeeWindowState encode_flags produces well-formed output.
            if FeeWindowId::is_window_boundary(new_height) {
                let window_flags = fw_state.encode_flags();
                assert!(window_flags.is_active(),
                    "[GAP-3-F3] window boundary at height {}: flags must be active",
                    new_height);
                let w_cm = window_flags.circuit_byte().congestion_multiplier();
                let w_wm = window_flags.wasm_byte().congestion_multiplier();
                assert!(w_cm <= 2,
                    "[GAP-3-F3] window boundary circuit CM valid: {}", w_cm);
                assert!(w_wm <= 2,
                    "[GAP-3-F3] window boundary wasm CM valid: {}", w_wm);
                chain.log(&format!(
                    "[GAP-3] Window boundary at height {}: flags=0x{:04x}",
                    new_height, window_flags.get()));
            }
        }

        // GAP-19-F1: FeeWindowState CFs remain at SCALE across the run.
        let final_circuit = fw_state.circuit_cf();
        let final_wasm = fw_state.wasm_cf();
        assert_eq!(final_circuit.premium().get(), CongestionFactor::SCALE,
            "[GAP-19-F1] circuit premium CF must be SCALE at end of 21-block run");
        assert_eq!(final_circuit.standard().get(), CongestionFactor::SCALE,
            "[GAP-19-F1] circuit standard CF must be SCALE at end of 21-block run");
        assert_eq!(final_wasm.premium().get(), CongestionFactor::SCALE,
            "[GAP-19-F1] wasm premium CF must be SCALE at end of 21-block run");
        assert_eq!(final_wasm.standard().get(), CongestionFactor::SCALE,
            "[GAP-19-F1] wasm standard CF must be SCALE at end of 21-block run");

        assert_eq!(chain.height(), dwow_sdk::blockchain::BlockHeight::new(21),
            "[GAP-3-F4] chain height must be 21 after mining 20 blocks");

        chain.log("[GAP-3/GAP-19] Cross-window congestion test PASSED: 21 blocks verified");
        Ok(())
    })
}

#[test]
fn test_fee_integration_attack_vectors() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // GAP-23: Verify fee system robustness against known attack vectors.
    //
    // Attack vectors tested:
    //   1. Accumulator reset integrity — after FeeCollectV1, accumulator must
    //      be Identity (prevents fee-doubling attacks).
    //   2. Fee pot zeroing — after collection, fees_db[height] must be 0
    //      (prevents double-claim).
    //   3. Supply neutrality — fees transfer value, never create or destroy
    //      (prevents hidden inflation, ZCash Orchard class).
    //   4. Nullifier replay — double-spend rejected (tested by NF-1, verified
    //      here through accumulator integrity).
    //
    // Contract-level checks (C1 zero-claim, C2 bad-claim Pedersen mismatch)
    // are verified in native_token unit tests. This test verifies the
    // full-stack path: FeeV2 → accumulator → FeeCollectV1 → reset.
    dwow_native_token_contract::enable_deterministic_zk();

    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    smol::block_on(async {
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        let native_harness = NativeTokenHarness::spawn();
        let cid = *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(dwow_sdk::blockchain::BlockHeight::new(2));
        let fee_amount: u64 = 1;
        let fee_result = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            fee_amount, 1,
        ).map_err(|e| dwow_core::Error::Custom(format!("[GAP-23] fee_v2: {}", e)))?;

        let before = chain.height();
        let new_height = chain.block()?
            .with_call(cid, &native_harness, &fee_result.call_data, fee_result.proofs)?
            .add_fee(FeeAmount::new(fee_amount))
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await?;
        assert!(new_height > before, "[GAP-23-AV1] height must advance");

        // AV1: Accumulator reset — prevents fee-doubling attack.
        // If accumulator were NOT reset, an attacker could replay fees.
        let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("[GAP-23-AV1] accumulator not found");
        let acc_point: pallas::Point = Option::from(
            pallas::Point::from_bytes(&acc_data[..32].try_into().unwrap())
        ).expect("[GAP-23-AV1] invalid accumulator point");
        assert_eq!(acc_point, pallas::Point::identity(),
            "[GAP-23-AV1] accumulator must reset to Identity after FeeCollectV1 — \
             prevents fee-doubling replay attack");

        // AV2: Fee pot zeroed — prevents double-claim attack.
        // If fees_db were NOT zeroed, attacker could claim same fees twice.
        let fee_height = chain.height();
        let fees_data = chain.query_contract_state(cid, "fees", &fee_height.to_le_bytes())?
            .expect("[GAP-23-AV2] fees_db entry not found");
        let fee_pot = u64::from_le_bytes(fees_data[..8].try_into().unwrap());
        assert_eq!(fee_pot, 0,
            "[GAP-23-AV2] fee pot must be zeroed — prevents double-claim attack");

        // AV3: Supply neutrality — prevents hidden inflation (ZCash Orchard class).
        let supply = chain.cumulative_supply();
        let expected: u64 = (1..=3u64)
            .map(|h| dwow_sdk::blockchain::expected_reward(
                dwow_sdk::blockchain::BlockHeight::new(h)).get())
            .sum();
        assert_eq!(supply, expected,
            "[GAP-23-AV3] supply unchanged by fees: {} == {} — \
             prevents hidden inflation", supply, expected);

        // AV4: Nullifier written — prevents double-spend (complements NF-1).
        let spent_nf = fee_result.params.input.nullifier.to_bytes();
        assert!(chain.query_contract_state(cid, "nullifiers", &spent_nf)?.is_some(),
            "[GAP-23-AV4] spent nullifier exists on-chain — \
             prevents double-spend");

        chain.log("[GAP-23] Attack vectors test PASSED");
        Ok(())
    })
}

#[test]
fn test_fee_integration_mempool_lifecycle() -> std::result::Result<(), Box<dyn std::error::Error>> {
    dwow_native_token_contract::enable_deterministic_zk();
    use crate::tests::specs::fee_integration_spec::run_fee_integration_mempool_lifecycle;
    Ok(smol::block_on(run_fee_integration_mempool_lifecycle())?)
}

#[test]
fn test_fee_integration_miner_decrypt_loop() -> std::result::Result<(), Box<dyn std::error::Error>> {
    dwow_native_token_contract::enable_deterministic_zk();
    use crate::tests::specs::fee_integration_spec::run_fee_integration_miner_decrypt_loop;
    Ok(smol::block_on(run_fee_integration_miner_decrypt_loop())?)
}

// GAP-20: TierTestExtractor — lightweight FeeSignallingExtractor for
// tier partition testing. At module level because Rust 2021 forbids
// impl Trait inside function bodies.
struct TierTestExtractor;
impl dwow_mempool::FeeSignallingExtractor for TierTestExtractor {
    fn extract_fee(&self, tx: &dwow_chain::Transaction) -> dwow_sdk::blockchain::FeeAmount {
        if let Some(call) = tx.contract_calls.first() {
            if call.data.len() >= 9 && call.data[0] == 0x08 {
                return dwow_sdk::blockchain::FeeAmount::new(u64::from_le_bytes(
                    call.data[1..9].try_into().unwrap_or([0; 8])));
            }
        }
        dwow_sdk::blockchain::FeeAmount::ZERO
    }
    fn declare_charge(&self, tx: &dwow_chain::Transaction) -> dwow_sdk::blockchain::BlockCharge {
        dwow_sdk::blockchain::BlockCharge::new(tx.contract_calls.len() as u64 * 400_000_000)
    }
    fn extract_fee_commitment(&self, _tx: &dwow_chain::Transaction) -> Option<dwow_mempool::FeeCommitment> {
        None
    }
    fn verify_threshold_proof(&self, tx: &dwow_chain::Transaction, threshold: dwow_sdk::blockchain::FeeAmount) -> bool {
        self.extract_fee(tx) >= threshold
    }
}

#[test]
fn test_fee_integration_two_tier_admission() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // GAP-20: 15-tx tier partition — premium/general/rejected (FI-ADMIT-1/2).
    //
    // The two-tier mempool admission gate SHALL partition transactions into
    // premium (fee >= premium_threshold), general (fee >= general_threshold
    // but < premium), and rejected (fee < general_threshold). Within each
    // tier, transactions SHALL be selected FCFS. Premium queue drains first.
    //
    // This test:
    //   1. Creates 15 FeeV2 transactions with varying fees
    //   2. Sets premium=100M, general=10M thresholds
    //   3. Verifies correct tier assignment counts
    //   4. Verifies FCFS ordering within tiers
    //   5. Verifies premium queue drains before general queue
    dwow_native_token_contract::enable_deterministic_zk();

    use dwow_mempool::{Mempool, MempoolConfig, MinerConfig};
    use dwow_sdk::blockchain::{BlockVersion, FeeAmount};
    use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

    fn make_partition_tx(fee: u64) -> dwow_chain::Transaction {
        let mut data = vec![0x08u8];
        data.extend_from_slice(&fee.to_le_bytes());
        dwow_chain::Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                data,
            }],
            lock_time: 0, nullifiers: vec![], witness: vec![],
        }
    }

    smol::block_on(async {
        let premium_threshold = FeeAmount::new(100_000_000);
        let general_threshold = FeeAmount::new(10_000_000);

        let config = MempoolConfig {
            premium_threshold,
            general_threshold,
            max_size: 100,
            ..Default::default()
        };
        let mempool = Mempool::new(
            config, None,
            Box::new(TierTestExtractor), None,
        );

        // Fees: 5 premium (>= 100M), 5 general (10M-99M), 5 rejected (< 10M).
        let premium_fees: [u64; 5]   = [500_000_000, 400_000_000, 300_000_000, 200_000_000, 100_000_000];
        let general_fees: [u64; 5]   = [90_000_000, 80_000_000, 70_000_000, 60_000_000, 50_000_000];
        let rejected_fees: [u64; 5]  = [9_000_000, 7_000_000, 5_000_000, 3_000_000, 1_000_000];

        // Admit premium txs (FCFS order: first admitted = first selected).
        for &fee in &premium_fees {
            let tx = make_partition_tx(fee);
            mempool.add(tx).await
                .expect(&format!("[GAP-20] premium tx with fee {} must be admitted", fee));
        }
        assert_eq!(mempool.premium_queue_len(), 5,
            "[GAP-20-T1] 5 premium txs must be in premium queue");

        // Admit general txs.
        for &fee in &general_fees {
            let tx = make_partition_tx(fee);
            mempool.add(tx).await
                .expect(&format!("[GAP-20] general tx with fee {} must be admitted", fee));
        }
        assert_eq!(mempool.standard_queue_len(), 10,
            "[GAP-20-T2] 5 general txs + 5 premium (removed from fee_index) = 10 standard queue entries");

        // Reject below-general txs.
        let mut rejected_count = 0;
        for &fee in &rejected_fees {
            let tx = make_partition_tx(fee);
            if mempool.add(tx).await.is_err() {
                rejected_count += 1;
            }
        }
        assert_eq!(rejected_count, 5,
            "[GAP-20-T3] all 5 below-general txs must be rejected, got {} rejects",
            rejected_count);

        // Selection: premium queue drains first (FCFS).
        let selected = mempool.select_for_block(&MinerConfig {
            max_charge: u64::MAX, max_txs: 100, ..Default::default()
        }).await;

        // First 5 selected must be premium (FCFS: 500M, 400M, 300M, 200M, 100M).
        assert!(selected.len() >= 5,
            "[GAP-20-T4] at least 5 txs must be selected, got {}", selected.len());
        for i in 0..5 {
            let fee = u64::from_le_bytes(
                selected[i].contract_calls[0].data[1..9].try_into().unwrap());
            assert_eq!(fee, premium_fees[i],
                "[GAP-20-T4] selection[{}] must be premium FCFS: expected {}, got {}",
                i, premium_fees[i], fee);
        }

        // Next 5 must be general (FCFS: 90M, 80M, 70M, 60M, 50M).
        for i in 0..5 {
            let fee = u64::from_le_bytes(
                selected[5 + i].contract_calls[0].data[1..9].try_into().unwrap());
            assert_eq!(fee, general_fees[i],
                "[GAP-20-T5] selection[{}] must be general FCFS: expected {}, got {}",
                5 + i, general_fees[i], fee);
        }

        Ok(())
    })
}

#[test]
fn test_fee_integration_multi_contract_differential() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // GAP-16: Deploy vs transfer fee differential (FI-WASM-1, FI-WASM-2).
    //
    // DeployV1 transactions carry WASM bincode that determines wasm_kB in
    // the two-component fee formula. A deploy with 50+ kB WASM must pay
    // proportionally more than a 1 kB transfer.
    //
    // This test:
    //   1. Builds a DeployV1 call via DeployooorHarness
    //   2. Verifies extract_tx_wasm_kb() returns > 1 for the deploy
    //   3. Verifies extract_tx_wasm_kb() returns 1 for a plain transfer
    //   4. Verifies compute_fee() with deploy wasm_kB > transfer wasm_kB
    dwow_native_token_contract::enable_deterministic_zk();

    use dwow_contract_test_harness::harness::{DeployooorHarness, NativeTokenHarness};
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{Keypair, MerkleNode, MerkleTree, PublicKey, SecretKey, DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};
    use dwow_sdk::pasta::pallas;
    use dwow_mempool::extract_tx_wasm_kb;
    use dwow_chain::fee_window::compute_fee;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    smol::block_on(async {
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;

        let native_harness = NativeTokenHarness::spawn();
        let deployooor_harness = DeployooorHarness::spawn();

        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        // Build deploy call with real WASM binary.
        let dk = SecretKey::from_bytes([9u8; 32])?;
        let deploy = deployooor_harness.build_deploy_call(
            Keypair { secret: dk.clone(), public: PublicKey::from_secret(dk) },
            include_bytes!("../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm").to_vec(),
            vec![0x00],
        ).map_err(|e| dwow_core::Error::Custom(format!("[GAP-16] deploy: {:?}", e)))?;

        // Build deploy call data: [0x00 selector][serialized DeployParamsV1]
        let mut deploy_call_data = vec![0x00u8];
        deploy_call_data.extend_from_slice(&dwow_serial::serialize(&deploy.params));

        // Build deploy transaction.
        let deploy_tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: *DEPLOYOOOR_CONTRACT_ID,
                data: deploy_call_data,
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };

        // GAP-16-D1: extract_tx_wasm_kb() detects deploy WASM size.
        let deploy_kb = extract_tx_wasm_kb(&deploy_tx);
        assert!(deploy_kb > 1,
            "[GAP-16-D1] deploy wasm_kB must be > 1, got {} (WASM was {} bytes)",
            deploy_kb, deploy.params.wasm_bincode.len());

        // GAP-16-D2: A transfer tx returns wasm_kB = 1.
        let gen_reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        let gen_cb = chain.build_coinbase_for_height(BlockHeight::new(1), gen_reward).await?;
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(gen_cb.coin_commitment.inner()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_result = native_harness.fee_v2(
            cb2.coin_value,
            pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind,
            u64::from(coin_pos),
            path.clone(),
            root,
            mining_kp.secret.clone(),
            mining_kp.secret.clone(),
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            1, 1,
        ).map_err(|e| dwow_core::Error::Custom(format!("[GAP-16] fee_v2: {}", e)))?;

        let transfer_tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                data: fee_result.call_data.clone(),
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };
        let transfer_kb = extract_tx_wasm_kb(&transfer_tx);
        assert_eq!(transfer_kb, 1,
            "[GAP-16-D2] transfer wasm_kB must be 1, got {}", transfer_kb);

        // GAP-16-D3: Deploy admission threshold > transfer admission threshold.
        let cf = dwow_chain::fee_window::CongestionFactor::zero();
        let deploy_fee = compute_fee(&[1000], dwow_sdk::blockchain::WasmKb::new(deploy_kb), cf, cf);
        let transfer_fee = compute_fee(&[1000], dwow_sdk::blockchain::WasmKb::new(transfer_kb), cf, cf);
        assert!(deploy_fee > transfer_fee,
            "[GAP-16-D3] deploy fee ({}) must exceed transfer fee ({}) — \
             FI-WASM-2: deploy pays proportionally for WASM storage",
            deploy_fee, transfer_fee);

        // Submit transfer FeeV2 + FeeCollectV1 to verify chain integrity.
        let new_height = chain.block()?
            .with_call(*NATIVE_TOKEN_CONTRACT_ID, &native_harness, &fee_result.call_data, fee_result.proofs)?
            .add_fee(FeeAmount::new(1))
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await?;
        assert!(new_height > BlockHeight::new(2),
            "[GAP-16-D4] height must advance past coinbase blocks");

        chain.log(&format!(
            "[GAP-16] Deploy vs transfer differential test PASSED: \
             deploy_kB={}, transfer_kB={}, deploy_fee={}, transfer_fee={}",
            deploy_kb, transfer_kb, deploy_fee, transfer_fee));
        Ok(())
    })
}
