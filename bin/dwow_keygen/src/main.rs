// dwow_keygen — thin CLI for AccountManager key lifecycle operations.
//
// Key generation, import, export, and seed-phrase import are AccountManager
// lifecycle operations. This binary is the owner-facing entry point. The
// declared identity lives in keys.toml [section]; lifecycle keys are additive
// (index >= 1) and never displace the declaration.
//
// Persistence: after any mutation, the encrypted JSON blob is written to
// `~/.dwow/lifecycle.json` (overridable with --output). Wallet and miner
// hydrates this blob on boot via AccountManager::load_lifecycle().

use std::env;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
        return;
    }

    let subcmd = &args[1];
    match subcmd.as_str() {
        "generate" => cmd_generate(),
        "import-hex" => cmd_import_hex(&args),
        "import-base58" => cmd_import_base58(&args),
        "from-seed" => cmd_from_seed(&args),
        "export" => cmd_export(&args),
        "list" => cmd_list(),
        _ => usage(),
    }
}

fn usage() {
    eprintln!("dwow_keygen — AccountManager key lifecycle CLI");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  generate                        Generate a fresh random key (output=hex)");
    eprintln!("  import-hex <hex>                Import a key from 64-char hex");
    eprintln!("  import-base58 <b58>             Import a key from base58");
    eprintln!("  from-seed <phrase>              Derive HD key from BIP39 mnemonic");
    eprintln!("  export <index>                  Export account by index (base58)");
    eprintln!("  list                            List all accounts");
    eprintln!();
    eprintln!("After mutation, the encrypted lifecycle blob is written to");
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

fn open_manager() -> dwow_accounts::AccountManager {
    // keygen reads an existing lifecycle blob (if present) so we don't
    // clobber previously-imported keys. It does NOT read keys.toml — this
    // is a lifecycle-only manager (no declared identity). `empty()` is the
    // constructor for this purpose (no declared root).
    let mut mgr = dwow_accounts::AccountManager::empty(
        dwow_sdk::crypto::keypair::Network::Testnet,
    );
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
        eprintln!("Usage: dwow_keygen import-hex <64-char-hex>");
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
        eprintln!("Usage: dwow_keygen import-base58 <base58-string>");
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
        eprintln!("Usage: dwow_keygen from-seed <BIP39-mnemonic-phrase>");
        return;
    }
    let phrase = &args[2];
    let mut mgr = open_manager();
    match dwow_accounts::AccountManager::from_seed_phrase(phrase, "") {
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
    if args.len() < 3 {
        eprintln!("Usage: dwow_keygen export <index>");
        return;
    }
    let idx: usize = match args[2].parse() {
        Ok(i) => i,
        Err(_) => { eprintln!("Invalid index: {}", args[2]); return; }
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
