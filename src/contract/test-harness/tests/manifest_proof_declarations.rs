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
