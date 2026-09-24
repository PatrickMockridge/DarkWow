//! Manifest ↔ dispatch agreement for the ZK functions of the genesis contracts.
//!
//! OBL-C90. The manifest is the type declaration a wallet reads: `ocap.md` §7 makes it the basis
//! of capability construction, and `sdk/src/contract_client.rs:412-419` is where the generic path
//! acts on it —
//!
//! ```text
//! let proof_bytes: Vec<u8> = if func.requires_proof {
//!     let circuit_name = func.proof_circuit.as_deref().unwrap_or("none");
//!     let circuit = self.manifest.circuits.iter().find(|c| c.name == circuit_name)
//!         .ok_or_else(|| format!("requires proof circuit '{circuit_name}' but manifest has no
//!                                 [[circuits]] entry for it"))?;
//! ```
//!
//! So a function that declares `requires_proof = true` and names no circuit resolves to the
//! literal string `"none"`, finds no such circuit, and **cannot be built at all** through the
//! generic path. That is what four functions across two genesis contracts did until 2026-09-23:
//! `oracle`'s `set_oracle_active` declared neither field (while its five siblings declared both),
//! and `multisig`'s `create_group`, `sign` and `finalize` declared `requires_proof` but no
//! `proof_circuit` while its `[[circuits]]` section listed exactly the three circuits they need.
//!
//! **The lookup has two halves, and the first repair reached only one of them.** Naming the
//! circuit is necessary; `find` must also *find* it. `oracle`'s `[[circuits]]` section listed the
//! five circuits its five older functions use and never gained the sixth — `set_oracle_active.zk`
//! declares `circuit "SetOracleActiveV2"`, and the entrypoint registers and pushes exactly that
//! namespace, so the circuit is built and on-chain while the manifest's own list contradicts it.
//! With the field added and the entry still missing, `find` returns `None` one line later and the
//! call fails for the same reason under a different message. That is why this file asserts the
//! **resolution** rather than the field: an assertion on `proof_circuit` alone passes while the
//! instruction stays unreachable, which is the shape of check this repository has rejected before.
//!
//! The heavyweight specs do not catch this, and that is why it survived: they build their proofs
//! directly through the harness and never read `requires_proof`. Nothing read the field, so
//! nothing noticed it was wrong — which is the shape this test exists to close.
//!
//! The expected circuit is taken from the **contract's own constant** rather than transcribed, so
//! this cannot pass by agreeing with itself: the value asserted is the namespace the dispatch
//! pushes, imported from the crate that pushes it. The class gate below is separate and needs no
//! constant — it asks the manifest two questions it can answer about itself.

/// The `proof_circuit` a manifest declares for `function`, or `None` if it declares none.
///
/// Deliberately narrow: it reads the one field, in the one block, and does not try to interpret
/// comments or conditionals — a check that tried to be cleverer is the classifier this repository
/// has twice rejected.
fn declared_proof_circuit<'a>(manifest: &'a str, function: &str) -> Option<&'a str> {
    let needle = format!("name = \"{function}\"");
    for block in manifest.split("[[functions]]").skip(1) {
        let block = block.split("[[").next().unwrap_or(block);
        if !block.contains(&needle) {
            continue;
        }
        for line in block.lines() {
            if let Some(rest) = line.trim().strip_prefix("proof_circuit = \"") {
                return rest.strip_suffix('"');
            }
        }
        return None;
    }
    None
}

/// The names of the circuits a manifest declares in its `[[circuits]]` section.
///
/// Same narrowness as `declared_proof_circuit`: one field, one block kind.
fn declared_circuits(manifest: &str) -> Vec<&str> {
    manifest
        .split("[[circuits]]")
        .skip(1)
        .filter_map(|block| {
            let block = block.split("[[").next().unwrap_or(block);
            block.lines().find_map(|line| {
                line.trim().strip_prefix("name = \"")?.strip_suffix('"')
            })
        })
        .collect()
}

/// The names of the functions a manifest declares `requires_proof = true` for, in file order.
fn functions_requiring_proof(manifest: &str) -> Vec<&str> {
    manifest
        .split("[[functions]]")
        .skip(1)
        .filter_map(|block| {
            let block = block.split("[[").next().unwrap_or(block);
            if !block.lines().any(|l| l.trim() == "requires_proof = true") {
                return None;
            }
            block.lines().find_map(|line| {
                line.trim().strip_prefix("name = \"")?.strip_suffix('"')
            })
        })
        .collect()
}

const ORACLE_MANIFEST: &str = include_str!("../../oracle/manifest.toml");
const MULTISIG_MANIFEST: &str = include_str!("../../multisig/manifest.toml");

/// The genesis contracts whose manifests are **deployed**, i.e. the ones whose bytes ride in the
/// genesis transaction and are read back by a wallet through `contract_client.rs`.
///
/// `deployooor` and `native_token` are absent deliberately, and for the reason
/// `bin/dwowd/src/lib.rs`'s genesis table states: both deploy `&[]` — no manifest bytes — so
/// neither has a declaration a wallet can act on. `native_token`'s on-disk manifest is the FYI
/// document its own header says it is, and its `transfer` and `spend` name no circuit for a
/// reason this schema cannot express: they spend a `Burn_V2` and a `Mint_V2` in one instruction,
/// and `ManifestFunction` carries a single `proof_circuit`. Gating it here would demand a shape
/// the type does not have; the multi-proof schema that would is its own work, recorded in the
/// register rather than smuggled into this test.
const DEPLOYED_GENESIS_MANIFESTS: &[(&str, &str)] = &[
    ("promissory_note", include_str!("../../promissory_note/manifest.toml")),
    ("identity", include_str!("../../identity/manifest.toml")),
    ("oracle", ORACLE_MANIFEST),
    ("attestation", include_str!("../../attestation/manifest.toml")),
    ("purse", include_str!("../../purse/manifest.toml")),
    ("box", include_str!("../../box/manifest.toml")),
    ("multisig", MULTISIG_MANIFEST),
];

#[test]
fn genesis_zk_functions_declare_the_circuit_their_dispatch_pushes() {
    // (contract, manifest, function, the namespace its metadata arm pushes)
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "oracle",
            ORACLE_MANIFEST,
            "set_oracle_active",
            dwow_oracle_contract::ORACLE_CONTRACT_ZKAS_SET_ORACLE_ACTIVE_NS_V2,
        ),
        (
            "multisig",
            MULTISIG_MANIFEST,
            "create_group",
            dwow_multisig_contract::MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V2,
        ),
        (
            "multisig",
            MULTISIG_MANIFEST,
            "sign",
            dwow_multisig_contract::MULTISIG_CONTRACT_ZKAS_SIGN_NS_V2,
        ),
        (
            "multisig",
            MULTISIG_MANIFEST,
            "finalize",
            dwow_multisig_contract::MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V2,
        ),
    ];

    for (contract, manifest, function, pushed) in cases {
        assert_eq!(
            declared_proof_circuit(manifest, function),
            Some(*pushed),
            "{contract}: '{function}' declares a proof, so its manifest must name the circuit its \
             dispatch pushes ({pushed}) — with the field absent the generic client resolves the \
             circuit name to \"none\", finds no [[circuits]] entry for it, and the call cannot be \
             built (OBL-C90)"
        );
    }

    // The controls, without which a parser that always returned `Some` would pass the loop above.
    assert_eq!(
        declared_proof_circuit(MULTISIG_MANIFEST, "initialize"),
        None,
        "a function that needs no proof must declare no circuit — `initialize` is the deploy-time \
         instruction and its metadata arm answers with the empty buffer"
    );
    assert_eq!(
        declared_proof_circuit(MULTISIG_MANIFEST, "no_such_function"),
        None,
        "an undeclared function must resolve to nothing rather than to a neighbouring block"
    );
}

/// The class gate: every ZK function of a deployed genesis manifest resolves, in that same
/// manifest, to a circuit it declares.
///
/// The test above asks the strong question about four functions and needs a constant per case. This
/// one needs none — it holds for every ZK function of every deployed genesis contract, and for
/// contracts not yet written — and it is the half that the `proof_circuit` assertion alone cannot
/// reach: `oracle`'s `set_oracle_active` named a circuit its own `[[circuits]]` section did not
/// list, so `find` returned `None` and the instruction stayed unreachable with the field present.
///
/// Measured before it was written: across the seven deployed manifests, four functions were in the
/// first half (the explicit cases above) and one was in the second (`set_oracle_active`, repaired
/// in `oracle/manifest.toml` in the same change). Three further instances exist outside the
/// genesis set — `baccarat`'s `commit_bet` and `settle_bet` name `CommitBetV2`/`SettleBetV2` where
/// the built circuits are `CommitBet_V2`/`SettleBet_V2`, and `labor_market` and `stablecoin` each
/// declare circuits with no `.zk` behind them — and they are recorded in the register rather than
/// fixed here, because none is a genesis contract and each moves only its own source hash.
#[test]
fn every_deployed_genesis_manifest_resolves_its_own_proof_circuits() {
    let mut checked = 0usize;

    for (contract, manifest) in DEPLOYED_GENESIS_MANIFESTS {
        let declared = declared_circuits(manifest);
        assert!(
            !declared.is_empty(),
            "{contract}: a deployed manifest with no [[circuits]] section cannot serve the \
             generic path at all"
        );

        for function in functions_requiring_proof(manifest) {
            let named = declared_proof_circuit(manifest, function).unwrap_or_else(|| {
                panic!(
                    "{contract}: '{function}' declares `requires_proof = true` and names no \
                     circuit. The generic client resolves the name to the literal \"none\", finds \
                     no [[circuits]] entry for it, and the call cannot be built (OBL-C90)"
                )
            });
            assert!(
                declared.contains(&named),
                "{contract}: '{function}' names proof circuit '{named}', but this manifest's \
                 [[circuits]] section declares {declared:?} — the lookup finds no entry and the \
                 call cannot be built (OBL-C90)"
            );
            checked += 1;
        }
    }

    // The control: the count is the number of ZK functions in the seven deployed manifests, so a
    // parser that silently stopped finding functions would be caught rather than pass vacuously.
    assert_eq!(
        checked, 32,
        "the seven deployed genesis manifests declare 32 proof-bearing functions; a different \
         count means the parser above stopped seeing them, not that the contracts changed"
    );
}

// ============================================================================
// OBL-C91 — the fourth artefact, checked against the other three
//
// The genesis manifests above are checked by a *constant*: the expected circuit comes from the
// contract's own namespace constant, so the assertion cannot pass by agreeing with itself. That
// needs an import per contract, and it is why nothing looked at the other twenty-three. This block
// is the half that needs no constant: it asks each contract's manifest four questions it can
// answer about itself, against the `circuit "…"` names in that contract's own `proof/*.zk`.
//
// The enumeration is **by directory presence**, not by a list, for the reason
// `check-circuit-metadata-alignment.sh` records under `OBL-C79`: a hardcoded list called `GENESIS`
// that examined 11 of 32 contracts while printing `PASS` is how six broken-proof contracts stayed
// invisible. A future allowlist cannot return quietly here: the covered set is printed, and the
// contracts the checks do not apply to are named with their reason.
//
// What each check is worth, and what it is not: (A) and (B) are the generic client's own lookup —
// a function whose `proof_circuit` resolves to nothing, and a `[[circuits]]` entry that names a
// circuit no `.zk` builds, each make the instruction unbuildable through `contract_client.rs`.
// (D) is weaker and is checked as a named list rather than a rule: a declared circuit no function
// names is *sometimes* correct — `native_token`'s `transfer` and `spend` spend a `Burn_V2` and a
// `Mint_V2` in one instruction and the schema carries one `proof_circuit` — so the sites are
// declared with their measurements instead of being asserted away. The exception tables below are
// a ratchet: a *new* site fails, and every entry names the row that owns it.
// ============================================================================

/// Contracts whose `manifest.toml` is **not deployed**, so no wallet reads it: the genesis table in
/// `bin/dwowd/src/lib.rs` deploys `&[]` for these. The checks still run over them — this list is
/// what they are excepted *by*, so the omission is stated rather than achieved by leaving them off.
const MANIFESTS_NOT_DEPLOYED: &[&str] = &["deployooor", "native_token"];

/// `OBL-C91` (A): a function declaring `requires_proof = true` whose `proof_circuit` names nothing
/// in its own `[[circuits]]`. `(contract, function, why)`.
const UNDECLARED_PROOF_CIRCUIT: &[(&str, &str, &str)] = &[
    ("native_token", "transfer", "not deployed (manifest is an FYI document) and spends a Burn_V2 + a Mint_V2 in one instruction; `ManifestFunction` carries a single `proof_circuit`, so no value would resolve — OBL-C91's multi-proof clause"),
    ("native_token", "spend", "as `transfer` above — the same instruction shape named twice"),
];

/// `OBL-C91` (B): a `[[circuits]]` name no `proof/*.zk` declares — the instruction cannot be built
/// because the circuit was never written. `(contract, circuit, why)`.
const DECLARED_WITHOUT_CIRCUIT: &[(&str, &str, &str)] = &[
    ("stablecoin", "RedeemStableV1", "no `proof/*.zk` declares any `RedeemStable*`; the arm pushes the namespace and the manifest declares it, so the decision is whether the function gets the circuit or the manifest stops claiming one — that decision is the work, and repairing the declaration without making it would bury it (OBL-C91)"),
    ("labor_market", "CreateJobWithCapabilityV2", "no `proof/*.zk` declares it; its sibling `CreateJobV2` does — same decision owed as `stablecoin`'s (OBL-C91)"),
    ("labor_market", "CreateJobWithMilestonesAndCapabilityV2", "no `proof/*.zk` declares it — same decision owed (OBL-C91)"),
];

/// `OBL-C91` (D): a declared circuit that no function names. `(contract, circuit, why)`.
const DECLARED_WITHOUT_FUNCTION: &[(&str, &str, &str)] = &[
    ("native_token", "Mint_V2", "the second circuit of the transfer/spend pair — legitimate, and the schema cannot express it"),
    ("lottery", "InitializeV2", "the initialize arm publishes an empty instance vector by design and no client proves it; adjudicated in OBL-C78 rather than here"),
    ("lottery", "DrawWinnersV2", "measured: the manifest's `draw_winners` function declares no `proof_circuit` while the arm pushes this namespace — a missing declaration, not a stray circuit"),
    ("lottery", "ClaimPrizeV2", "as `DrawWinnersV2`: `claim_prize` declares none, the arm pushes this"),
    ("lottery", "ExpireLotteryV2", "as `DrawWinnersV2`, and the manifest's function block that should declare it is *named* after this circuit instead — filed as OBL-C120"),
    ("labor_market", "MilestonePaymentV2", "no function names it; the manifest's milestone functions (`submit_milestone`, `confirm_milestone`) declare none, and two of its blocks are named after circuits — filed as OBL-C120"),
];

fn excepted(table: &[(&str, &str, &str)], contract: &str, name: &str) -> bool {
    table.iter().any(|(c, n, _)| *c == contract && *n == name)
}

/// Every `circuit "…"` name declared by the `.zk` files of `contract`.
fn zk_circuit_names(crate_dir: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(crate_dir.join("proof")) else {
        return names;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("zk") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("circuit \"") {
                if let Some(name) = rest.split('"').next() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The `(name, proof_circuit, requires_proof)` of every function a manifest declares.
fn manifest_functions(manifest: &str) -> Vec<(String, Option<String>, bool)> {
    manifest
        .split("[[functions]]")
        .skip(1)
        .filter_map(|block| {
            let block = block.split("[[").next().unwrap_or(block);
            let mut name = None;
            let mut circuit = None;
            let mut requires = false;
            for line in block.lines() {
                let s = line.trim();
                if let Some(rest) = s.strip_prefix("name = \"") {
                    name = rest.strip_suffix('"').map(str::to_string);
                } else if let Some(rest) = s.strip_prefix("proof_circuit = \"") {
                    circuit = rest.strip_suffix('"').map(str::to_string);
                } else if s == "requires_proof = true" {
                    requires = true;
                }
            }
            name.map(|n| (n, circuit, requires))
        })
        .collect()
}

fn manifest_circuits(manifest: &str) -> Vec<String> {
    manifest
        .split("[[circuits]]")
        .skip(1)
        .filter_map(|block| {
            let block = block.split("[[").next().unwrap_or(block);
            block.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("name = \"")?
                    .strip_suffix('"')
                    .map(str::to_string)
            })
        })
        .collect()
}

/// The four checks over **every** contract that ships both a manifest and a proof directory.
#[test]
fn every_contract_manifest_agrees_with_its_own_circuits() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut contracts: Vec<String> = std::fs::read_dir(&root)
        .expect("the contracts directory is readable")
        .flatten()
        .filter(|e| e.path().join("manifest.toml").is_file() && e.path().join("proof").is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    contracts.sort();

    let mut sites = 0usize;
    for contract in &contracts {
        let path = root.join(contract).join("manifest.toml");
        let manifest = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: unreadable manifest: {e}", path.display()));
        let declared = manifest_circuits(&manifest);
        let functions = manifest_functions(&manifest);
        let zk = zk_circuit_names(&root.join(contract));

        // (A) a function that requires a proof must name a circuit its own manifest declares.
        for (name, circuit, requires) in &functions {
            if !requires {
                continue;
            }
            sites += 1;
            let ok = circuit.as_ref().is_some_and(|c| declared.contains(c));
            assert!(
                ok || excepted(UNDECLARED_PROOF_CIRCUIT, contract, name),
                "{contract}: function '{name}' declares `requires_proof = true` and names \
                 {circuit:?}, which its own [[circuits]] section ({declared:?}) does not declare — \
                 the generic client resolves the name, finds no entry and cannot build the call \
                 (OBL-C91)"
            );
        }

        // (B) every declared circuit must be one a `.zk` file builds.
        for name in &declared {
            sites += 1;
            assert!(
                zk.contains(name) || excepted(DECLARED_WITHOUT_CIRCUIT, contract, name),
                "{contract}: [[circuits]] declares '{name}' and no `circuit \"{name}\"` exists in \
                 its proof/ directory ({zk:?}) — the entry names a circuit that was never built \
                 (OBL-C91)"
            );
        }

        // (C) and every built circuit must be declared. No exceptions: measured clean on
        // 2026-09-24, so this is the one letter that must stay at zero.
        for name in &zk {
            sites += 1;
            assert!(
                declared.contains(name),
                "{contract}: proof/*.zk builds '{name}' and [[circuits]] does not declare it \
                 ({declared:?}) — a wallet cannot resolve it (OBL-C91)"
            );
        }

        // (D) and every declared circuit should be named by some function.
        for name in &declared {
            sites += 1;
            let named = functions.iter().any(|(_, c, _)| c.as_deref() == Some(name.as_str()));
            assert!(
                named || excepted(DECLARED_WITHOUT_FUNCTION, contract, name),
                "{contract}: [[circuits]] declares '{name}' and no function names it — either a \
                 function's `proof_circuit` is missing, or the entry is stray (OBL-C91)"
            );
        }
    }

    println!(
        "OBL-C91: {} manifest(s) with a proof directory checked ({contracts:?}); \
         {} site(s); {} not deployed and excepted by name ({MANIFESTS_NOT_DEPLOYED:?})",
        contracts.len(),
        sites,
        // The third placeholder is the *count* of the list the fourth names; it was missing, so this
        // `println!` did not compile and took `make test` with it (`3 positional arguments in format
        // string, but there are 2 arguments`). Found behind the E0061 in the sibling file, which is
        // why the run's log showed only one of the two.
        MANIFESTS_NOT_DEPLOYED.len(),
    );

    // The control: a parser that quietly stopped finding functions or circuits would otherwise
    // pass every check vacuously. The counts are measurements of the tree and move only when a
    // contract is added or removed — which is when a reader should look.
    //
    // **Re-measured 2026-09-24, and the movement is the reason the control is worth its
    // maintenance cost**: they read 25 and 388 when this test was written, and the four checks had
    // been *passing on every site* the whole time the numbers were wrong — the panic was the
    // control's, after the loop, so the A–D results above it were real. Six more contracts ship
    // both a manifest and a proof directory now (auction, drain_protection, game_room, pool_stake,
    // relayer_endowment and darkbet_exchange), and the site count grew with them. A count that
    // fails loudly on drift is the opposite of the hardcoded-coverage failure `OBL-C79` records;
    // what it must not become is a number nobody re-measures.
    assert_eq!(
        contracts.len(),
        31,
        "31 contracts ship both a manifest.toml and a proof/ directory (measured 2026-09-24); a \
         different count means either the enumeration above changed or a contract moved — and the \
         contract list is printed above, so the reader can tell which"
    );
    assert_eq!(
        sites, 673,
        "the four checks walk 673 sites over those manifests (measured 2026-09-24); a different \
         count means a parser stopped seeing them, or the tree moved"
    );
}
