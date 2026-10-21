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
    /// env). REQUIRED at key-resolution time — no default, mirroring dwowd's
    /// `NODE_NAME`. `None` if unset; the resolve sites hard-fail rather than
    /// silently pick a wrong identity.
    pub section: Option<String>,
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
    // HAZOP RC-F fix: no default password — must be configured or use env var.
    // Previously fell back to "changeme" which is trivially decryptable.
    let wallet_pass = network_config
        .get("wallet_pass")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| std::env::var("DWOW_WALLET_PASS").ok());

    // CLI --production overrides TOML value
    let mut production_mode = network_config
        .get("production")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if args.production {
        production_mode = true;
    }

    // HAZOP RC-F fix: validate password strength. Production requires
    // explicit non-empty, non-weak password (no default fallback).
    if production_mode {
        let pass = wallet_pass.as_deref().unwrap_or("");
        if pass.is_empty() {
            return Err(Error::Custom(
                "wallet_pass must not be empty in production mode. Set it in config or via DWOW_WALLET_PASS env var.".into()
            ));
        }
        if pass.len() < 8 {
            return Err(Error::Custom(
                "wallet_pass must be at least 8 characters in production mode.".into()
            ));
        }
        if pass == "testpassword123" || pass == "changeme" {
            return Err(Error::Custom(
                "wallet_pass is a default/weak password. Set a strong password in production mode.".into()
            ));
        }
    }

    // HAZOP RC-F fix: resolve wallet_pass to a String for downstream use.
    // In non-production mode with no password configured, use an empty
    // string — the wallet will still encrypt data (empty password means
    // SQLCipher with zero-length key, which is better than a known default).
    let wallet_pass = wallet_pass.unwrap_or_default();

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
            "[dww] P2P config parsed: peers={:?} magic_bytes={:?} localnet={} app_name={:?}",
            cfg.peers.iter().map(|s| &s.url).collect::<Vec<_>>(),
            cfg.magic_bytes, cfg.localnet, cfg.app_name,
        );
    }

    // Wallet identity declaration: keys.toml path (--keys or KEYS_FILE env) and
    // section (WALLET_NAME env, REQUIRED — no default). The wallet derives its
    // identity from these on boot; nothing is persisted. Mirrors dwowd's
    // --keys + NODE_NAME (both required at resolution time).
    let keys_toml = args.keys.clone().or_else(|| std::env::var("KEYS_FILE").ok());
    let section = std::env::var("WALLET_NAME").ok();

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
            keys: None,
            network: "darkwow-testnet".into(),
            network_explicit: true,
            command: crate::args::WalletCommand::Wallet {
                command: crate::args::WalletSubcmd::Balance { porcelain: false },
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

    /// `OBL-C247` — the embedded public-testnet profile is a public profile.
    ///
    /// It used to be `localnet = true` with a loopback peer, and the guide told a native wallet to
    /// use it: TLS SAN and TOFU pinning were skipped and the only sync target was the machine the
    /// wallet ran on. This test reads the shipped TOML, not a copy, and fails on those values.
    #[test]
    fn embedded_profiles_do_not_put_a_public_network_in_localnet() {
        let doc: toml::Value = toml::from_str(DEFAULT_CONFIG).expect("embedded config parses");
        let networks = doc.get("network_config").and_then(|v| v.as_table())
            .expect("embedded config has network_config");

        // The upstream `testnet` profile (different magic, different seeds) is not a DarkWow
        // network and must not ship as an option.
        assert!(
            !networks.contains_key("testnet"),
            "the embedded config still carries the upstream `testnet` profile"
        );

        let net = networks.get("darkwow-testnet").and_then(|v| v.get("net"))
            .expect("darkwow-testnet has a [net] section");
        assert_eq!(
            net.get("localnet").and_then(|v| v.as_bool()),
            Some(false),
            "a public-testnet profile must not run in localnet mode (TLS SAN and pinning are skipped)"
        );
        let magic: Vec<i64> = net.get("magic_bytes")
            .and_then(|v| v.as_array())
            .expect("magic_bytes is an array")
            .iter()
            .map(|v| v.as_integer().expect("magic byte is an integer"))
            .collect();
        assert_eq!(magic, vec![68, 82, 75, 87], "public testnet magic must be DRKW");

        // A peer is a full node named by its P2P base port. No loopback/private peer may stand in
        // for one on a public profile: that is how the local override leaked into the public file.
        let peers = net.get("peers").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        for peer in peers {
            let url = peer.get("url").and_then(|v| v.as_str()).expect("peer has a url");
            let parsed = url::Url::parse(url).expect("peer url parses");
            let host = parsed.host_str().expect("peer url has a host");
            assert!(
                !is_loopback_or_private(host),
                "public-testnet peer {url} is loopback/private; name a public full node"
            );
        }

        // The devnet profile is the local overlay and is expected to stay local.
        let devnet = networks.get("darkwow-devnet").and_then(|v| v.get("net"))
            .expect("darkwow-devnet has a [net] section");
        assert_eq!(
            devnet.get("localnet").and_then(|v| v.as_bool()),
            Some(true),
            "the devnet overlay is the one profile that may run localnet"
        );
    }

    fn is_loopback_or_private(host: &str) -> bool {
        let host = host.trim_matches(['[', ']']);
        if host.eq_ignore_ascii_case("localhost") {
            return true;
        }
        match host.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
            Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local(),
            Err(_) => false,
        }
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
            keys: None,
            network: "darkwow-testnet".into(),
            network_explicit: true,
            command: crate::args::WalletCommand::Wallet {
                command: crate::args::WalletSubcmd::Balance { porcelain: false },
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
/// The wallet connects DIRECTLY to `peers` (ManualSession) — no seeds/hostlist.
/// Wallet is client-only: empty inbound_addrs.
pub fn build_p2p_settings(
    config: &crate::p2p_wallet::P2pWalletConfig,
) -> crate::wallet_error::Result<dwow_core::net::Settings> {
    use dwow_core::net::settings::{BanPolicy, MagicBytes, Settings};
    use url::Url;
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
    // Harmonised with the mining node: derive app_name/app_version from Cargo
    // (env!("CARGO_PKG_NAME")/env!("CARGO_PKG_VERSION")), exactly as dwowd's
    // main.rs does — NOT hardcoded. A hardcoded version silently drifts from
    // the node and makes every connection fail the major.minor handshake.
    #[expect(clippy::expect_used, reason = "CARGO_PKG_VERSION is a valid semver (compile-time)")]
    let app_version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("CARGO_PKG_VERSION must be valid semver");
    Ok(Settings {
        app_name: config.app_name.clone().unwrap_or_else(|| env!("CARGO_PKG_NAME").to_string()),
        app_version,
        inbound_addrs,
        external_addrs: vec![],
        // HAZOP H1: the datastore is where `known_hosts` (the TOFU certificate pins) lives.
        // Without it the TLS verifier validates the shared self-signed SAN and pins nothing.
        p2p_datastore: config.datastore.clone(),
        outbound_connections: 8,
        inbound_connections: 4,
        localnet: config.localnet,
        magic_bytes: MagicBytes(config.magic_bytes),
        peers,
        active_profiles: vec!["tcp+tls".into()],
        profiles,
        // DarkWow terms: the wallet is a fixed-peer pull client — it must never
        // ban the peers the user explicitly configured. Relaxed disables the
        // Strict-gated bans (MissingDispatcher/MessageInvalid/MeteringLimitExceeded).
        ban_policy: BanPolicy::Relaxed,
        ..Default::default()
    })
}
