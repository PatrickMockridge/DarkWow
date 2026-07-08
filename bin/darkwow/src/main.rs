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
    eprintln!("  darkwow node [--observer] [args...]   Run a full node (mining) or observer");
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

/// `darkwow node [--observer] [args...]` → exec `dwowd` with the remaining args.
/// `--observer` is a launcher-level flag: it sets MINING_ENABLED=false in the
/// child env (unless already set) and is stripped before forwarding.
fn run_node(args: &[String]) {
    let mut forward: Vec<String> = Vec::new();
    let mut observer = false;
    for a in &args[2..] {
        if a == "--observer" {
            observer = true;
        } else {
            forward.push(a.clone());
        }
    }

    let bin = sibling_binary("dwowd");
    let mut cmd = Command::new(&bin);
    cmd.args(&forward);
    if observer && std::env::var_os("MINING_ENABLED").is_none() {
        cmd.env("MINING_ENABLED", "false");
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
