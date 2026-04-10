/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    process::Command,
};

use darkfi::{
    blockchain::BlockchainOverlayPtr,
    zk::{empty_witnesses, ProvingKey, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_dao_contract::{
    DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_ENC_COIN_NS, DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_NS,
    DAO_CONTRACT_ZKAS_EARLY_EXEC_NS, DAO_CONTRACT_ZKAS_EXEC_NS, DAO_CONTRACT_ZKAS_MINT_NS,
    DAO_CONTRACT_ZKAS_PROPOSE_INPUT_NS, DAO_CONTRACT_ZKAS_PROPOSE_MAIN_NS,
    DAO_CONTRACT_ZKAS_VOTE_INPUT_NS, DAO_CONTRACT_ZKAS_VOTE_MAIN_NS,
};
use darkfi_money_contract::{
    MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1, MONEY_CONTRACT_ZKAS_BURN_NS_V1,
    MONEY_CONTRACT_ZKAS_FEE_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
    MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
};
use darkfi_sdk::crypto::contract_id::{
    DAO_CONTRACT_ID, MONEY_CONTRACT_ID, SMART_CONTRACT_ZKAS_DB_NAME,
};
use darkfi_serial::{deserialize, serialize};

use crate::contract_graph::{resolve_dependencies, Contract};

use tracing::debug;

/// Update these if any circuits are changed.
/// Delete the existing cachefiles, and enable debug logging, you will see the new hashes.
const PKS_HASH: &str = "35ce1debf6ab12d1ec6db2b8c0c2a8a9b1fd25c2ff15c1258548923ce00f781f";
const VKS_HASH: &str = "415cb6ae64917b4dac078ac47d49408549799b71a39603d5fa4d3e6934eeece9";

/// Stablecoin contract ZK namespaces (WASM contract, ID derived at deployment)
pub const STABLECOIN_CONTRACT_ZKAS_OPEN_NS_V1: &str = "OpenPositionV1";
pub const STABLECOIN_CONTRACT_ZKAS_MINT_STABLE_NS_V1: &str = "MintStableV1";
pub const STABLECOIN_CONTRACT_ZKAS_LIQUIDATE_NS_V1: &str = "LiquidateV1";
pub const STABLECOIN_CONTRACT_ZKAS_ACCRUE_INTEREST_NS_V1: &str = "AccrueInterestV1";
pub const STABLECOIN_CONTRACT_ZKAS_GOVERNANCE_REPORT_NS_V1: &str = "GovernanceReportV1";

/// DAO-Escrow contract ZK namespaces (WASM contract, ID derived at deployment)
pub const DAO_ESCROW_ZKAS_INIT_NS: &str = "Init";
pub const DAO_ESCROW_ZKAS_PREMIUM_NS: &str = "PayPremium";

/// DEX contract ZK namespaces (WASM contract, ID derived at deployment)
pub const DEX_CONTRACT_ZKAS_CREATE_SWAP_NS_V1: &str = "CreateSwapV1";
pub const DEX_CONTRACT_ZKAS_ACCEPT_SWAP_NS_V1: &str = "AcceptSwapV1";
pub const DEX_CONTRACT_ZKAS_EXECUTE_SWAP_NS_V1: &str = "ExecuteSwapV1";
pub const DEX_CONTRACT_ZKAS_CANCEL_SWAP_NS_V1: &str = "CancelSwapV1";
pub const DEX_CONTRACT_ZKAS_EXECUTE_SWAP_SLIPPAGE_NS_V1: &str = "ExecuteSwapSlippageV1";
pub const DEX_CONTRACT_ZKAS_EXECUTE_SWAP_FEE_NS_V1: &str = "ExecuteSwapFeeV1";

/// Identity contract ZK namespaces (WASM contract, ID derived at deployment)
pub const IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_NS: &str = "CreateClaimV1";
pub const IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_L1_NS: &str = "CreateClaimV1L1";
pub const IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_DAG_NS: &str = "CreateClaimV1DAG";
pub const IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_L1_V2_NS: &str = "CreateClaimV1L1V2";
pub const IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_MULTI_NS: &str = "CreateClaimV1Multi";
pub const IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_RATIO_NS: &str = "CreateClaimV1Ratio";
pub const IDENTITY_CONTRACT_ZKAS_ISSUE_CREDENTIAL_V1_NS: &str = "IssueCredentialV1";
pub const IDENTITY_CONTRACT_ZKAS_VERIFY_CAPABILITY_V1_NS: &str = "VerifyCapabilityV1";

/// MoneyV2 contract ZK namespaces (WASM contract, ID derived at deployment)
pub const MONEY_V2_CONTRACT_ZKAS_FEE_NS_V1: &str = "Fee_V2";
pub const MONEY_V2_CONTRACT_ZKAS_MINT_NS_V1: &str = "Mint_V2";
pub const MONEY_V2_CONTRACT_ZKAS_BURN_NS_V1: &str = "Burn_V2";
pub const MONEY_V2_CONTRACT_ZKAS_TOKEN_MINT_NS_V1: &str = "TokenMint_V1";
pub const MONEY_V2_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1: &str = "AuthTokenMint_V1";

/// Auction contract ZK namespaces (WASM contract, ID derived at deployment)
pub const AUCTION_CONTRACT_ZKAS_CREATE_NS_V1: &str = "CreateAuction";
pub const AUCTION_CONTRACT_ZKAS_PLACE_BID_NS_V1: &str = "PlaceBid";
pub const AUCTION_CONTRACT_ZKAS_CLOSE_NS_V1: &str = "CloseAuction";
pub const AUCTION_CONTRACT_ZKAS_CLAIM_WINNINGS_NS_V1: &str = "ClaimWinnings";
pub const AUCTION_CONTRACT_ZKAS_SETTLE_NS_V1: &str = "SettleAuction";
pub const AUCTION_CONTRACT_ZKAS_REFUND_BID_NS_V1: &str = "RefundBid";

/// Lottery contract ZK namespaces (WASM contract, ID derived at deployment)
pub const LOTTERY_CONTRACT_ZKAS_COMMIT_TICKET_NS_V1: &str = "CommitTicket_V1";
pub const LOTTERY_CONTRACT_ZKAS_REVEAL_TICKET_NS_V1: &str = "RevealTicket_V1";

/// Slot contract ZK namespaces (WASM contract, ID derived at deployment)
pub const SLOT_CONTRACT_ZKAS_COMMIT_BET_NS_V1: &str = "CommitBet_V1";
pub const SLOT_CONTRACT_ZKAS_SETTLE_BET_NS_V1: &str = "SettleBet_V1";

/// Baccarat contract ZK namespaces (WASM contract, ID derived at deployment)
pub const BACCARAT_CONTRACT_ZKAS_COMMIT_BET_NS_V1: &str = "CommitBet_V1";
pub const BACCARAT_CONTRACT_ZKAS_SETTLE_BET_NS_V1: &str = "SettleBet_V1";

/// DarkToshi Dice contract ZK namespaces (WASM contract, ID derived at deployment)
pub const DARKTOSHI_DICE_CONTRACT_ZKAS_COMMIT_BET_NS_V1: &str = "CommitBet_V1";
pub const DARKTOSHI_DICE_CONTRACT_ZKAS_SETTLE_BET_NS_V1: &str = "SettleBet_V1";

/// Roulette contract ZK namespaces (WASM contract, ID derived at deployment)
pub const ROULETTE_CONTRACT_ZKAS_PLACE_BET_NS_V1: &str = "PlaceBet_V1";
pub const ROULETTE_CONTRACT_ZKAS_SETTLE_BET_NS_V1: &str = "SettleBet_V1";

/// Build a `PathBuf` to a cachefile
fn cache_path(typ: &str) -> Result<PathBuf> {
    let output = Command::new("git").arg("rev-parse").arg("--show-toplevel").output()?.stdout;
    let mut path = PathBuf::from(String::from_utf8(output[..output.len() - 1].to_vec())?);
    path.push("src");
    path.push("contract");
    path.push("test-harness");
    path.push(typ);
    Ok(path)
}

/// (Bincode, Namespace, VK)
pub type Vks = Vec<(Vec<u8>, String, Vec<u8>)>;
/// (Bincode, Namespace, VK)
pub type Pks = Vec<(Vec<u8>, String, Vec<u8>)>;

/// Generate or read cached PKs and VKs
pub fn get_cached_pks_and_vks() -> Result<(Pks, Vks)> {
    let pks_path = cache_path("pks.bin")?;
    let vks_path = cache_path("vks.bin")?;

    let mut pks = None;
    let mut vks = None;

    if pks_path.exists() {
        debug!("Found {pks_path:?}");
        let mut f = File::open(pks_path.clone())?;
        let mut data = vec![];
        f.read_to_end(&mut data)?;

        let known_hash = blake3::Hash::from_hex(PKS_HASH)?;
        let found_hash = blake3::hash(&data);

        debug!("Known PKS hash: {known_hash}");
        debug!("Found PKS hash: {found_hash}");

        if known_hash == found_hash {
            pks = Some(deserialize(&data)?)
        }

        drop(f);
    }

    if vks_path.exists() {
        debug!("Found {vks_path:?}");
        let mut f = File::open(vks_path.clone())?;
        let mut data = vec![];
        f.read_to_end(&mut data)?;

        let known_hash = blake3::Hash::from_hex(VKS_HASH)?;
        let found_hash = blake3::hash(&data);

        debug!("Known VKS hash: {known_hash}");
        debug!("Found VKS hash: {found_hash}");

        if known_hash == found_hash {
            vks = Some(deserialize(&data)?)
        }

        drop(f);
    }

    // Cache is correct, return
    if let (Some(pks), Some(vks)) = (pks, vks) {
        return Ok((pks, vks))
    }

    // Otherwise, build them
    let bins = vec![
        // Money
        &include_bytes!("../../money/proof/fee_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/mint_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/burn_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/token_mint_v1.zk.bin")[..],
        &include_bytes!("../../money/proof/auth_token_mint_v1.zk.bin")[..],
        // DAO
        &include_bytes!("../../dao/proof/mint.zk.bin")[..],
        &include_bytes!("../../dao/proof/propose-input.zk.bin")[..],
        &include_bytes!("../../dao/proof/propose-main.zk.bin")[..],
        &include_bytes!("../../dao/proof/vote-input.zk.bin")[..],
        &include_bytes!("../../dao/proof/vote-main.zk.bin")[..],
        &include_bytes!("../../dao/proof/exec.zk.bin")[..],
        &include_bytes!("../../dao/proof/early-exec.zk.bin")[..],
        &include_bytes!("../../dao/proof/auth-money-transfer.zk.bin")[..],
        &include_bytes!("../../dao/proof/auth-money-transfer-enc-coin.zk.bin")[..],
        // Stablecoin (WASM contract - deployed via deployooor)
        &include_bytes!("../../stablecoin/proof/open_position_v1.zk.bin")[..],
        &include_bytes!("../../stablecoin/proof/mint_stable_v1.zk.bin")[..],
        &include_bytes!("../../stablecoin/proof/liquidate_v1.zk.bin")[..],
        &include_bytes!("../../stablecoin/proof/accrue_interest_v1.zk.bin")[..],
        &include_bytes!("../../stablecoin/proof/governance_report_v1.zk.bin")[..],
        // DAO-Escrow (WASM contract - deployed via deployooor)
        &include_bytes!("../../dao_escrow/proof/init_v1.zk.bin")[..],
        &include_bytes!("../../dao_escrow/proof/pay_premium_v1.zk.bin")[..],
        // Identity (WASM contract - deployed via deployooor)
        &include_bytes!("../../identity/proof/create_claim_v1.zk.bin")[..],
        &include_bytes!("../../identity/proof/create_claim_v1_l1.zk.bin")[..],
        &include_bytes!("../../identity/proof/create_claim_v1_dag.zk.bin")[..],
        &include_bytes!("../../identity/proof/create_claim_v1_l1_v2.zk.bin")[..],
        &include_bytes!("../../identity/proof/create_claim_v1_multi.zk.bin")[..],
        &include_bytes!("../../identity/proof/create_claim_v1_ratio.zk.bin")[..],
        &include_bytes!("../../identity/proof/issue_credential_v1.zk.bin")[..],
        &include_bytes!("../../identity/proof/verify_capability_v1.zk.bin")[..],
        // DEX (WASM contract - deployed via deployooor)
        &include_bytes!("../../dex/proof/create_swap_v1.zk.bin")[..],
        &include_bytes!("../../dex/proof/accept_swap_v1.zk.bin")[..],
        &include_bytes!("../../dex/proof/execute_swap_v1.zk.bin")[..],
        &include_bytes!("../../dex/proof/cancel_swap_v1.zk.bin")[..],
        &include_bytes!("../../dex/proof/execute_swap_fee_v1.zk.bin")[..],
        &include_bytes!("../../dex/proof/execute_swap_slippage_v1.zk.bin")[..],
        // Lottery (WASM contract - deployed via deployooor)
        &include_bytes!("../../lottery/proof/commit_ticket_v1.zk.bin")[..],
        &include_bytes!("../../lottery/proof/reveal_ticket_v1.zk.bin")[..],
        // Slot (WASM contract - deployed via deployooor)
        &include_bytes!("../../slot/proof/commit_bet_v1.zk.bin")[..],
        &include_bytes!("../../slot/proof/settle_bet_v1.zk.bin")[..],
        // Baccarat (WASM contract - deployed via deployooor)
        &include_bytes!("../../baccarat/proof/commit_bet_v1.zk.bin")[..],
        &include_bytes!("../../baccarat/proof/settle_bet_v1.zk.bin")[..],
        // DarkToshi Dice (WASM contract - deployed via deployooor)
        &include_bytes!("../../darktoshi_dice/proof/commit_bet_v1.zk.bin")[..],
        &include_bytes!("../../darktoshi_dice/proof/settle_bet_v1.zk.bin")[..],
        // Roulette (WASM contract - deployed via deployooor)
        &include_bytes!("../../roulette/proof/place_bet_v1.zk.bin")[..],
        &include_bytes!("../../roulette/proof/settle_bet_v1.zk.bin")[..],
        // MoneyV2 (WASM contract - deployed via deployooor)
        &include_bytes!("../../money_v2/proof/fee_v1.zk.bin")[..],
        &include_bytes!("../../money_v2/proof/mint_v1.zk.bin")[..],
        &include_bytes!("../../money_v2/proof/burn_v1.zk.bin")[..],
        &include_bytes!("../../money_v2/proof/token_mint_v1.zk.bin")[..],
        &include_bytes!("../../money_v2/proof/auth_token_mint_v1.zk.bin")[..],
        // Auction (WASM contract - deployed via deployooor)
        &include_bytes!("../../auction/proof/create_auction_v1.zk.bin")[..],
        &include_bytes!("../../auction/proof/place_bid_v1.zk.bin")[..],
        &include_bytes!("../../auction/proof/close_auction_v1.zk.bin")[..],
        &include_bytes!("../../auction/proof/claim_winnings_v1.zk.bin")[..],
        &include_bytes!("../../auction/proof/settle_auction_v1.zk.bin")[..],
        &include_bytes!("../../auction/proof/refund_bid_v1.zk.bin")[..],
    ];

    let mut pks = vec![];
    let mut vks = vec![];

    for bincode in bins.iter() {
        let zkbin = ZkBinary::decode(bincode, false)?;
        debug!("Building PK for {}", zkbin.namespace);
        let witnesses = empty_witnesses(&zkbin)?;
        let circuit = ZkCircuit::new(witnesses, &zkbin);

        let pk = ProvingKey::build(zkbin.k, &circuit);
        let mut pk_buf = vec![];
        pk.write(&mut pk_buf)?;
        pks.push((bincode.to_vec(), zkbin.namespace.clone(), pk_buf));

        debug!("Building VK for {}", zkbin.namespace);
        let vk = VerifyingKey::build(zkbin.k, &circuit);
        let mut vk_buf = vec![];
        vk.write(&mut vk_buf)?;
        vks.push((bincode.to_vec(), zkbin.namespace.clone(), vk_buf));
    }

    debug!("Writing PKs to {pks_path:?}");
    let mut f = File::create(&pks_path)?;
    let ser = serialize(&pks);
    let hash = blake3::hash(&ser);
    debug!("{pks_path:?} {hash}");
    f.write_all(&ser)?;

    debug!("Writing VKs to {vks_path:?}");
    let mut f = File::create(&vks_path)?;
    let ser = serialize(&vks);
    let hash = blake3::hash(&ser);
    debug!("{vks_path:?} {hash}");
    f.write_all(&ser)?;

    Ok((pks, vks))
}

/// Inject cached VKs into a given blockchain database overlay
/// reference.
pub fn inject(overlay: &BlockchainOverlayPtr, vks: &Vks) -> Result<()> {
    // Grab a lock over the blockchain overlay
    let lock = overlay.lock().unwrap();
    let mut overlay = lock.overlay.lock().unwrap();

    // Derive the database names for the specific contracts
    let money_db_name = MONEY_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);
    let dao_db_name = DAO_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);

    // Ensure they are open in the overlay
    overlay.open_tree(&money_db_name, false)?;
    overlay.open_tree(&dao_db_name, false)?;

    for (bincode, namespace, vk) in vks.iter() {
        match namespace.as_str() {
            // Money contract circuits
            MONEY_CONTRACT_ZKAS_FEE_NS_V1 |
            MONEY_CONTRACT_ZKAS_MINT_NS_V1 |
            MONEY_CONTRACT_ZKAS_BURN_NS_V1 |
            MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1 |
            MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1 => {
                let key = serialize(&namespace.as_str());
                let value = serialize(&(bincode.clone(), vk.clone()));
                overlay.insert(&money_db_name, &key, &value)?;
            }

            // DAO contract circuits
            DAO_CONTRACT_ZKAS_MINT_NS |
            DAO_CONTRACT_ZKAS_VOTE_INPUT_NS |
            DAO_CONTRACT_ZKAS_VOTE_MAIN_NS |
            DAO_CONTRACT_ZKAS_PROPOSE_INPUT_NS |
            DAO_CONTRACT_ZKAS_PROPOSE_MAIN_NS |
            DAO_CONTRACT_ZKAS_EXEC_NS |
            DAO_CONTRACT_ZKAS_EARLY_EXEC_NS |
            DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_NS |
            DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_ENC_COIN_NS => {
                let key = serialize(&namespace.as_str());
                let value = serialize(&(bincode.clone(), vk.clone()));
                overlay.insert(&dao_db_name, &key, &value)?;
            }

            // Stablecoin contract circuits (WASM contract - dynamically deployed)
            // These namespaces are built but NOT injected here because stablecoin
            // is a WASM contract whose ID is derived at deployment time.
            // VK injection for stablecoin must happen after contract deployment.
            STABLECOIN_CONTRACT_ZKAS_OPEN_NS_V1 |
            STABLECOIN_CONTRACT_ZKAS_MINT_STABLE_NS_V1 |
            STABLECOIN_CONTRACT_ZKAS_LIQUIDATE_NS_V1 |
            STABLECOIN_CONTRACT_ZKAS_ACCRUE_INTEREST_NS_V1 |
            STABLECOIN_CONTRACT_ZKAS_GOVERNANCE_REPORT_NS_V1 => {
                debug!("Stablecoin ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // DAO-Escrow contract circuits (WASM contract - dynamically deployed)
            // VK injection for dao_escrow must happen after contract deployment.
            DAO_ESCROW_ZKAS_INIT_NS |
            DAO_ESCROW_ZKAS_PREMIUM_NS => {
                debug!("DAO-Escrow ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // Identity contract circuits (WASM contract - dynamically deployed)
            // VK injection for identity must happen after contract deployment.
            IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_NS |
            IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_L1_NS |
            IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_DAG_NS |
            IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_L1_V2_NS |
            IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_MULTI_NS |
            IDENTITY_CONTRACT_ZKAS_CREATE_CLAIM_V1_RATIO_NS |
            IDENTITY_CONTRACT_ZKAS_ISSUE_CREDENTIAL_V1_NS |
            IDENTITY_CONTRACT_ZKAS_VERIFY_CAPABILITY_V1_NS => {
                debug!("Identity ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // DEX contract circuits (WASM contract - dynamically deployed)
            // VK injection for DEX must happen after contract deployment.
            DEX_CONTRACT_ZKAS_CREATE_SWAP_NS_V1 |
            DEX_CONTRACT_ZKAS_ACCEPT_SWAP_NS_V1 |
            DEX_CONTRACT_ZKAS_EXECUTE_SWAP_NS_V1 |
            DEX_CONTRACT_ZKAS_CANCEL_SWAP_NS_V1 |
            DEX_CONTRACT_ZKAS_EXECUTE_SWAP_SLIPPAGE_NS_V1 |
            DEX_CONTRACT_ZKAS_EXECUTE_SWAP_FEE_NS_V1 => {
                debug!("DEX ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // MoneyV2 contract circuits (WASM contract - dynamically deployed)
            // VK injection for money_v2 must happen after contract deployment.
            MONEY_V2_CONTRACT_ZKAS_FEE_NS_V1 |
            MONEY_V2_CONTRACT_ZKAS_MINT_NS_V1 |
            MONEY_V2_CONTRACT_ZKAS_BURN_NS_V1 |
            MONEY_V2_CONTRACT_ZKAS_TOKEN_MINT_NS_V1 |
            MONEY_V2_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1 => {
                debug!("MoneyV2 ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // Auction contract circuits (WASM contract - dynamically deployed)
            // VK injection for auction must happen after contract deployment.
            AUCTION_CONTRACT_ZKAS_CREATE_NS_V1 |
            AUCTION_CONTRACT_ZKAS_PLACE_BID_NS_V1 |
            AUCTION_CONTRACT_ZKAS_CLOSE_NS_V1 |
            AUCTION_CONTRACT_ZKAS_CLAIM_WINNINGS_NS_V1 |
            AUCTION_CONTRACT_ZKAS_SETTLE_NS_V1 |
            AUCTION_CONTRACT_ZKAS_REFUND_BID_NS_V1 => {
                debug!("Auction ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // Lottery contract circuits (WASM contract - dynamically deployed)
            LOTTERY_CONTRACT_ZKAS_COMMIT_TICKET_NS_V1 |
            LOTTERY_CONTRACT_ZKAS_REVEAL_TICKET_NS_V1 => {
                debug!("Lottery ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // Slot contract circuits (WASM contract - dynamically deployed)
            SLOT_CONTRACT_ZKAS_COMMIT_BET_NS_V1 |
            SLOT_CONTRACT_ZKAS_SETTLE_BET_NS_V1 => {
                debug!("Slot ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // Baccarat contract circuits (WASM contract - dynamically deployed)
            BACCARAT_CONTRACT_ZKAS_COMMIT_BET_NS_V1 |
            BACCARAT_CONTRACT_ZKAS_SETTLE_BET_NS_V1 => {
                debug!("Baccarat ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // DarkToshi Dice contract circuits (WASM contract - dynamically deployed)
            DARKTOSHI_DICE_CONTRACT_ZKAS_COMMIT_BET_NS_V1 |
            DARKTOSHI_DICE_CONTRACT_ZKAS_SETTLE_BET_NS_V1 => {
                debug!("DarkToshi Dice ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            // Roulette contract circuits (WASM contract - dynamically deployed)
            ROULETTE_CONTRACT_ZKAS_PLACE_BET_NS_V1 |
            ROULETTE_CONTRACT_ZKAS_SETTLE_BET_NS_V1 => {
                debug!("Roulette ZK namespace {} skipped - WASM contract, injected post-deployment", namespace);
            }

            x => panic!("Found unhandled zkas namespace {x}"),
        }
    }

    Ok(())
}

/// Namespace to circuit binary mapping.
/// Returns (bincode, namespace_str) pairs for a given contract.
fn get_circuit_binaries(contract: super::contract_graph::Contract) -> Vec<(&'static [u8], &'static str)> {
    match contract {
        super::contract_graph::Contract::Money => vec![
            (&include_bytes!("../../money/proof/fee_v1.zk.bin")[..], "Fee_V1"),
            (&include_bytes!("../../money/proof/mint_v1.zk.bin")[..], "Mint_V1"),
            (&include_bytes!("../../money/proof/burn_v1.zk.bin")[..], "Burn_V1"),
            (&include_bytes!("../../money/proof/token_mint_v1.zk.bin")[..], "TokenMint_V1"),
            (&include_bytes!("../../money/proof/auth_token_mint_v1.zk.bin")[..], "AuthTokenMint_V1"),
        ],
        super::contract_graph::Contract::Dao => vec![
            (&include_bytes!("../../dao/proof/mint.zk.bin")[..], "Mint"),
            (&include_bytes!("../../dao/proof/propose-input.zk.bin")[..], "ProposeInput"),
            (&include_bytes!("../../dao/proof/propose-main.zk.bin")[..], "ProposeMain"),
            (&include_bytes!("../../dao/proof/vote-input.zk.bin")[..], "VoteInput"),
            (&include_bytes!("../../dao/proof/vote-main.zk.bin")[..], "VoteMain"),
            (&include_bytes!("../../dao/proof/exec.zk.bin")[..], "Exec"),
            (&include_bytes!("../../dao/proof/early-exec.zk.bin")[..], "EarlyExec"),
            (&include_bytes!("../../dao/proof/auth-money-transfer.zk.bin")[..], "AuthTransfer"),
            (&include_bytes!("../../dao/proof/auth-money-transfer-enc-coin.zk.bin")[..], "AuthTransferEnc"),
        ],
        super::contract_graph::Contract::Stablecoin => vec![
            (&include_bytes!("../../stablecoin/proof/open_position_v1.zk.bin")[..], "OpenPositionV1"),
            (&include_bytes!("../../stablecoin/proof/mint_stable_v1.zk.bin")[..], "MintStableV1"),
            (&include_bytes!("../../stablecoin/proof/liquidate_v1.zk.bin")[..], "LiquidateV1"),
            (&include_bytes!("../../stablecoin/proof/accrue_interest_v1.zk.bin")[..], "AccrueInterestV1"),
            (&include_bytes!("../../stablecoin/proof/governance_report_v1.zk.bin")[..], "GovernanceReportV1"),
        ],
        super::contract_graph::Contract::Identity => vec![
            (&include_bytes!("../../identity/proof/create_claim_v1.zk.bin")[..], "CreateClaimV1"),
            (&include_bytes!("../../identity/proof/create_claim_v1_l1.zk.bin")[..], "CreateClaimV1L1"),
            (&include_bytes!("../../identity/proof/create_claim_v1_dag.zk.bin")[..], "CreateClaimV1DAG"),
            (&include_bytes!("../../identity/proof/create_claim_v1_l1_v2.zk.bin")[..], "CreateClaimV1L1V2"),
            (&include_bytes!("../../identity/proof/create_claim_v1_multi.zk.bin")[..], "CreateClaimV1Multi"),
            (&include_bytes!("../../identity/proof/create_claim_v1_ratio.zk.bin")[..], "CreateClaimV1Ratio"),
            (&include_bytes!("../../identity/proof/issue_credential_v1.zk.bin")[..], "IssueCredentialV1"),
            (&include_bytes!("../../identity/proof/verify_capability_v1.zk.bin")[..], "VerifyCapabilityV1"),
        ],
        super::contract_graph::Contract::Dex => vec![
            (&include_bytes!("../../dex/proof/create_swap_v1.zk.bin")[..], "CreateSwapV1"),
            (&include_bytes!("../../dex/proof/accept_swap_v1.zk.bin")[..], "AcceptSwapV1"),
            (&include_bytes!("../../dex/proof/execute_swap_v1.zk.bin")[..], "ExecuteSwapV1"),
            (&include_bytes!("../../dex/proof/cancel_swap_v1.zk.bin")[..], "CancelSwapV1"),
            (&include_bytes!("../../dex/proof/execute_swap_fee_v1.zk.bin")[..], "ExecuteSwapFeeV1"),
            (&include_bytes!("../../dex/proof/execute_swap_slippage_v1.zk.bin")[..], "ExecuteSwapSlippageV1"),
        ],
        super::contract_graph::Contract::DaoEscrow => vec![
            (&include_bytes!("../../dao_escrow/proof/init_v1.zk.bin")[..], "Init"),
            (&include_bytes!("../../dao_escrow/proof/pay_premium_v1.zk.bin")[..], "PayPremium"),
        ],
        super::contract_graph::Contract::MoneyV2 => vec![
            (&include_bytes!("../../money_v2/proof/fee_v1.zk.bin")[..], "Fee_V2"),
            (&include_bytes!("../../money_v2/proof/mint_v1.zk.bin")[..], "Mint_V2"),
            (&include_bytes!("../../money_v2/proof/burn_v1.zk.bin")[..], "Burn_V2"),
            (&include_bytes!("../../money_v2/proof/token_mint_v1.zk.bin")[..], "TokenMint_V1"),
            (&include_bytes!("../../money_v2/proof/auth_token_mint_v1.zk.bin")[..], "AuthTokenMint_V1"),
        ],
        super::contract_graph::Contract::Auction => vec![
            (&include_bytes!("../../auction/proof/create_auction_v1.zk.bin")[..], "CreateAuction"),
            (&include_bytes!("../../auction/proof/place_bid_v1.zk.bin")[..], "PlaceBid"),
            (&include_bytes!("../../auction/proof/close_auction_v1.zk.bin")[..], "CloseAuction"),
            (&include_bytes!("../../auction/proof/claim_winnings_v1.zk.bin")[..], "ClaimWinnings"),
            (&include_bytes!("../../auction/proof/settle_auction_v1.zk.bin")[..], "SettleAuction"),
            (&include_bytes!("../../auction/proof/refund_bid_v1.zk.bin")[..], "RefundBid"),
        ],
        super::contract_graph::Contract::Lottery => vec![
            (&include_bytes!("../../lottery/proof/commit_ticket_v1.zk.bin")[..], "CommitTicket_V1"),
            (&include_bytes!("../../lottery/proof/reveal_ticket_v1.zk.bin")[..], "RevealTicket_V1"),
        ],
        super::contract_graph::Contract::Slot => vec![
            (&include_bytes!("../../slot/proof/commit_bet_v1.zk.bin")[..], "CommitBet_V1"),
            (&include_bytes!("../../slot/proof/settle_bet_v1.zk.bin")[..], "SettleBet_V1"),
        ],
        super::contract_graph::Contract::Baccarat => vec![
            (&include_bytes!("../../baccarat/proof/commit_bet_v1.zk.bin")[..], "CommitBet_V1"),
            (&include_bytes!("../../baccarat/proof/settle_bet_v1.zk.bin")[..], "SettleBet_V1"),
        ],
        super::contract_graph::Contract::DarkToshiDice => vec![
            (&include_bytes!("../../darktoshi_dice/proof/commit_bet_v1.zk.bin")[..], "CommitBet_V1"),
            (&include_bytes!("../../darktoshi_dice/proof/settle_bet_v1.zk.bin")[..], "SettleBet_V1"),
        ],
        super::contract_graph::Contract::Roulette => vec![
            (&include_bytes!("../../roulette/proof/place_bet_v1.zk.bin")[..], "PlaceBet_V1"),
            (&include_bytes!("../../roulette/proof/settle_bet_v1.zk.bin")[..], "SettleBet_V1"),
        ],
        super::contract_graph::Contract::Deployooor => vec![],
        // Newer contracts without circuit binaries yet
        super::contract_graph::Contract::AtomicSwap
        | super::contract_graph::Contract::Attestation
        | super::contract_graph::Contract::BettingStake
        | super::contract_graph::Contract::BlockHeightPrediction
        | super::contract_graph::Contract::Bridge
        | super::contract_graph::Contract::DarkbetExchange
        | super::contract_graph::Contract::DrainProtection
        | super::contract_graph::Contract::Escrow
        | super::contract_graph::Contract::GameRoom
        | super::contract_graph::Contract::InsuranceMarket
        | super::contract_graph::Contract::LaborMarket
        | super::contract_graph::Contract::Oracle
        | super::contract_graph::Contract::PoolStake
        | super::contract_graph::Contract::RelayerEndowment
        | super::contract_graph::Contract::SafeMath
        | super::contract_graph::Contract::Subscription
        | super::contract_graph::Contract::Tender => vec![],
    }
}

/// Get PKs and VKs for specific contracts (and their dependencies).
///
/// This function enables selective circuit loading instead of loading ALL circuits.
/// Only the specified contracts and their dependencies are loaded.
///
/// # Example
///
/// ```rust,ignore
/// use darkfi_contract_test_harness::{vks::get_vks_for, contract_graph::{Contract, resolve_dependencies}};
///
/// // Get only Money circuits (no dependencies)
/// let (pks, vks) = get_vks_for(&[Contract::Money])?;
///
/// // Get DAO circuits (includes Money as dependency)
/// let (pks, vks) = get_vks_for(&[Contract::Dao])?;
///
/// // Get Roulette only (isolated, no dependencies)
/// let (pks, vks) = get_vks_for(&[Contract::Roulette])?;
/// ```
pub fn get_vks_for(contracts: &[Contract]) -> Result<(Pks, Vks)> {
    // Resolve dependencies
    let resolved = resolve_dependencies(contracts);

    // Build PKs and VKs only for the resolved contracts
    let mut pks = vec![];
    let mut vks = vec![];

    for contract in resolved {
        for (bincode, namespace) in get_circuit_binaries(contract) {
            debug!("Building PK for {} ({})", namespace, contract.name());
            let zkbin = ZkBinary::decode(bincode, false)?;
            let witnesses = empty_witnesses(&zkbin)?;
            let circuit = ZkCircuit::new(witnesses, &zkbin);

            let pk = ProvingKey::build(zkbin.k, &circuit);
            let mut pk_buf = vec![];
            pk.write(&mut pk_buf)?;
            pks.push((bincode.to_vec(), zkbin.namespace.clone(), pk_buf));

            debug!("Building VK for {} ({})", namespace, contract.name());
            let vk = VerifyingKey::build(zkbin.k, &circuit);
            let mut vk_buf = vec![];
            vk.write(&mut vk_buf)?;
            vks.push((bincode.to_vec(), zkbin.namespace.clone(), vk_buf));
        }
    }

    Ok((pks, vks))
}