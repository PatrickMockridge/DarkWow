/// Build script: force cargo to recompile the test harness whenever
/// contract proof (.zk.bin) files or harness source files change.
/// Without this, `cargo test` can use a cached rlib with stale
/// `include_bytes!()` data, producing PK/VK mismatches.
fn main() {
    let contracts = [
        "attestation", "auction", "baccarat", "bearer_bond", "betting_stake",
        "box", "bridge", "dao_escrow", "darkbet_exchange", "darktoshi_dice",
        "deployooor", "dex", "drain_protection", "escrow", "game_room",
        "identity", "insurance_market", "labor_market", "lottery",
        "multisig", "native_token", "oracle", "otc_swap", "pool_stake",
        "promissory_note", "purse", "relayer_endowment", "roulette",
        "slot", "stablecoin", "subscription", "tender",
    ];
    for c in &contracts {
        println!("cargo:rerun-if-changed=../{}/proof/", c);
    }
    println!("cargo:rerun-if-changed=src/harness/");
}
