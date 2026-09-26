/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
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

//! The nine genesis contract ids, pinned in their base58 form — the external report's finding 7.
//!
//! That finding observed that only Deployooor's id is written anywhere in the tree: the other eight
//! are derivable from `poseidon_hash([42, 0, counter])` and are pinned nowhere, so a change that
//! silently altered the derivation, the prefix, or a counter would be invisible — and it cannot be
//! caught by a rebuild, because `contract_id.rs` is in every contract's `SOURCE_MANIFEST` and its
//! derived values reach the chain only through the deployment transactions. A test that names the
//! nine values is the check, and it doubles as the place a reader can look them up: `genesis.md`
//! gives the derivation, this gives the result.
//!
//! The base58 strings were read from this program's own output rather than recalled, which is the
//! same rule the pin ceremony follows: print the value, then record it.

use dwow_sdk::crypto::contract_id::{
    ATTESTATION_CONTRACT_ID, BOX_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID, IDENTITY_CONTRACT_ID,
    MULTISIG_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID, ORACLE_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID,
    PURSE_CONTRACT_ID,
};

#[test]
fn the_nine_genesis_contract_ids_are_what_they_were() {
    // (counter, name, base58) — the counters are `genesis.md`'s table, and the deployment order is a
    // different list on purpose (`genesis_contracts()`; see that page's note). Each string is what
    // `poseidon_hash([42, 0, counter])` renders as, and the test is the pin: changing the prefix, a
    // counter, or the hash changes a line here rather than shipping silently.
    let named: [(u8, &str, &str, &dwow_sdk::crypto::ContractId); 9] = [
        (2, "Deployooor", "EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN", &DEPLOYOOOR_CONTRACT_ID),
        (3, "PromissoryNote", "21LYoifepcySKhyDA1vzxRDWGHyDizPQ8f11zSqhep7t", &PROMISSORY_NOTE_CONTRACT_ID),
        (4, "NativeToken", "DgmXpuU1EcM54E8GuNTAkBUThcCoYzGN5kRCNXA4cPtw", &NATIVE_TOKEN_CONTRACT_ID),
        (5, "Identity", "AyJtw5sxYrKBkeec73hxLDUPh6ZY32gXRagWcZ3hctBA", &IDENTITY_CONTRACT_ID),
        (6, "Oracle", "DkrPpNQERff36c7B7qhryCfpnzUjGKcYipbpegKxVYnr", &ORACLE_CONTRACT_ID),
        (7, "Attestation", "5sKmJNgZCjJ2sFjxzpwS9R1sLyrNc9DZ6gL1KcDHihfe", &ATTESTATION_CONTRACT_ID),
        (8, "Purse", "8v6z9CTT7ed8fDdyY9iNBL9DY8co5GqMyNT7azQcFZPB", &PURSE_CONTRACT_ID),
        (9, "Box", "2afzyKdAkNu9tVZe7aug7xPBcgjGkGZRC3bH6doDWEy7", &BOX_CONTRACT_ID),
        (10, "MultiSig", "G7ZRpbi8AQeU38JGYehRRyirPpcM3WXqSZGxXCQVLYpn", &MULTISIG_CONTRACT_ID),
    ];

    let mut seen = std::collections::BTreeSet::new();
    for (counter, name, pinned, id) in named {
        assert_eq!(
            id.to_string(),
            pinned,
            "counter {counter} ({name}) no longer derives its pinned id"
        );
        assert!(seen.insert(pinned), "{name}'s id is not distinct from an earlier one");
    }
    assert_eq!(seen.len(), 9, "nine ids, nine distinct strings");
    // The deployment order is NOT the counter order — NativeToken has counter 4 at position 2 —
    // so this test pins identity, and `genesis_contracts()` pins order.
    assert_ne!(
        DEPLOYOOOR_CONTRACT_ID.to_string(),
        NATIVE_TOKEN_CONTRACT_ID.to_string(),
    );
}
