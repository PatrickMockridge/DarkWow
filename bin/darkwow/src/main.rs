// darkwow — the top-level DarkWow CLI.
//
// One coherent entry point from which the owner can spin up a mining node, an
// observer node, run a wallet, or perform key/account operations. This is a
// thin launcher: `node` and `wallet` hand off (exec) to the untouched `dwowd`
// and `dwow_wallet` binaries; `account` runs in-process via the universal
// service provider, `dwow_accounts::AccountManager`.
//
//   darkwow node [--observer] [dwowd args...]   Full node (mining), or observer
//   darkwow wallet <subcommand...>              Wallet operations
//   darkwow account <subcommand...>             Key/account lifecycle
//
// `dwowd`/`dwow_wallet` behavior is unchanged; only their invocation path moves
// behind `darkwow`.

use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

mod account;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "node" => run_node(&args),
        "wallet" => run_wallet(&args),
        "account" => {
            // Re-shape argv so the account module sees dwow_keygen-style args:
            // [program-label, subcmd, ...rest].
            let mut acct_args = vec!["darkwow account".to_string()];
            acct_args.extend_from_slice(&args[2..]);
            account::run(&acct_args);
        }
        "-h" | "--help" | "help" => usage(),
        "-V" | "--version" | "version" => {
            println!("darkwow {}", env!("CARGO_PKG_VERSION"));
        }
        other => {
            eprintln!("darkwow: unknown command `{other}`\n");
            usage();
            std::process::exit(1);
        }
    }
}

fn usage() {
    eprintln!("darkwow — DarkWow top-level CLI");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  darkwow node --role <genesis|miner|observer> --identity <section> [args...]");
    eprintln!("  darkwow wallet <subcommand...>        Wallet operations");
    eprintln!("  darkwow account <subcommand...>       Key/account lifecycle (generate, import, export, ...)");
    eprintln!("  darkwow help                          Print this help");
    eprintln!("  darkwow version                       Print version");
}

/// Resolve a sibling binary: prefer the launcher's own directory (this is
/// `/app` inside the Docker images), fall back to bare name on `PATH`.
fn sibling_binary(name: &str) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(name);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(name)
}

/// `darkwow node [--role R] [--identity S] [args...]` → exec `dwowd`.
///
/// Role is the single composition surface, replacing the scattered
/// MINING_ENABLED / IS_SEED / CREATE_GENESIS env trio:
///   genesis  → mining on,  seed off, create-genesis on
///   miner    → mining on,  seed off, create-genesis off
///   observer → mining off, seed on,  create-genesis off  (also `--observer`)
/// `--identity <section>` sets NODE_NAME (the keys.toml section). The launcher
/// derives the child env in ONE place; dwowd reads MINING_ENABLED and (new)
/// CREATE_GENESIS from that env. observer≠genesis is structurally impossible
/// (one role value) and re-asserted by dwowd's own panic as defense-in-depth.
/// The seed-vs-peer P2P topology is a config concern owned by the entrypoint.
fn run_node(args: &[String]) {
    let mut forward: Vec<String> = Vec::new();
    let mut role: Option<String> = None;
    let mut identity: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--observer" => role = Some("observer".to_string()),
            "--role" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("darkwow node: --role requires a value (genesis|miner|observer)");
                    std::process::exit(2);
                }
                role = Some(args[i].clone());
            }
            "--identity" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("darkwow node: --identity requires a keys.toml section name");
                    std::process::exit(2);
                }
                identity = Some(args[i].clone());
            }
            other => forward.push(other.to_string()),
        }
        i += 1;
    }

    let bin = sibling_binary("dwowd");
    let mut cmd = Command::new(&bin);
    cmd.args(&forward);

    // Derive the mining/genesis env from role (single derivation point). An
    // explicitly pre-set env var wins — the escape hatch for special topologies
    // (e.g. join-merge, which mines externally via p2pool and keeps the internal
    // miner off with MINING_ENABLED=false).
    if let Some(r) = role.as_deref() {
        // MINING_ENABLED starts the miner task, which itself gates on
        // sync_state == CaughtUp (lib.rs:1264). So `miner` means "may mine AFTER
        // sync", not "mine now": it starts in observer mode and becomes a miner
        // on CaughtUp. CREATE_GENESIS is a SEPARATE explicit flag — only
        // `genesis` sets it; the genesis ceremony is decoupled from mining.
        let (mining, create_genesis) = match r {
            "genesis" => ("true", "true"),     // explicit genesis ceremony + mines
            "miner" => ("true", "false"),      // observer until CaughtUp, then miner
            "observer" => ("false", "false"),  // sync-only, never mines
            other => {
                eprintln!("darkwow node: unknown --role `{other}` (expected genesis|miner|observer)");
                std::process::exit(2);
            }
        };
        if std::env::var_os("MINING_ENABLED").is_none() {
            cmd.env("MINING_ENABLED", mining);
        }
        if std::env::var_os("CREATE_GENESIS").is_none() {
            cmd.env("CREATE_GENESIS", create_genesis);
        }
    }
    if let Some(id) = identity {
        cmd.env("NODE_NAME", id);
    }

    // exec replaces this process — signals and exit code pass through dwowd.
    let err = cmd.exec();
    eprintln!("darkwow: failed to exec {}: {err}", bin.display());
    std::process::exit(127);
}

/// `darkwow wallet <...>` → exec `dwow_wallet` with the args forwarded verbatim.
fn run_wallet(args: &[String]) {
    let bin = sibling_binary("dwow_wallet");
    let mut cmd = Command::new(&bin);
    cmd.args(&args[2..]);
    let err = cmd.exec();
    eprintln!("darkwow: failed to exec {}: {err}", bin.display());
    std::process::exit(127);
}
