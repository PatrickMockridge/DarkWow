/// Configuration loading module — visible, testable, no derive magic.
///
/// Provides:
/// - WalletConfig: loaded configuration
/// - load_config(): reads TOML, merges CLI overrides, returns Result

use std::path::PathBuf;

use dwow_core::{util::path::expand_path, Error, Result};

use crate::args::WalletArgs;

/// Loaded wallet configuration.
#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub network: String,
    pub database: String,
    pub cache_path: String,
    pub wallet_path: String,
    pub wallet_pass: String,
    pub endpoint: String,
    pub history_path: String,
}

/// Default config file contents — embedded at compile time.
const DEFAULT_CONFIG: &str = include_str!("../dww_config.toml");

/// Resolve config path from user-provided or default location.
fn resolve_config_path(config_arg: Option<&str>, fallback: &str) -> Result<PathBuf> {
    match config_arg {
        Some(path) => expand_path(path),
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
    let database = network_config
        .get("database")
        .and_then(|v| v.as_str())
        .unwrap_or("~/.local/share/dwow/dww/database")
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
    let endpoint = network_config
        .get("endpoint")
        .and_then(|v| v.as_str())
        .unwrap_or("tcp://127.0.0.1:31345")
        .to_string();
    let history_path = network_config
        .get("history_path")
        .and_then(|v| v.as_str())
        .unwrap_or("~/.local/share/dwow/dww/history.txt")
        .to_string();

    // Expand path segments
    let database = expand_path(&database)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(database);
    let cache_path = expand_path(&cache_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(cache_path);
    let wallet_path = expand_path(&wallet_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(wallet_path);
    let history_path = expand_path(&history_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(history_path);

    Ok(WalletConfig {
        network: network_name.clone(),
        database,
        cache_path,
        wallet_path,
        wallet_pass,
        endpoint,
        history_path,
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
            endpoint = "tcp://node0:31345"
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
        };
        let config = load_config(&args).unwrap();
        assert_eq!(config.cache_path, "/tmp/cache");
        assert_eq!(config.wallet_path, "/tmp/wallet.db");
        assert_eq!(config.wallet_pass, "testpass");
        assert_eq!(config.endpoint, "tcp://node0:31345");
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
        };
        let result = load_config(&args);
        assert!(result.is_err());
        std::fs::remove_file(&path).ok();
    }
}
