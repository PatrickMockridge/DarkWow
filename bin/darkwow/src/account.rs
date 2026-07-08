// darkwow account — key/account lifecycle operations.
//
// This is the thin argv→method adapter over the universal service provider,
// `dwow_accounts::AccountManager`. No key logic lives here; every operation is
// a call into AccountManager. Ported from the former `dwow_keygen` binary and
// extended with the declared-identity export absorbed from `dwowd`.
//
// Subcommands:
//   generate                        Generate a fresh random key (output=hex)
//   import-hex <hex>                Import a key from 64-char hex
//   import-base58 <b58>             Import a key from base58
//   from-seed <phrase>              Derive HD key from BIP39 mnemonic
//   export <index>                  Export account by index from the vault (base58)
//   export --keys <f> --section <s> Export the declared keys.toml identity (base58)
//   list                            List all accounts in the vault
//
// Persistence (vault): after any mutation, the encrypted JSON blob is written
// to `~/.dwow/lifecycle.json` (overridable with --output).

use std::path::PathBuf;

use dwow_sdk::crypto::keypair::Network;

/// Entry point. `args` is dwow_keygen-style: args[0] = program label,
/// args[1] = subcommand, args[2..] = subcommand arguments.
pub fn run(args: &[String]) {
    if args.len() < 2 {
        usage();
        return;
    }

    let subcmd = &args[1];
    match subcmd.as_str() {
        "generate" => cmd_generate(),
        "import-hex" => cmd_import_hex(args),
        "import-base58" => cmd_import_base58(args),
        "from-seed" => cmd_from_seed(args),
        "export" => cmd_export(args),
        "list" => cmd_list(),
        _ => usage(),
    }
}

fn usage() {
    eprintln!("darkwow account — AccountManager key lifecycle CLI");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  generate                          Generate a fresh random key (output=hex)");
    eprintln!("  import-hex <hex>                  Import a key from 64-char hex");
    eprintln!("  import-base58 <b58>               Import a key from base58");
    eprintln!("  from-seed <phrase>                Derive HD key from BIP39 mnemonic");
    eprintln!("  export <index>                    Export vault account by index (base58)");
    eprintln!("  export --keys <f> --section <s>   Export the declared keys.toml identity (base58)");
    eprintln!("  list                              List all vault accounts");
    eprintln!();
    eprintln!("After a vault mutation, the encrypted lifecycle blob is written to");
    eprintln!("~/.dwow/lifecycle.json (overridable with --output <path>).");
}

// ── helpers ───────────────────────────────────────────────────────────────

fn lifecycle_path(args: &[String]) -> PathBuf {
    for i in 0..args.len() {
        if args[i] == "--output" && i + 1 < args.len() {
            return PathBuf::from(&args[i + 1]);
        }
    }
    let mut p = dirs_next().unwrap_or_else(|| PathBuf::from("."));
    p.push(".dwow");
    p.push("lifecycle.json");
    p
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Look up `--flag <value>` in args; return the value if present.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

fn open_manager() -> dwow_accounts::AccountManager {
    // The vault manager reads an existing lifecycle blob (if present) so we
    // don't clobber previously-imported keys. It does NOT read keys.toml —
    // this is a lifecycle-only manager (no declared identity). `empty()` is the
    // constructor for this purpose (no declared root).
    let mut mgr = dwow_accounts::AccountManager::empty(Network::Testnet);
    let path = lifecycle_path(&[]);
    if path.exists() {
        if let Ok(blob) = std::fs::read_to_string(&path) {
            let _ = mgr.load_lifecycle(blob.as_bytes());
        }
    }
    mgr
}

fn save_manager(mgr: &dwow_accounts::AccountManager, args: &[String]) {
    let path = lifecycle_path(args);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match mgr.to_json_string() {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, &json) {
                eprintln!("ERROR writing lifecycle blob to {}: {}", path.display(), e);
            } else {
                eprintln!("Lifecycle blob saved to {}", path.display());
            }
        }
        Err(e) => eprintln!("ERROR serializing: {}", e),
    }
}

// ── commands ──────────────────────────────────────────────────────────────

fn cmd_generate() {
    let mut mgr = open_manager();
    let idx = mgr.generate();
    let secret_hex = mgr.export_hex(idx).unwrap_or_else(|e| format!("ERROR: {}", e));
    println!("{}", secret_hex);
    eprintln!("Generated key at index {} (secret hex printed to stdout)", idx);
    save_manager(&mgr, &[]);
}

fn cmd_import_hex(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: darkwow account import-hex <64-char-hex>");
        return;
    }
    let mut mgr = open_manager();
    match mgr.import_hex(&args[2]) {
        Ok(idx) => {
            eprintln!("Imported key at index {}", idx);
            save_manager(&mgr, args);
        }
        Err(e) => eprintln!("{}", e),
    }
}

fn cmd_import_base58(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: darkwow account import-base58 <base58-string>");
        return;
    }
    let mut mgr = open_manager();
    match mgr.import_base58(&args[2]) {
        Ok(idx) => {
            eprintln!("Imported key at index {}", idx);
            save_manager(&mgr, args);
        }
        Err(e) => eprintln!("{}", e),
    }
}

fn cmd_from_seed(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: darkwow account from-seed <BIP39-mnemonic-phrase>");
        return;
    }
    let phrase = &args[2];
    let mut mgr = open_manager();
    match dwow_accounts::AccountManager::from_seed_phrase(phrase, "", Network::Testnet) {
        Ok(new_mgr) => {
            for account in new_mgr.accounts().iter().skip(0) {
                if let Err(e) = mgr.import_hex(&account.secret_hex()) {
                    eprintln!("Skip duplicate: {}", e);
                }
            }
            if let Some(seed) = new_mgr.encrypted_seed.as_ref() {
                mgr.encrypted_seed = Some(seed.clone());
                mgr.seed_is_mnemonic = new_mgr.seed_is_mnemonic;
            }
            eprintln!("HD accounts imported from seed phrase");
            save_manager(&mgr, args);
        }
        Err(e) => eprintln!("{}", e),
    }
}

fn cmd_export(args: &[String]) {
    // keys.toml form — export the declared identity (absorbs `dwowd --export-secret`).
    // `--keys <file>` with `--section <name>` (or the NODE_NAME env var) resolves
    // the declared identity via AccountManager::open and prints its base58 secret.
    if let Some(keys_file) = flag_value(args, "--keys") {
        let section = flag_value(args, "--section")
            .or_else(|| std::env::var("NODE_NAME").ok());
        let section = match section {
            Some(s) => s,
            None => {
                eprintln!("export: --section <name> (or NODE_NAME env) required with --keys");
                return;
            }
        };
        let path = PathBuf::from(&keys_file);
        let mgr = match dwow_accounts::AccountManager::open(&path, Network::Testnet, &section) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("export: AccountManager::open failed: {e}");
                return;
            }
        };
        let idx = mgr.default_index();
        let b58 = match mgr.export_base58(idx) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("export: {e}");
                return;
            }
        };
        // Loud diagnostic on stderr — key identity for verification.
        // The base58 secret key on stdout is the pipe-able output.
        let pk_hex = match mgr.default_public_key() {
            Ok(pk) => hex::encode(pk.to_bytes()),
            Err(_) => "unknown".to_string(),
        };
        eprintln!("export: section={} account[{}] secrets={} public={}",
            section, idx, mgr.secrets().len(), pk_hex);
        println!("{b58}");
        return;
    }

    // Vault form — export account by index from the lifecycle blob.
    let idx_arg = flag_value(args, "--index").or_else(|| args.get(2).cloned());
    let idx: usize = match idx_arg.as_deref().map(|s| s.parse()) {
        Some(Ok(i)) => i,
        _ => {
            eprintln!("Usage: darkwow account export <index>");
            eprintln!("   or: darkwow account export --keys <file> --section <name>");
            return;
        }
    };
    let mgr = open_manager();
    match mgr.export_base58(idx) {
        Ok(b58) => println!("{}", b58),
        Err(e) => eprintln!("{}", e),
    }
}

fn cmd_list() {
    let mgr = open_manager();
    for (i, a) in mgr.accounts().iter().enumerate() {
        let pk_hex = hex::encode(a.keypair.public.to_bytes());
        println!("[{}] public={}", i, pk_hex);
    }
}
