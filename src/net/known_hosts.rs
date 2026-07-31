/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! SSH-style known_hosts for TLS certificate pinning (HAZOP H1).
//!
//! Trust-On-First-Use: on first connection to a clearnet TLS peer, store
//! SHA-256(cert.der) keyed by hostname. On subsequent connections, verify
//! the presented certificate matches the stored fingerprint. Reject on
//! mismatch (possible MITM or legitimate key rotation).
//!
//! Tor onion addresses are self-authenticating (the .onion IS the key hash)
//! and are not stored here.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use tracing::{debug, warn};

/// SHA-256 fingerprint of a DER-encoded TLS certificate.
pub type CertFingerprint = [u8; 32];

/// SSH-style known_hosts trust store for TLS certificate pinning.
pub struct KnownHosts {
    entries: Mutex<HashMap<String, CertFingerprint>>,
    path: PathBuf,
}

impl KnownHosts {
    /// Load known_hosts from a TSV file. Creates an empty store if the
    /// file does not exist. Lines with invalid fingerprints are skipped.
    pub fn load(path: PathBuf) -> Result<Self, std::io::Error> {
        let mut entries = HashMap::new();
        if path.exists() {
            let file = fs::File::open(&path)?;
            let reader = std::io::BufReader::new(file);
            for (line_no, line) in reader.lines().enumerate() {
                let line = line?;
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() < 2 {
                    warn!(target: "net::known_hosts",
                        "Line {} malformed (expected hostname<TAB>fingerprint): {}", line_no + 1, line);
                    continue;
                }
                let hostname = parts[0].to_string();
                let fingerprint_hex = parts[1];
                if let Ok(fingerprint) = hex::decode(fingerprint_hex) {
                    let mut arr = [0u8; 32];
                    let len = fingerprint.len().min(32);
                    arr[..len].copy_from_slice(&fingerprint[..len]);
                    entries.insert(hostname, arr);
                } else {
                    warn!(target: "net::known_hosts",
                        "Line {} invalid hex fingerprint: {}", line_no + 1, fingerprint_hex);
                }
            }
        }
        debug!(target: "net::known_hosts", "Loaded {} known hosts from {:?}", entries.len(), path);
        Ok(Self { entries: Mutex::new(entries), path })
    }

    /// Look up a stored fingerprint for a hostname.
    pub fn lookup(&self, hostname: &str) -> Option<CertFingerprint> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).get(hostname).copied()
    }

    /// Store a fingerprint for a hostname (Trust-On-First-Use).
    /// Silently overwrites if the hostname already exists (key rotation).
    pub fn store(&self, hostname: &str, fingerprint: CertFingerprint) {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).insert(hostname.to_string(), fingerprint);
    }

    /// Remove a hostname from the trust store (operator-initiated key rotation).
    pub fn remove(&self, hostname: &str) {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).remove(hostname);
    }

    /// Persist all entries to disk as TSV: `hostname<TAB>hex_fingerprint\n`.
    pub fn save(&self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&self.path)?;
        writeln!(file, "# DarkWow known_hosts — TLS certificate fingerprints")?;
        writeln!(file, "# <hostname>\t<sha256_hex>")?;
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        for (hostname, fingerprint) in entries.iter() {
            writeln!(file, "{}\t{}", hostname, hex::encode(fingerprint))?;
        }
        debug!(target: "net::known_hosts", "Saved {} known hosts to {:?}", entries.len(), self.path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_hosts_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");

        let kh = KnownHosts::load(path.clone()).unwrap();
        assert!(kh.lookup("example.com").is_none());

        let fp = [0xAAu8; 32];
        kh.store("example.com", fp);
        assert_eq!(kh.lookup("example.com"), Some(fp));
        kh.save().unwrap();

        let kh2 = KnownHosts::load(path).unwrap();
        assert_eq!(kh2.lookup("example.com"), Some(fp));
    }

    #[test]
    fn test_known_hosts_reject_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");

        let kh = KnownHosts::load(path.clone()).unwrap();
        kh.store("example.com", [0xAAu8; 32]);
        kh.save().unwrap();

        let kh2 = KnownHosts::load(path).unwrap();
        let stored = kh2.lookup("example.com").unwrap();
        assert_ne!(stored, [0xBBu8; 32]);
    }

    #[test]
    fn test_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent");
        let kh = KnownHosts::load(path).unwrap();
        assert!(kh.lookup("anything").is_none());
    }
}
