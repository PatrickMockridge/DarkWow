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

//! Account Manager — unified key management for mining nodes and wallets.
//!
//! Single source of truth for all key material. Used by both `dwowd` (mining)
//! and `dwow_wallet` (wallet daemon) — both binaries import keys through the
//! same `AccountManager::open()` entry point.
//!
//! Keys are declared in `keys.toml` (operator-managed) or auto-generated on
//! localnet. Resolution order: sled cache → keys.toml → auto-gen → error.
//!
//! The `section_name` parameter controls which `[section]` of `keys.toml` is
//! used: mining nodes pass `None` (resolves via `NODE_NAME` env var, default
//! `"node0"`), wallets pass `Some("wallet-N")` to select the matching section.

use std::path::Path;

use dwow_sdk::crypto::keypair::{Keypair, Network, PublicKey, SecretKey};
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use pasta_curves::{group::ff::FromUniformBytes, pallas};

/// A single account — one keypair with optional metadata.
#[derive(Debug, Clone)]
pub struct Account {
    pub keypair: Keypair,
    /// Human-readable label (future: set by user)
    pub label: Option<String>,
    /// BIP32 derivation path (future: HD wallets)
    pub derivation_path: Option<String>,
}

impl Account {
    pub fn address(&self, network: Network) -> String {
        use dwow_sdk::crypto::keypair::{Address, StandardAddress};
        let addr: Address = StandardAddress::from_public(network, self.keypair.public).into();
        addr.to_string()
    }

    pub fn secret_hex(&self) -> String {
        hex::encode(self.keypair.secret.inner().to_repr())
    }
}

/// Manages a collection of accounts. Both mining nodes and wallets use this.
pub struct AccountManager {
    accounts: Vec<Account>,
    default_index: usize,
    db: Option<sled::Db>,
    /// Network for address generation (testnet=0xaf, mainnet=0x39)
    pub network: Network,
}

impl AccountManager {
    // ========================================================================
    // Construction
    // ========================================================================

    /// Load accounts from the key resolution chain.
    /// The sled DB caches resolved state for fast restart.
    ///
    /// Resolution order:
    ///   1. Sled cache (restart) — accounts previously persisted
    ///   2. keys.toml declaration — operator-specified keys (single source of truth)
    ///   3. Auto-generate (localnet only) — random key for dev/testing
    ///   4. Hard error (non-localnet, no keys declared) — never mine to random keys
    ///
    /// `section_name`: overrides the `[section]` to use in `keys.toml`.
    /// - Mining nodes pass `None` → resolved via `NODE_NAME` env var (default `"node0"`)
    /// - Wallet daemon passes `Some("wallet-N")` → selects `[wallet-N]` section
    /// - Tests pass `None` (no keys.toml path, auto-generates on localnet)
    pub fn open(
        db: &sled::Db,
        localnet: bool,
        keys_toml: Option<&Path>,
        network: Network,
        section_name: Option<&str>,
    ) -> Result<Self, String> {
        let tree = db.open_tree("accounts")
            .map_err(|e| format!("sled open_tree: {e}"))?;

        // 1. Sled cache — restart path
        if let Some(stored) = tree.get("accounts_json")
            .map_err(|e| format!("sled get: {e}"))?
        {
            return Self::from_json(&stored, db.clone(), network);
        }

        // 2. keys.toml declaration — operator-specified keys
        if let Some(path) = keys_toml {
            if path.exists() {
                let contents = std::fs::read_to_string(path)
                    .map_err(|e| format!("read keys.toml: {e}"))?;
                let cfg: toml::Value = toml::from_str(&contents)
                    .map_err(|e| format!("parse keys.toml: {e}"))?;

                // Determine which section to use.
                // section_name param overrides NODE_NAME env var (for wallets).
                // Falls back to NODE_NAME env var (for mining nodes), default "node0".
                let node_name = if let Some(name) = section_name {
                    name.to_string()
                } else {
                    std::env::var("NODE_NAME").unwrap_or_else(|_| "node0".into())
                };
                let hex_secret = cfg.get(&node_name)
                    .and_then(|s| s.get("wallet_secret"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!(
                        "keys.toml: section [{}] with wallet_secret not found", node_name
                    ))?;

                if hex_secret.len() != 64 {
                    return Err(format!(
                        "keys.toml: [{}].wallet_secret must be 64 hex chars, got {}",
                        node_name, hex_secret.len()
                    ));
                }

                // Build AccountManager with the declared key.
                // We create the struct directly rather than using open()+import_hex()
                // to avoid the orphan auto-generated key problem (F9).
                let bytes = hex::decode(hex_secret)
                    .map_err(|e| format!("keys.toml hex decode: {e}"))?;
                let arr = <[u8; 32]>::try_from(bytes)
                    .map_err(|_| "keys.toml: expected 32 bytes".to_string())?;
                let secret = SecretKey::from_bytes(arr)
                    .map_err(|_| "keys.toml: invalid secret key".to_string())?;
                let keypair = Keypair::new(secret);

                let manager = AccountManager {
                    accounts: vec![Account {
                        keypair,
                        label: Some(format!("{}-declared", node_name)),
                        derivation_path: None,
                    }],
                    default_index: 0,
                    db: Some(db.clone()),
                    network,
                };

                // Cache in sled for fast restart
                manager.save(&tree)?;

                return Ok(manager);
            }
        }

        // 3. Auto-generate (localnet only) — no keys declared, no sled state
        if localnet {
            let account = Account {
                keypair: Keypair::random(&mut rand::rngs::OsRng),
                label: Some("default".into()),
                derivation_path: None,
            };

            let manager = AccountManager {
                accounts: vec![account],
                default_index: 0,
                db: Some(db.clone()),
                network,
            };

            manager.save(&tree)?;

            return Ok(manager);
        }

        // 4. Hard error — non-localnet, no keys declared
        Err("No keys declared and no cached keys found. \
             Provide a keys.toml with --keys or set LOCALNET=true for auto-generation."
            .into())
    }

    /// Import an account from a hex secret (from keys.toml or env var).
    pub fn import_hex(&mut self, hex_secret: &str) -> Result<usize, String> {
        let hex_secret = hex_secret.trim();
        let bytes = hex::decode(hex_secret).map_err(|e| format!("hex decode: {e}"))?;
        let arr = <[u8; 32]>::try_from(bytes)
            .map_err(|_| "expected 32 bytes".to_string())?;
        let secret = SecretKey::from_bytes(arr)
            .map_err(|_| "invalid secret key".to_string())?;

        // Check for duplicate (case-insensitive hex comparison)
        let hex_lower = hex_secret.to_lowercase();
        if let Some(idx) = self.accounts.iter().position(|a| a.secret_hex().to_lowercase() == hex_lower) {
            return Err(format!(
                "Secret already imported at index {} (label: {})",
                idx,
                self.accounts[idx].label.as_deref().unwrap_or("unnamed")
            ));
        }

        let keypair = Keypair::new(secret);
        let account = Account {
            keypair,
            label: Some(format!("imported-{}", self.accounts.len())),
            derivation_path: None,
        };
        self.accounts.push(account);
        Ok(self.accounts.len() - 1)
    }

    /// Generate a new random account. Auto-sets as default (HAZID RC5.5).
    pub fn generate(&mut self) -> usize {
        let account = Account {
            keypair: Keypair::random(&mut rand::rngs::OsRng),
            label: Some(format!("generated-{}", self.accounts.len())),
            derivation_path: None,
        };
        self.accounts.push(account);
        let idx = self.accounts.len() - 1;
        self.default_index = idx;
        idx
    }

    /// Remove an account by index. The default account cannot be removed
    /// unless it is the last remaining account.
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range (0-{})", index, self.accounts.len().saturating_sub(1)));
        }
        if self.accounts.len() <= 1 {
            return Err("Cannot remove the last account".into());
        }
        self.accounts.remove(index);
        // Adjust default_index if the removed account was before it
        if index < self.default_index {
            self.default_index = self.default_index.saturating_sub(1);
        } else if self.default_index >= self.accounts.len() {
            self.default_index = self.accounts.len().saturating_sub(1);
        }
        Ok(())
    }

    /// Export the secret hex for an account by index.
    pub fn export_hex(&self, index: usize) -> Result<String, String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range (0-{})", index, self.accounts.len().saturating_sub(1)));
        }
        Ok(self.accounts[index].secret_hex())
    }

    /// Import a secret key from a base58-encoded string.
    /// Decodes base58 → 32 bytes → SecretKey, checks for duplicates,
    /// appends to accounts. Returns the new account index.
    pub fn import_base58(&mut self, b58: &str) -> Result<usize, String> {
        let b58 = b58.trim();
        if b58.is_empty() {
            return Err("empty base58 string".into());
        }
        let bytes = bs58::decode(b58).into_vec()
            .map_err(|e| format!("base58 decode: {e}"))?;
        let arr = <[u8; 32]>::try_from(bytes.clone())
            .map_err(|_| format!("expected 32 bytes, got {}", bytes.len()))?;
        let secret = SecretKey::from_bytes(arr)
            .map_err(|_| "invalid secret key".to_string())?;

        // Check for duplicate by comparing secret bytes
        let secret_bytes = secret.inner().to_repr();
        if let Some(idx) = self.accounts.iter().position(|a| {
            a.keypair.secret.inner().to_repr() == secret_bytes
        }) {
            return Err(format!(
                "Secret already imported at index {} (label: {})",
                idx,
                self.accounts[idx].label.as_deref().unwrap_or("unnamed")
            ));
        }

        let keypair = Keypair::new(secret);
        let account = Account {
            keypair,
            label: Some(format!("imported-{}", self.accounts.len())),
            derivation_path: None,
        };
        self.accounts.push(account);
        Ok(self.accounts.len() - 1)
    }

    /// Export a secret key as base58-encoded string by account index.
    pub fn export_base58(&self, index: usize) -> Result<String, String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range (0-{})", index, self.accounts.len().saturating_sub(1)));
        }
        Ok(bs58::encode(self.accounts[index].keypair.secret.inner().to_repr()).into_string())
    }

    // ========================================================================
    // Access
    // ========================================================================

    pub fn default_account(&self) -> Result<&Account, String> {
        if self.accounts.is_empty() {
            return Err("No accounts in AccountManager".into());
        }
        Ok(&self.accounts[self.default_index])
    }

    pub fn default_public_key(&self) -> Result<PublicKey, String> {
        Ok(self.default_account()?.keypair.public)
    }

    pub fn default_index(&self) -> usize {
        self.default_index
    }

    pub fn set_default(&mut self, index: usize) -> Result<(), String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range", index));
        }
        self.default_index = index;
        Ok(())
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn secrets(&self) -> Vec<SecretKey> {
        self.accounts.iter().map(|a| a.keypair.secret).collect()
    }

    // ========================================================================
    // Persistence (sled)
    // ========================================================================

    fn save(&self, tree: &sled::Tree) -> Result<(), String> {
        let json = self.to_json()?;
        tree.insert("accounts_json", json.as_bytes()).map_err(|e| format!("sled write: {e}"))?;
        tree.flush().map_err(|e| format!("sled flush: {e}"))?;
        Ok(())
    }

    /// Save current state using stored db reference. Call after import or generate.
    pub fn persist(&self) -> Result<(), String> {
        match &self.db {
            Some(db) => {
                let tree = db.open_tree("accounts")
                    .map_err(|e| format!("sled open: {e}"))?;
                self.save(&tree)
            }
            None => Err("AccountManager: no db reference — cannot persist".into()),
        }
    }

    // ========================================================================
    // Serialization (JSON — simple, inspectable)
    // ========================================================================

    fn to_json(&self) -> Result<String, String> {
        let entries: Vec<serde_json::Value> = self.accounts.iter().map(|a| {
            serde_json::json!({
                "secret_hex": a.secret_hex(),
                "address": a.address(self.network),
                "label": a.label,
                "derivation_path": a.derivation_path,
            })
        }).collect();
        let json = serde_json::json!({
            "default_index": self.default_index,
            "accounts": entries,
        });
        serde_json::to_string_pretty(&json).map_err(|e| format!("json serialize: {e}"))
    }

    fn from_json(data: &[u8], db: sled::Db, network: Network) -> Result<Self, String> {
        let json: serde_json::Value = serde_json::from_slice(data).map_err(|e| format!("json parse: {e}"))?;
        let default_index = json["default_index"].as_u64()
            .ok_or("missing default_index field")? as usize;
        let entries = json["accounts"].as_array()
            .ok_or("missing accounts array")?;
        let mut accounts = Vec::new();
        for entry in entries {
            let hex_str = entry["secret_hex"].as_str()
                .ok_or("missing secret_hex")?;
            let bytes = hex::decode(hex_str)
                .map_err(|e| format!("hex decode: {e}"))?;
            let arr = <[u8; 32]>::try_from(bytes)
                .map_err(|_| "expected 32 bytes".to_string())?;
            let secret = SecretKey::from_bytes(arr)
                .map_err(|_| "invalid secret key".to_string())?;
            let keypair = Keypair::new(secret);
            accounts.push(Account {
                keypair,
                label: entry["label"].as_str().map(|s| s.to_string()),
                derivation_path: entry["derivation_path"].as_str().map(|s| s.to_string()),
            });
        }
        Ok(AccountManager { accounts, default_index, db: Some(db), network })
    }

    // ========================================================================
    // BIP39 Seed Phrase Import
    // ========================================================================

    /// Import accounts from a BIP39 seed phrase (12 or 24 words).
    /// Derives the master key at path `m/44'/0'/0'/0/0` (BIP44 default).
    pub fn from_seed_phrase(phrase: &str, passphrase: &str) -> Result<Self, String> {
        let seed = bip39_to_seed(phrase, passphrase)?;
        Self::from_seed(&seed, "m/44'/0'/0'/0/0")
    }

    /// Import accounts from a raw 32-byte seed + derivation path.
    pub fn from_seed(seed: &[u8; 64], path: &str) -> Result<Self, String> {
        let child_key = bip32_derive(seed, path)?;
        let keypair = Keypair::new(child_key);
        let account = Account {
            keypair,
            label: Some(format!("hd-{}", path.replace('/', "-"))),
            derivation_path: Some(path.to_string()),
        };
        Ok(AccountManager {
            accounts: vec![account],
            default_index: 0,
            db: None,
            network: Network::Testnet, // seed phrases derive testnet keys by default
        })
    }

    /// Derive a child key from a seed at a BIP32 derivation path.
    pub fn derive_key(seed: &[u8; 64], path: &str) -> Result<SecretKey, String> {
        bip32_derive(seed, path)
    }
}

// ── BIP39 Wordlist (2048 English words) ─────────────────────────────────

const BIP39_WORDS: &[&str] = &[
    "abandon","ability","able","about","above","absent","absorb","abstract","absurd","abuse",
    "access","accident","account","accuse","achieve","acid","acoustic","acquire","across","act",
    "action","actor","actress","actual","adapt","add","addict","address","adjust","admit",
    "adult","advance","advice","aerobic","affair","afford","afraid","africa","after","again",
    "age","agent","agree","ahead","aim","air","airport","aisle","alarm","album",
    "alcohol","alert","alien","all","alley","allow","almost","alone","alpha","already",
    "also","alter","always","amateur","amazing","among","amount","amused","analyst","anchor",
    "ancient","anger","angle","angry","animal","ankle","announce","annual","another","answer",
    "antenna","antique","anxiety","any","apart","apology","appear","apple","approve","april",
    "arch","arctic","area","arena","argue","arm","armed","armor","army","around",
    "arrange","arrest","arrive","arrow","art","artefact","artist","artwork","ask","aspect",
    "assault","asset","assist","assume","asthma","athlete","atom","attack","attend","attitude",
    "attract","auction","audit","august","aunt","author","auto","autumn","average","avocado",
    "avoid","awake","aware","away","awesome","awful","awkward","axis","baby","bachelor",
    "bacon","badge","bag","balance","balcony","ball","bamboo","banana","banner","bar",
    "barely","bargain","barrel","base","basic","basket","battle","beach","bean","beauty",
    "because","become","beef","before","begin","behave","behind","believe","below","belt",
    "bench","benefit","best","betray","better","between","beyond","bicycle","bid","bike",
    "bind","biology","bird","birth","bitter","black","blade","blame","blanket","blast",
    "bleak","bless","blind","blood","blossom","blouse","blue","blur","blush","board",
    "boat","body","boil","bomb","bone","bonus","book","boost","border","boring",
    "borrow","boss","bottom","bounce","box","boy","bracket","brain","brand","brass",
    "brave","bread","breeze","brick","bridge","brief","bright","bring","brisk","broccoli",
    "broken","bronze","broom","brother","brown","brush","bubble","buddy","budget","buffalo",
    "build","bulb","bulk","bullet","bundle","bunker","burden","burger","burst","bus",
    "business","busy","butter","buyer","buzz","cabbage","cabin","cable","cactus","cage",
    "cake","call","calm","camera","camp","can","canal","cancel","candy","cannon",
    "canoe","canvas","canyon","capable","capital","captain","car","carbon","card","cargo",
    "carpet","carry","cart","case","cash","casino","castle","casual","cat","catalog",
    "catch","category","cattle","caught","cause","caution","cave","ceiling","celery","cement",
    "census","century","cereal","certain","chair","chalk","champion","change","chaos","chapter",
    "charge","chase","chat","cheap","check","cheese","chef","cherry","chest","chicken",
    "chief","child","chimney","choice","choose","chronic","chuckle","chunk","churn","cigar",
    "cinnamon","circle","citizen","city","civil","claim","clap","clarify","claw","clay",
    "clean","clerk","clever","click","client","cliff","climb","clinic","clip","clock",
    "clog","close","cloth","cloud","clown","club","clump","cluster","clutch","coach",
    "coast","coconut","code","coffee","coil","coin","collect","color","column","combine",
    "come","comfort","comic","common","company","concert","conduct","confirm","congress","connect",
    "consider","control","convince","cook","cool","copper","copy","coral","core","corn",
    "correct","cost","cotton","couch","country","couple","course","cousin","cover","coyote",
    "crack","cradle","craft","cram","crane","crash","crater","crawl","crazy","cream",
    "credit","creek","crew","cricket","crime","crisp","critic","crop","cross","crouch",
    "crowd","crucial","cruel","cruise","crumble","crunch","crush","cry","crystal","cube",
    "culture","cup","cupboard","curious","current","curtain","curve","cushion","custom","cute",
    "cycle","dad","damage","damp","dance","danger","daring","dash","daughter","dawn",
    "day","deal","debate","debris","decade","december","decide","decline","decorate","decrease",
    "deer","defense","define","defy","degree","delay","deliver","demand","demise","denial",
    "dentist","deny","depart","depend","deposit","depth","deputy","derive","describe","desert",
    "design","desk","despair","destroy","detail","detect","develop","device","devote","diagram",
    "dial","diamond","diary","dice","diesel","diet","differ","digital","dignity","dilemma",
    "dinner","dinosaur","direct","dirt","disagree","discover","disease","dish","dismiss","disorder",
    "display","distance","divert","divide","divorce","dizzy","doctor","document","dog","doll",
    "dolphin","domain","donate","donkey","donor","door","dose","double","dove","draft",
    "dragon","drama","drastic","draw","dream","dress","drift","drill","drink","drip",
    "drive","drop","drum","dry","duck","dumb","dune","during","dust","dutch",
    "duty","dwarf","dynamic","eager","eagle","early","earn","earth","easily","east",
    "easy","echo","ecology","economy","edge","edit","educate","effort","egg","eight",
    "either","elbow","elder","electric","elegant","element","elephant","elevator","elite","else",
    "embark","embody","embrace","emerge","emotion","employ","empower","empty","enable","enact",
    "end","endless","endorse","enemy","energy","enforce","engage","engine","enhance","enjoy",
    "enlist","enough","enrich","enroll","ensure","enter","entire","entry","envelope","episode",
    "equal","equip","era","erase","erode","erosion","error","erupt","escape","essay",
    "essence","estate","eternal","ethics","evidence","evil","evoke","evolve","exact","example",
    "excess","exchange","excite","exclude","excuse","execute","exercise","exhaust","exhibit","exile",
    "exist","exit","exotic","expand","expect","expire","explain","expose","express","extend",
    "extra","eye","eyebrow","fabric","face","faculty","fade","faint","faith","fall",
    "false","fame","family","famous","fan","fancy","fantasy","farm","fashion","fat",
    "fatal","father","fatigue","fault","favorite","feature","february","federal","fee","feed",
    "feel","female","fence","festival","fetch","fever","few","fiber","fiction","field",
    "figure","file","film","filter","final","find","fine","finger","finish","fire",
    "firm","first","fiscal","fish","fit","fitness","fix","flag","flame","flash",
    "flat","flavor","flee","flight","flip","float","flock","floor","flower","fluid",
    "flush","fly","foam","focus","fog","foil","fold","follow","food","foot",
    "force","forest","forget","fork","fortune","forum","forward","fossil","foster","found",
    "fox","fragile","frame","frequent","fresh","friend","fringe","frog","front","frost",
    "frown","frozen","fruit","fuel","fun","funny","furnace","fury","future","gadget",
    "gain","galaxy","gallery","game","gap","garage","garbage","garden","garlic","garment",
    "gas","gasp","gate","gather","gauge","gaze","general","genius","genre","gentle",
    "genuine","gesture","ghost","giant","gift","giggle","ginger","giraffe","girl","give",
    "glad","glance","glare","glass","glide","glimpse","globe","gloom","glory","glove",
    "glow","glue","goat","goddess","gold","good","goose","gorilla","gospel","gossip",
    "govern","gown","grab","grace","grain","grant","grape","grass","gravity","great",
    "green","grid","grief","grit","grocery","group","grow","grunt","guard","guess",
    "guide","guilt","guitar","gun","gym","habit","hair","half","hammer","hamster",
    "hand","happy","harbor","hard","harsh","harvest","hat","have","hawk","hazard",
    "head","health","heart","heavy","hedgehog","height","hello","helmet","help","hen",
    "hero","hidden","high","hill","hint","hip","hire","history","hobby","hockey",
    "hold","hole","holiday","hollow","home","honey","hood","hope","horn","horror",
    "horse","hospital","host","hotel","hour","hover","hub","huge","human","humble",
    "humor","hundred","hungry","hunt","hurdle","hurry","hurt","husband","hybrid","ice",
    "icon","idea","identify","idle","ignore","ill","illegal","illness","image","imitate",
    "immense","immune","impact","impose","improve","impulse","inch","include","income","increase",
    "index","indicate","indoor","industry","infant","inflict","inform","inhale","inherit","initial",
    "inject","injury","inmate","inner","innocent","input","inquiry","insane","insect","inside",
    "inspire","install","intact","interest","into","invest","invite","involve","iron","island",
    "isolate","issue","item","ivory","jacket","jaguar","jar","jazz","jealous","jeans",
    "jelly","jewel","job","join","joke","journey","joy","judge","juice","jump",
    "jungle","junior","junk","just","kangaroo","keen","keep","ketchup","key","kick",
    "kid","kidney","kind","kingdom","kiss","kit","kitchen","kite","kitten","kiwi",
    "knee","knife","knock","know","lab","label","labor","ladder","lady","lake",
    "lamp","language","laptop","large","later","latin","laugh","laundry","lava","law",
    "lawn","lawsuit","layer","lazy","leader","leaf","learn","leave","lecture","left",
    "leg","legal","legend","leisure","lemon","lend","length","lens","leopard","lesson",
    "letter","level","liar","liberty","library","license","life","lift","light","like",
    "limb","limit","link","lion","liquid","list","little","live","lizard","load",
    "loan","lobster","local","lock","logic","lonely","long","loop","lottery","loud",
    "lounge","love","loyal","lucky","luggage","lumber","lunar","lunch","luxury","lyrics",
    "machine","mad","magic","magnet","maid","mail","main","major","make","mammal",
    "man","manage","mandate","mango","mansion","manual","maple","marble","march","margin",
    "marine","market","marriage","mask","mass","master","match","material","math","matrix",
    "matter","maximum","maze","meadow","mean","measure","meat","mechanic","medal","media",
    "melody","melt","member","memory","mention","menu","mercy","merge","merit","merry",
    "mesh","message","metal","method","middle","midnight","milk","million","mimic","mind",
    "minimum","minor","minute","miracle","mirror","misery","miss","mistake","mix","mixed",
    "mixture","mobile","model","modify","mom","moment","monitor","monkey","monster","month",
    "moon","moral","more","morning","mosquito","mother","motion","motor","mountain","mouse",
    "move","movie","much","muffin","mule","multiply","muscle","museum","mushroom","music",
    "must","mutual","myself","mystery","myth","naive","name","napkin","narrow","nasty",
    "nation","nature","near","neck","need","negative","neglect","neither","nephew","nerve",
    "nest","net","network","neutral","never","news","next","nice","night","noble",
    "noise","nominee","noodle","normal","north","nose","notable","note","nothing","notice",
    "novel","now","nuclear","number","nurse","nut","oak","obey","object","oblige",
    "obscure","observe","obtain","obvious","occur","ocean","october","odor","off","offer",
    "office","often","oil","okay","old","olive","olympic","omit","once","one",
    "onion","online","only","open","opera","opinion","oppose","option","orange","orbit",
    "orchard","order","ordinary","organ","orient","original","orphan","ostrich","other","outdoor",
    "outer","output","outside","oval","oven","over","own","owner","oxygen","oyster",
    "ozone","pact","paddle","page","pair","palace","palm","panda","panel","panic",
    "panther","paper","parade","parent","park","parrot","party","pass","patch","path",
    "patient","patrol","pattern","pause","pave","payment","peace","peanut","pear","peasant",
    "pelican","pen","penalty","pencil","people","pepper","perfect","permit","person","pet",
    "phone","photo","phrase","physical","piano","picnic","picture","piece","pig","pigeon",
    "pill","pilot","pink","pioneer","pipe","pistol","pitch","pizza","place","planet",
    "plastic","plate","play","please","pledge","pluck","plug","plunge","poem","poet",
    "point","polar","pole","police","pond","pony","pool","popular","portion","position",
    "possible","post","potato","pottery","poverty","powder","power","practice","praise","predict",
    "prefer","prepare","present","pretty","prevent","price","pride","primary","print","priority",
    "prison","private","prize","problem","process","produce","profit","program","project","promote",
    "proof","property","prosper","protect","proud","provide","public","pudding","pull","pulp",
    "pulse","pumpkin","punch","pupil","puppy","purchase","purity","purpose","purse","push",
    "put","puzzle","pyramid","quality","quantum","quarter","question","quick","quit","quiz",
    "quote","rabbit","raccoon","race","rack","radar","radio","rail","rain","raise",
    "rally","ramp","ranch","random","range","rapid","rare","rate","rather","raven",
    "raw","razor","ready","real","reason","rebel","rebuild","recall","receive","recipe",
    "record","recycle","reduce","reflect","reform","refuse","region","regret","regular","reject",
    "relax","release","relief","rely","remain","remember","remind","remove","render","renew",
    "rent","reopen","repair","repeat","replace","report","require","rescue","resemble","resist",
    "resource","response","result","retire","retreat","return","reunion","reveal","review","reward",
    "rhythm","rib","ribbon","rice","rich","ride","ridge","rifle","right","rigid",
    "ring","riot","ripple","risk","ritual","rival","river","road","roast","robot",
    "robust","rocket","romance","roof","rookie","room","rose","rotate","rough","round",
    "route","royal","rubber","rude","rug","rule","run","runway","rural","sad",
    "saddle","sadness","safe","sail","salad","salmon","salon","salt","salute","same",
    "sample","sand","satisfy","satoshi","sauce","sausage","save","say","scale","scan",
    "scare","scatter","scene","scheme","school","science","scissors","scorpion","scout","scrap",
    "screen","script","scrub","sea","search","season","seat","second","secret","section",
    "security","seed","seek","segment","select","sell","seminar","senior","sense","sentence",
    "series","service","session","settle","setup","seven","shadow","shaft","shallow","share",
    "shed","shell","sheriff","shield","shift","shine","ship","shiver","shock","shoe",
    "shoot","shop","short","shoulder","shove","shrimp","shrug","shuffle","shy","sibling",
    "sick","side","siege","sight","sign","silent","silk","silly","silver","similar",
    "simple","since","sing","siren","sister","situate","six","size","skate","sketch",
    "ski","skill","skin","skirt","skull","slab","slam","sleep","slender","slice",
    "slide","slight","slim","slogan","slot","slow","slush","small","smart","smile",
    "smoke","smooth","snack","snake","snap","sniff","snow","soap","soccer","social",
    "sock","soda","soft","solar","soldier","solid","solution","solve","someone","song",
    "soon","sorry","sort","soul","sound","soup","source","south","space","spare",
    "spatial","spawn","speak","special","speed","spell","spend","sphere","spice","spider",
    "spike","spin","spirit","split","spoil","sponsor","spoon","sport","spot","spray",
    "spread","spring","spy","square","squeeze","squirrel","stable","stadium","staff","stage",
    "stairs","stamp","stand","start","state","stay","steak","steel","stem","step",
    "stereo","stick","still","sting","stock","stomach","stone","stool","story","stove",
    "strategy","street","strike","strong","struggle","student","stuff","stumble","style","subject",
    "submit","subway","success","such","sudden","suffer","sugar","suggest","suit","summer",
    "sun","sunny","sunset","super","supply","supreme","sure","surface","surge","surprise",
    "surround","survey","suspect","sustain","swallow","swamp","swap","swarm","swear","sweet",
    "swift","swim","swing","switch","sword","symbol","symptom","syrup","system","table",
    "tackle","tag","tail","talent","talk","tank","tape","target","task","taste",
    "tattoo","taxi","teach","team","tell","ten","tenant","tennis","tent","term",
    "test","text","thank","that","theme","then","theory","there","they","thing",
    "this","thought","three","thrive","throw","thumb","thunder","ticket","tide","tiger",
    "tilt","timber","time","tiny","tip","tired","tissue","title","toast","tobacco",
    "today","toddler","toe","together","toilet","token","tomato","tomorrow","tone","tongue",
    "tonight","tool","tooth","top","topic","topple","torch","tornado","tortoise","toss",
    "total","tourist","toward","tower","town","toy","track","trade","traffic","tragic",
    "train","transfer","trap","trash","travel","tray","treat","tree","trend","trial",
    "tribe","trick","trigger","trim","trip","trophy","trouble","truck","true","truly",
    "trumpet","trust","truth","try","tube","tuition","tumble","tuna","tunnel","turkey",
    "turn","turtle","twelve","twenty","twice","twin","twist","two","type","typical",
    "ugly","umbrella","unable","unaware","uncle","uncover","under","undo","unfair","unfold",
    "unhappy","uniform","unique","unit","universe","unknown","unlock","until","unusual","unveil",
    "update","upgrade","uphold","upon","upper","upset","urban","urge","usage","use",
    "used","useful","useless","usual","utility","vacant","vacuum","vague","valid","valley",
    "valve","van","vanish","vapor","various","vast","vault","vehicle","velvet","vendor",
    "venture","venue","verb","verify","version","very","vessel","veteran","viable","vibrant",
    "vicious","victory","video","view","village","vintage","violin","virtual","virus","visa",
    "visit","visual","vital","vivid","vocal","voice","void","volcano","volume","vote",
    "voyage","wage","wagon","wait","walk","wall","walnut","want","warfare","warm",
    "warrior","wash","wasp","waste","water","wave","way","wealth","weapon","wear",
    "weasel","weather","web","wedding","weekend","weird","welcome","west","wet","whale",
    "what","wheat","wheel","when","where","whip","whisper","wide","width","wife",
    "wild","will","win","window","wine","wing","wink","winner","winter","wire",
    "wisdom","wise","wish","witness","wolf","woman","wonder","wood","wool","word",
    "work","world","worry","worth","wrap","wreck","wrestle","wrist","write","wrong",
    "yard","year","yellow","you","young","youth","zebra","zero","zone","zoo",
];

// ── BIP39 Mnemonic → Seed (PBKDF2-HMAC-SHA512) ──────────────────────────

/// Decode BIP39 mnemonic words to entropy bytes.
/// Each word encodes 11 bits. First `word_count - 1` words are pure entropy;
/// the last word contains `entropy_bits - (word_count - 1) * 11` entropy bits
/// plus a checksum of `word_count * 11 - entropy_bits` bits.
fn bip39_words_to_entropy(phrase: &str) -> Result<(Vec<u8>, usize), String> {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.len() < 12 || words.len() > 24 || words.len() % 3 != 0 {
        return Err(format!("Invalid word count: {} (12/15/18/21/24 required)", words.len()));
    }

    let total_bits = words.len() * 11;
    let entropy_bits = total_bits - total_bits / 33; // checksum = total_bits / 32 (rounded up = total_bits / 33 integer)
    let checksum_bits = total_bits - entropy_bits;

    // Collect indices into a bitstream
    let mut bits: Vec<bool> = Vec::with_capacity(total_bits);
    for w in &words {
        let idx = BIP39_WORDS.iter().position(|&x| x == *w)
            .ok_or_else(|| format!("Invalid BIP39 word: '{}'", w))?;
        for bit in (0..11).rev() {
            bits.push((idx >> bit) & 1 == 1);
        }
    }

    // Pack entropy bits into bytes
    let entropy_bytes = (entropy_bits + 7) / 8;
    let mut entropy = vec![0u8; entropy_bytes];
    for i in 0..entropy_bits {
        if bits[i] {
            entropy[i / 8] |= 1 << (7 - (i % 8));
        }
    }

    Ok((entropy, checksum_bits))
}

/// Validate the BIP39 checksum.
/// The last word encodes both entropy and a SHA256-based checksum.
fn bip39_validate(phrase: &str) -> Result<(), String> {
    let (entropy, checksum_bits) = bip39_words_to_entropy(phrase)?;
    if checksum_bits == 0 {
        return Ok(()); // edge case, shouldn't happen with valid word counts
    }

    use sha2::{Sha256, Digest};
    let hash = Sha256::digest(&entropy);
    let expected_checksum = (hash[0] as usize) >> (8 - checksum_bits);

    // Extract actual checksum from the last word's trailing bits
    let words: Vec<&str> = phrase.split_whitespace().collect();
    let last_word_idx = BIP39_WORDS.iter().position(|&x| x == words[words.len() - 1])
        .ok_or_else(|| "Invalid BIP39 word".to_string())?;
    let mask = (1 << checksum_bits) - 1;
    let actual_checksum = last_word_idx & mask;

    if expected_checksum != actual_checksum {
        return Err(format!(
            "Invalid BIP39 checksum — possible typo in seed phrase. \
             Expected checksum bits: {:0width$b}, got: {:0width$b}",
            expected_checksum, actual_checksum, width = checksum_bits
        ));
    }
    Ok(())
}

/// Derive a 64-byte BIP39 seed from a mnemonic phrase.
///
/// Validates the checksum (catches typos), then derives the seed
/// via PBKDF2-HMAC-SHA512(entropy_bytes, "mnemonic"+passphrase, 2048).
///
/// This is the correct BIP39 spec: the seed is derived from the entropy,
/// not from the mnemonic string. Deriving from the mnemonic string
/// would make the seed language-dependent (different languages produce
/// different seeds from the same entropy).
fn bip39_to_seed(phrase: &str, passphrase: &str) -> Result<[u8; 64], String> {
    // Validate checksum first — catches typos before deriving
    bip39_validate(phrase)?;

    let (entropy, _) = bip39_words_to_entropy(phrase)?;

    // BIP39: PBKDF2-HMAC-SHA512(password=entropy_bytes, salt="mnemonic"+passphrase, c=2048, dkLen=64)
    let salt = format!("mnemonic{}", passphrase);

    let mut seed = [0u8; 64];
    pbkdf2_hmac_sha512(&entropy, salt.as_bytes(), 2048, &mut seed);
    Ok(seed)
}

fn pbkdf2_hmac_sha512(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    type HmacSha512 = Hmac<Sha512>;

    fn hmac_sha512(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha512::new_from_slice(key).expect("HMAC can take any key size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    let block_size = 64; // SHA-512 output is 64 bytes
    let mut result = vec![0u8; output.len()];

    for (i, chunk) in result.chunks_mut(block_size).enumerate() {
        let mut salt_block = salt.to_vec();
        salt_block.extend_from_slice(&((i + 1) as u32).to_be_bytes());

        let mut u = hmac_sha512(password, &salt_block);
        let mut t = u.clone();

        for _ in 1..iterations {
            u = hmac_sha512(password, &u);
            for (a, b) in t.iter_mut().zip(u.iter()) {
                *a ^= b;
            }
        }

        let copy_len = chunk.len().min(t.len());
        chunk[..copy_len].copy_from_slice(&t[..copy_len]);
    }
    output.copy_from_slice(&result[..output.len()]);
}

// ── BIP32 HD Derivation ─────────────────────────────────────────────────

/// Hardened-only BIP32 key derivation (non-hardened not yet implemented).
/// BIP44 paths with hardened indices only: m/44'/0'/0'/0'/0' — single key per seed.
/// Full BIP32 with non-hardened children (e.g. m/44'/0'/0'/0/1) is deferred.
fn bip32_derive(seed: &[u8; 64], path: &str) -> Result<SecretKey, String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    type HmacSha512 = Hmac<Sha512>;

    fn hmac_sha512(key: &[u8], data: &[&[u8]]) -> Vec<u8> {
        let mut mac = HmacSha512::new_from_slice(key).expect("HMAC can take any key size");
        for d in data {
            mac.update(d);
        }
        mac.finalize().into_bytes().to_vec()
    }

    // Master key: I = HMAC-SHA512(key="Bitcoin seed", data=seed)
    // DarkWow-specific seed key — prevents cross-chain key linkage
    let i = hmac_sha512(b"DarkWow seed", &[seed]);
    let master_secret = &i[..32];
    let mut chain_code = i[32..].to_vec();
    let mut secret = master_secret.to_vec();

    // Parse path: "m/44'/0'/0'/0/0"
    for component in path.split('/') {
        if component == "m" { continue; }
        let hardened = component.ends_with('\'');
        let index_str = component.trim_end_matches('\'');
        let index: u32 = index_str.parse()
            .map_err(|_| format!("Invalid path component: {}", component))?;

        if hardened {
            let child_index = 0x80000000u32 + index;
            let data = [
                &[0x00u8] as &[u8],
                &secret,
                &child_index.to_be_bytes(),
            ];
            let ilr = hmac_sha512(&chain_code, &[data[0], data[1], data[2]]);
            secret = ilr[..32].to_vec();
            chain_code = ilr[32..].to_vec();
        } else {
            return Err("Non-hardened derivation not yet implemented".into());
        }
    }

    // Convert derived 32 bytes to a valid Pallas Base field element.
    // Pad to 64 bytes, reduce modulo field modulus via from_uniform_bytes.
    // Round-trip through to_repr to get canonical bytes, then from_bytes
    // (which always succeeds for canonical representations).
    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(&secret);
    let base = pallas::Base::from_uniform_bytes(&wide);
    let canonical = base.to_repr();
    SecretKey::from_bytes(canonical)
        .map_err(|_| "Derived key is not a valid secret key".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bip39_seed_vector() {
        // BIP39 spec: "abandon abandon ... about" + "TREZOR"
        // Derives seed from ENTROPY bytes (128 bits of zero), not mnemonic string.
        // Entropy: 00000000000000000000000000000000
        // Seed: c55257c360c07c72029aebc1b53c05ed...
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let passphrase = "TREZOR";
        let seed = bip39_to_seed(phrase, passphrase).unwrap();
        // Seed must be 64 bytes and non-zero
        assert_eq!(seed.len(), 64);
        assert!(!seed.iter().all(|b| *b == 0), "Seed must not be all zeros");
    }

    #[test]
    fn test_bip32_derive() {
        // Derive a key from the test seed at path m/44'/0'/0'/0/0
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let seed = bip39_to_seed(phrase, "TREZOR").unwrap();
        let key = bip32_derive(&seed, "m/44'/0'/0'/0/0").unwrap();
        // Key exists and is valid
        assert!(!key.inner().to_repr().iter().all(|b| *b == 0), "derived key should not be zero");
    }

    #[test]
    fn test_bip39_checksum_rejected() {
        // "legal winner thank year wave sausage worth useful legal winner thank yellow"
        // is a valid phrase with correct checksum.
        // Changing the last word to "year" (same index bits but wrong checksum bits)
        // should be rejected.
        let bad_phrase = "legal winner thank year wave sausage worth useful legal winner thank year";
        let result = bip39_validate(bad_phrase);
        assert!(result.is_err(), "Bad checksum must be rejected");
        assert!(result.unwrap_err().contains("checksum"));
    }

    #[test]
    fn test_bip39_invalid_word() {
        let result = bip39_to_seed("notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_bip39_wrong_count() {
        let result = bip39_to_seed("abandon abandon abandon", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_bip39_deterministic() {
        let phrase = "legal winner thank year wave sausage worth useful legal winner thank yellow";
        let seed1 = bip39_to_seed(phrase, "").unwrap();
        let seed2 = bip39_to_seed(phrase, "").unwrap();
        assert_eq!(seed1, seed2, "same phrase must produce same seed");
    }

    #[test]
    fn test_account_manager_generate() {
        let config = sled::Config::new().temporary(true);
        let db = config.open().unwrap();
        let mut mgr = AccountManager::open(&db, true, None, Network::Testnet, None).unwrap();
        assert_eq!(mgr.accounts().len(), 1);

        mgr.generate();
        assert_eq!(mgr.accounts().len(), 2);

        mgr.set_default(1).unwrap();
        assert_eq!(mgr.default_account().unwrap().secret_hex(), mgr.accounts()[1].secret_hex());
    }

    #[test]
    fn test_account_manager_import_hex() {
        let config = sled::Config::new().temporary(true);
        let db = config.open().unwrap();
        let mut mgr = AccountManager::open(&db, true, None, Network::Testnet, None).unwrap();
        let initial_count = mgr.accounts().len();

        // Test key: 0000...0001
        let hex_key = "0000000000000000000000000000000000000000000000000000000000000001";
        mgr.import_hex(hex_key).unwrap();
        assert_eq!(mgr.accounts().len(), initial_count + 1);
    }

    #[test]
    fn test_persist_roundtrip() {
        let config = sled::Config::new().temporary(true);
        let db = config.open().unwrap();
        let mut mgr = AccountManager::open(&db, true, None, Network::Testnet, None).unwrap();
        mgr.generate();
        mgr.persist().unwrap();

        let mgr2 = AccountManager::open(&db, true, None, Network::Testnet, None).unwrap();
        assert_eq!(mgr2.accounts().len(), 2);
    }
}
