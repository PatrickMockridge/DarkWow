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

//! Every codec a model struct carries agrees with that struct's field list (`OBL-C112`).
//!
//! `Plan` was encoded four ways: a `dwow_serial::Encodable`/`Decodable` pair, an async pair, and
//! the model's slice `encode`/`decode` — the one the contract and the client actually call. When
//! `OBL-C105` added `uses_allowed` and `rate_period`, the fields reached the serial pair and the
//! async pair but not the slice `encode`: it kept writing 99 bytes while its `decode` expected 115,
//! so the wasm read past the end of the host's buffer and the run died with `slice_index_fail`
//! **inside `Plan::decode`** — a panic, not a `ContractError`, which is why the contract's own
//! error path never saw it.
//!
//! `integration.rs`'s `test_plan_encoding` did not catch it either: it round-trips `serialize` /
//! `deserialize`, which is the serial pair — the *other* codec. One of four homes was covered, and
//! the three cases below are the ones the old length guards let through.
//!
//! The repair is one codec per struct, the others delegating, and a length guard computed from the
//! same layout constants the writer uses. These tests are its control.

use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{pasta_prelude::Group, PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_subscription_contract::model::{
    Plan, Subscription, SubscriptionId, SubscriptionState,
};

/// `integration.rs`'s helper, copied: a public key from a numeric seed.
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from_base(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// A plan with an allowance of 3 uses per 1000 blocks, with or without the escrow bulla.
fn make_plan(required_dao_escrow: Option<pallas::Base>) -> Plan {
    Plan {
        version: 0,
        id: 1,
        name_hash: pallas::Base::from(1),
        price: 1000,
        asset_id: pallas::Base::zero(),
        duration_blocks: 10000,
        treasury_share: 8000,
        endowment_share: 2000,
        active: true,
        dao_escrow_discount: 2000,
        required_dao_escrow,
        uses_allowed: 3,
        rate_period: 1000,
    }
}

/// A record, with or without each of its two optional fields.
fn make_subscription(
    dao_escrow_bulla: Option<pallas::Base>,
    dao_membership_note: Option<pallas::Base>,
) -> Subscription {
    Subscription {
        version: 0,
        id: SubscriptionId(pallas::Base::from(1)),
        subscriber_pubkey: make_pubkey(1),
        plan_id: 1,
        lock_until_block: 100000,
        deposit: 1000,
        asset_id: pallas::Base::zero(),
        value_commit: Group::identity(),
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: 50000,
        dao_escrow_bulla,
        dao_membership_note,
        uses_allowed: 3,
        rate_period: 1000,
        period_uses: 0,
        last_access_block: 0,
        uses_remaining: 3,
        instance_seed: [0u8; 32],
    }
}

/// The writer's own length is the one the guard is derived from: a field added to `encode` without
/// the layout constants moving fails here, and the slice `decode`'s guard moves with the constants.
#[test]
fn plan_encoded_length_is_the_declared_one() {
    assert_eq!(Plan::MIN_ENCODED_SIZE, 115);
    assert_eq!(make_plan(None).encode().len(), Plan::MIN_ENCODED_SIZE);
    assert_eq!(
        make_plan(Some(pallas::Base::from(2))).encode().len(),
        Plan::MIN_ENCODED_SIZE + 32
    );
}

/// The three codecs of `Plan` must produce the same bytes and read them back to the same value.
/// The serial pair is the one `test_plan_encoding` covers; this is the comparison it lacked.
#[test]
fn plan_codecs_agree_byte_for_byte() {
    for plan in [make_plan(None), make_plan(Some(pallas::Base::from(2)))] {
        let slice_bytes = plan.encode();

        // `dwow_serial::serialize` writes no length prefix, so the serial codec's bytes are
        // comparable to the slice codec's directly.
        assert_eq!(serialize(&plan), slice_bytes, "the serial codec is not the slice codec");

        // The trait decoder, by hand, so the reader's position is visible.
        let mut cursor = std::io::Cursor::new(&slice_bytes[..]);
        let decoded = <Plan as dwow_serial::Decodable>::decode(&mut cursor)
            .expect("the serial codec reads its own bytes");
        assert_eq!(cursor.position() as usize, slice_bytes.len(), "the decoder under-read");
        assert_eq!(decoded.encode(), slice_bytes);

        // And through `deserialize`, which requires the byte stream to be consumed entirely — the
        // check that a decoder reading too few bytes would fail.
        let via_deserialize: Plan =
            deserialize(&slice_bytes).expect("the stream is consumed entirely");
        assert_eq!(via_deserialize.encode(), slice_bytes);
    }
}

/// Two plans back to back: the decoder must consume exactly one and stop, or the second read starts
/// inside the first. The delegating decoder reads a computed number of bytes, so this is the test
/// that says the arithmetic is the writer's and not a guess.
#[test]
fn plan_stream_codec_reads_exactly_one_plan() {
    for (first, second) in [
        (make_plan(Some(pallas::Base::from(2))), make_plan(None)),
        (make_plan(None), make_plan(Some(pallas::Base::from(2)))),
    ] {
        let mut stream = serialize(&first);
        stream.extend_from_slice(&serialize(&second));

        let mut cursor = std::io::Cursor::new(&stream[..]);
        let got_first = <Plan as dwow_serial::Decodable>::decode(&mut cursor).expect("first plan");
        let got_second = <Plan as dwow_serial::Decodable>::decode(&mut cursor).expect("second plan");
        assert_eq!(got_first.encode(), first.encode());
        assert_eq!(got_second.encode(), second.encode());
        assert_eq!(cursor.position() as usize, stream.len(), "the stream was not read exactly");
    }
}

/// `OBL-C105`'s fields are what an old-format buffer lacks. Before this row, `MIN_ENCODED_SIZE`
/// was still 99 — measured on the tree, against the row's claim that it had moved to 115 — so a
/// 99-byte buffer passed the guard and the reads below it ran off the end: `slice_index_fail`.
/// The row's claim was wrong and the guard was the one that mattered; this asserts the refusal.
#[test]
fn plan_decode_refuses_the_pre_allowance_format() {
    let bytes = make_plan(None).encode();
    let old_format = &bytes[..99];

    let err = Plan::decode(old_format).expect_err("an old-format buffer is refused");
    let msg = format!("{err}");
    assert!(msg.contains("expected 115 bytes, got 99"), "unexpected error: {msg}");
}

/// A buffer whose presence byte promises a bulla the buffer does not carry. The minimum guard let
/// this through — 115 bytes is above 99 — and the bulla read ran past the end.
#[test]
fn plan_decode_refuses_a_bulla_that_is_not_there() {
    let mut bytes = make_plan(None).encode();
    bytes[Plan::FIXED_PREFIX - 1] = 1;

    let err = Plan::decode(&bytes).expect_err("a bulla that is not there is refused");
    let msg = format!("{err}");
    assert!(msg.contains("expected 147 bytes, got 115"), "unexpected error: {msg}");
}

/// The same two properties for the record, whose guard had the same shape: 264 was the *minimum*,
/// while the two presence bytes make the real length variable. A 264-byte record whose first flag
/// claims a bulla was read five `u64`s past its end.
#[test]
fn subscription_encoded_length_is_the_declared_one() {
    assert_eq!(Subscription::MIN_ENCODED_SIZE, 264);
    assert_eq!(make_subscription(None, None).encode().len(), Subscription::MIN_ENCODED_SIZE);
    assert_eq!(
        make_subscription(Some(pallas::Base::from(2)), None).encode().len(),
        Subscription::MIN_ENCODED_SIZE + 32
    );
    assert_eq!(
        make_subscription(Some(pallas::Base::from(2)), Some(pallas::Base::from(3))).encode().len(),
        Subscription::MIN_ENCODED_SIZE + 64
    );

    // And it still round-trips, all four shapes.
    for (bulla, note) in [
        (None, None),
        (Some(pallas::Base::from(2)), None),
        (None, Some(pallas::Base::from(3))),
        (Some(pallas::Base::from(2)), Some(pallas::Base::from(3))),
    ] {
        let record = make_subscription(bulla, note);
        let decoded = Subscription::decode(&record.encode()).expect("a record reads back");
        assert_eq!(decoded.encode(), record.encode());
    }
}

/// The malformed presence flag, on the record side.
#[test]
fn subscription_decode_refuses_a_bulla_that_is_not_there() {
    let mut bytes = make_subscription(None, None).encode();
    bytes[Subscription::FIXED_PREFIX] = 1;

    let err = Subscription::decode(&bytes).expect_err("a bulla that is not there is refused");
    let msg = format!("{err}");
    assert!(msg.contains("expected 296 bytes, got 264"), "unexpected error: {msg}");
}

/// A record that is not a record: shorter than the fixed prefix, so even the presence byte is
/// missing. Both guards must answer with a `ContractError`.
#[test]
fn plan_and_subscription_decode_refuse_a_truncated_buffer() {
    assert!(Plan::decode(&[0u8; 8]).is_err());
    assert!(Subscription::decode(&[0u8; 8]).is_err());
}
