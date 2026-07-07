/// Configuration loading module — visible, testable, no derive magic.
///
/// Provides:
/// - WalletConfig: loaded configuration
/// - load_config(): reads TOML, merges CLI overrides, returns Result

use std::path::PathBuf;

use crate::wallet_error::{Error, Result};
use crate::wallet_util::expand_path;

use crate::args::WalletArgs;

/// Loaded wallet configuration.
#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub network: String,
    pub chain_path: String,
    pub cache_path: String,
    pub wallet_path: String,
    pub wallet_pass: String,
    /// Enable production security checks (password strength, encryption verification).
    /// Defaults to false — dev/dockernet mode with relaxed password policies.
    pub production_mode: bool,
    pub history_path: String,
    /// Path to the declared keys.toml (the wallet's identity declaration).
    /// From `--keys` CLI or the `KEYS_FILE` env var. Required for key-bearing
    /// commands — the wallet derives its identity on boot, it is never stored.
    pub keys_toml: Option<String>,
    /// keys.toml section selecting this wallet's identity (from `WALLET_NAME`
    /// env; defaults to `wallet-1`). Mirrors dwowd's `NODE_NAME`.
    pub section: String,
    /// P2P network settings parsed from `[net]` TOML section.
    /// None if the section is missing or parsing fails (P2P disabled).
    pub p2p_settings: Option<crate::p2p_wallet::P2pWalletConfig>,
}

/// Default config file contents — embedded at compile time.
const DEFAULT_CONFIG: &str = include_str!("../dww_config.toml");

/// Resolve config path from user-provided or default location.
fn resolve_config_path(config_arg: Option<&str>, fallback: &str) -> Result<PathBuf> {
    match config_arg {
        Some(path) => Ok(expand_path(path)?),
        None => {
            let mut pb = expand_path("~/.config/dwow")?;
            pb.push(fallback);
            Ok(pb)
        }
    }
}

/// Load wallet configuration from TOML file, merging CLI overrides.
///
/// This is a pure function — takes parsed args, returns config.
/// Uses std::fs (sync), not smol::fs. No derive magic.
pub fn load_config(args: &WalletArgs) -> Result<WalletConfig> {
    // Resolve config path
    let config_path = resolve_config_path(args.config.as_deref(), "dww_config.toml")?;

    // Read TOML file
    let contents = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Create default config
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, DEFAULT_CONFIG)?;
            eprintln!(
                "Config file created in {:?}. Please review it and try again.",
                config_path
            );
            return Err(Error::ConfigInvalid)
        }
        Err(e) => return Err(e.into()),
    };

    // Parse as generic TOML value
    let toml_value: toml::Value = toml::from_str(&contents).map_err(|e| {
        eprintln!("Failed parsing TOML config {:?}: {}", config_path, e);
        Error::ParseFailed("Failed parsing TOML config")
    })?;

    // Network: CLI -n wins. If not passed, use TOML's top-level network field.
    // This matches dwowd's behavior where from_args_with_toml merges TOML network.
    let network_name = if args.network_explicit {
        args.network.clone()
    } else {
        toml_value
            .get("network")
            .and_then(|v| v.as_str())
            .unwrap_or(&args.network)
            .to_string()
    };
    let network_config = toml_value
        .get("network_config")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get(&network_name))
        .and_then(|v| v.as_table())
        .ok_or_else(|| {
            let available: Vec<&str> = toml_value
                .get("network_config")
                .and_then(|v| v.as_table())
                .map(|t| t.keys().map(|k| k.as_str()).collect())
                .unwrap_or_default();
            eprintln!(
                "Network '{}' not found in config. Available: {:?}",
                network_name, available
            );
            Error::ParseFailed("Network configuration not found")
        })?;

    // Extract values with defaults
    let chain_path = network_config
        .get("chain_path")
        .and_then(|v| v.as_str())
        .unwrap_or("~/.local/share/dwow/dww/chain")
        .to_string();
    let cache_path = network_config
        .get("cache_path")
        .and_then(|v| v.as_str())
        .unwrap_or("~/.local/share/dwow/dww/cache")
        .to_string();
    let wallet_path = network_config
        .get("wallet_path")
        .and_then(|v| v.as_str())
        .unwrap_or("~/.local/share/dwow/dww/wallet.db")
        .to_string();
    let wallet_pass = network_config
        .get("wallet_pass")
        .and_then(|v| v.as_str())
        .unwrap_or("changeme")
        .to_string();

    // CLI --production overrides TOML value
    let mut production_mode = network_config
        .get("production")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if args.production {
        production_mode = true;
    }

    // Production-mode password validation
    if production_mode {
        if wallet_pass.is_empty() {
            return Err(Error::Custom(
                "wallet_pass must not be empty in production mode".into()
            ));
        }
        if wallet_pass.len() < 8 {
            return Err(Error::Custom(
                "wallet_pass must be at least 8 characters in production mode".into()
            ));
        }
        if wallet_pass == "testpassword123" || wallet_pass == "changeme" {
            return Err(Error::Custom(
                "wallet_pass is a default/weak password. Set a strong password in production mode.".into()
            ));
        }
    }

    let history_path = network_config
        .get("history_path")
        .and_then(|v| v.as_str())
        .unwrap_or("~/.local/share/dwow/dww/history.txt")
        .to_string();

    // Expand path segments
    let chain_path = expand_path(&chain_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(chain_path);
    let cache_path = expand_path(&cache_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(cache_path);
    let wallet_path = expand_path(&wallet_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(wallet_path);
    let history_path = expand_path(&history_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(history_path);

    // Parse P2P settings from `[net]` subsection (optional).
    let p2p_settings = network_config
        .get("net")
        .and_then(|net_table| {
            let net_toml = toml::to_string(net_table).ok()?;
            match toml::from_str::<crate::p2p_wallet::P2pWalletConfig>(&net_toml) {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    eprintln!(
                        "[dww] [net] P2P config parse FAILED: {e}. \
                         Raw net section: {net_toml}"
                    );
                    None
                }
            }
        });
    if network_config.get("net").is_some() && p2p_settings.is_none() {
        eprintln!(
            "[dww] [net] P2P config section is present but failed to parse. P2P networking DISABLED."
        );
    }
    if let Some(ref cfg) = p2p_settings {
        eprintln!(
            "[dww] P2P config parsed: seeds={:?} magic_bytes={:?} localnet={} app_name={:?}",
            cfg.seeds.iter().map(|s| &s.url).collect::<Vec<_>>(),
            cfg.magic_bytes, cfg.localnet, cfg.app_name,
        );
    }

    // Wallet identity declaration: keys.toml path (--keys or KEYS_FILE env) and
    // section (WALLET_NAME env, default wallet-1). The wallet derives its identity
    // from these on boot; nothing is persisted. Mirrors dwowd's --keys + NODE_NAME.
    let keys_toml = args.keys.clone().or_else(|| std::env::var("KEYS_FILE").ok());
    let section = std::env::var("WALLET_NAME").unwrap_or_else(|_| "wallet-1".to_string());

    Ok(WalletConfig {
        network: network_name.clone(),
        chain_path,
        cache_path,
        wallet_path,
        wallet_pass,
        production_mode,
        history_path,
        keys_toml,
        section,
        p2p_settings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_config(name: &str, content: &str) -> String {
        let dir = std::env::temp_dir().join("dwow_test_config");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn test_load_config_basic() {
        let toml = r#"
            network = "darkwow-testnet"
            [network_config."darkwow-testnet"]
            cache_path = "/tmp/cache"
            wallet_path = "/tmp/wallet.db"
            wallet_pass = "testpass"
            history_path = "/tmp/history.txt"
        "#;
        let path = write_temp_config("basic.toml", toml);
        let args = WalletArgs {
            config: Some(path.clone()),
            network: "darkwow-testnet".into(),
            network_explicit: true,
            command: crate::args::WalletCommand::Wallet {
                command: crate::args::WalletSubcmd::Keygen,
            },
            log: None,
            verbose: 0,
            production: false,
        };
        let config = load_config(&args).unwrap();
        assert_eq!(config.cache_path, "/tmp/cache");
        assert_eq!(config.wallet_path, "/tmp/wallet.db");
        assert_eq!(config.wallet_pass, "testpass");
        assert_eq!(config.history_path, "/tmp/history.txt");
        // Clean up
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_load_config_missing_network() {
        let toml = r#"
            network = "darkwow-testnet"
            [network_config."othernet"]
            cache_path = "/tmp/cache"
        "#;
        let path = write_temp_config("missing_net.toml", toml);
        let args = WalletArgs {
            config: Some(path.clone()),
            network: "darkwow-testnet".into(),
            network_explicit: true,
            command: crate::args::WalletCommand::Wallet {
                command: crate::args::WalletSubcmd::Keygen,
            },
            log: None,
            verbose: 0,
            production: false,
        };
        let result = load_config(&args);
        assert!(result.is_err());
        std::fs::remove_file(&path).ok();
    }
}

/// Convert wallet TOML P2pWalletConfig → dwow_core::net::Settings.
/// Seeds as [{url = "tcp+tls://..."}] become peers as ["tcp+tls://..."].
/// Wallet is client-only: empty inbound_addrs, outbound_connections=1.
pub fn build_p2p_settings(
    config: &crate::p2p_wallet::P2pWalletConfig,
) -> crate::wallet_error::Result<dwow_core::net::Settings> {
    use dwow_core::net::settings::{MagicBytes, Settings};
    use url::Url;
    let seeds: Vec<Url> = config.seeds.iter()
        .filter_map(|s| Url::parse(&s.url).ok())
        .collect();
    let peers: Vec<Url> = config.peers.iter()
        .filter_map(|s| Url::parse(&s.url).ok())
        .collect();
    let inbound_addrs: Vec<Url> = config.inbound.iter()
        .filter_map(|s| Url::parse(s).ok())
        .collect();
    let mut profiles = std::collections::HashMap::new();
    profiles.insert("tcp+tls".into(), dwow_core::net::settings::NetworkProfile {
        outbound_connect_timeout: 15,
        channel_handshake_timeout: 10,
        channel_heartbeat_interval: 30,
    });
    Ok(Settings {
        app_name: config.app_name.clone().unwrap_or_else(|| "dwow-wallet".into()),
        app_version: semver::Version::new(0, 5, 0),
        inbound_addrs,
        external_addrs: vec![],
        outbound_connections: 8,
        inbound_connections: 4,
        localnet: config.localnet,
        magic_bytes: MagicBytes(config.magic_bytes),
        peers,
        seeds,
        active_profiles: vec!["tcp+tls".into()],
        profiles,
        ..Default::default()
    })
}
