//! ContractTestSpec definitions — one file per contract.
//! Each file exports a single function returning ContractTestSpec<'static>.

pub mod helpers;

pub mod fee_integration_spec;

pub mod attestation_spec;
pub mod auction_spec;
pub mod baccarat_spec;
pub mod bearer_bond_spec;
pub mod bridge_spec;
pub mod lottery_spec;
pub mod tender_spec;
pub mod betting_stake_spec;
pub mod box_spec;
pub mod dao_escrow_spec;
pub mod darkbet_exchange_spec;
pub mod darktoshi_dice_spec;
pub mod deployooor_spec;
pub mod dex_spec;
pub mod drain_protection_spec;
pub mod escrow_spec;
pub mod game_room_spec;
pub mod pool_stake_spec;
pub mod stablecoin_spec;
pub mod identity_spec;
pub mod insurance_market_spec;
pub mod labor_market_spec;
pub mod native_token_spec;
pub mod multisig_spec;
pub mod oracle_spec;
pub mod otc_swap_spec;
pub mod promissory_note_spec;
pub mod purse_spec;
pub mod relayer_endowment_spec;
pub mod roulette_spec;
pub mod slot_spec;
pub mod subscription_spec;
