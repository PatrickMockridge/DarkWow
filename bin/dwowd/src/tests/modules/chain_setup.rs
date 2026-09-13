//! Chain setup — create and initialize a HeavyweightPipeline.
//!
//! Used by: All 4 test categories (43 tests).
//! Spec: heavyweight-spec.md §9 (Per-Contract Test Template, Step 1).

use dwow_core::Result;
use crate::tests::blockchain::HeavyweightPipeline;

/// The canonical genesis identity: the `[node0]` secret from
/// `contrib/docker/darkwow-testnet/keys.toml`.
///
/// Genesis creation is deterministic, but its hash depends on TWO identity inputs —
/// the genesis miner's key (it drives the coinbase commitment and nullifier) and the
/// network magic bytes embedded in `anchor_tx_id[0..4]`. The compile-time pin
/// `bin/dwowd/genesis_hash.txt` is a single value, so there can be only ONE genesis
/// identity for the whole repository.
///
/// Every test that calls `init_genesis` MUST use [`GENESIS_KEYS_TOML`] and
/// [`DRKW_MAGIC`]. This is not cosmetic: on a populated (non-placeholder) pin, a
/// genesis built from any other key or magic does not match the pin and
/// `init_genesis` hard-errors. Adopting the devnet identity also means the tests
/// build the genesis the network actually runs, rather than a synthetic one.
pub const GENESIS_KEYS_TOML: &str = "[node0]\nwallet_secret = \
    \"755c6e8a21b3e15f146ba636a146c228b5f91202fc7e0bb0065efdd9fd685405\"\n";

/// Network magic embedded in the genesis `anchor_tx_id[0..4]` — the bytes "DRKW".
///
/// Must equal `magic_bytes` in the `[network_config."darkwow-testnet".net]` section of
/// `bin/dwowd/dwowd_config.toml` and `contrib/docker/darkwow-testnet/lib/config.sh`,
/// since the devnet's node0 derives its genesis from the config value.
pub const DRKW_MAGIC: [u8; 4] = [68, 82, 75, 87];

/// Create and initialize a fresh HeavyweightPipeline for testing.
/// Returns chain ready for deploy/block operations at height 1.
pub async fn init_test_chain() -> Result<HeavyweightPipeline> {
    let chain = HeavyweightPipeline::new().await?;
    chain.init_genesis().await?;
    Ok(chain)
}
