//! The reward schedule against the closed form the document states, not against itself (`OBL-C61`).
//!
//! Every other check of `expected_reward` compares it with a transcription of its own algorithm:
//! `src/sdk/src/blockchain.rs`'s `reward_formula_key_points` asserts three hand-computed points,
//! `merge_mining_model.py` mirrors the same fixed-point recurrence, and `Emission.lean` transcribes
//! the constants. So the schedule is checked twice against descriptions of itself and never against
//! the binary — which is what `OBL-C61` records. This computes
//!
//!     R(h) = max(R₀ · 2^-(h-1)/H, R_tail)        (`consensus-coinbase.md` §3.2)
//!
//! in `f64`, sharing no code with the implementation, and compares.
//!
//! **Why this lives here and not in the sdk's own test module.** `src/sdk/**/*.rs` is in **every**
//! contract's `SOURCE_MANIFEST`, so adding a `#[cfg(test)]` module there moves the genesis pin —
//! measured, not assumed: purse's artifact was rebuilt from a pristine sdk and from one carrying
//! this test, giving `28819142…` and `370aa49b…` respectively, two different genesis inputs. The
//! test is pin-neutral here (`bin/dwowd/**` is in no contract's manifest), and it reaches the same
//! public items: `expected_reward`, `reward::*` and `BlockHeight` are all `pub`.
//!
//! **What the comparison found, measured rather than assumed.** The two do not agree exactly in the
//! exponential region, and the reason is expressible: `DECAY_FP` is `floor(2^(-1/H) · 2^32)`, so the
//! factor it represents is low by about 2.2e-10 relative, and `fixed_pow_decay` raises it to the
//! `exp`-th power — the error accumulates *linearly in the height* and the implementation runs
//! **low**: 2.2e-10 at one block, 2.2e-5 at 100,000, 2.3e-4 at the half-life (0.023%, i.e. ~160,000
//! base units of the ~691,882,480 the closed form gives), 9.5e-4 at the tail transition. The
//! direction is conservative — the tail is reached slightly later and less is issued than the
//! formula alone would allow — and the drift is what a 32-bit fixed-point constant buys. The bound
//! below is that measurement with headroom, `1e-9 + 4e-10·exp`, still tight enough to catch a wrong
//! constant: a `HALF_LIFE_BLOCKS` off by one part in 10³ moves the reward at the half-life by
//! 6.9e-4, four times the bound there.

use dwow_sdk::blockchain::{expected_reward, reward, BlockHeight};

#[test]
fn reward_matches_the_closed_form_schedule() {
    let r0 = reward::INITIAL_REWARD.get() as f64;
    let half_life = reward::HALF_LIFE_BLOCKS as f64;
    let tail = reward::TAIL_REWARD.get() as f64;

    // The two constants against their own documented derivations first, so a constant and the
    // closed form cannot move together silently — the comparison below would follow both. The
    // module states both: R₀ = ⌊total_supply · ln 2 / half_life⌋ and
    // R_tail = ⌊21,000,000 · 1% · 10⁸ / 262,980⌋.
    let derived_r0 = (2_100_000_000_000_000f64 * std::f64::consts::LN_2 / half_life).floor() as u64;
    assert_eq!(
        reward::INITIAL_REWARD.get(),
        derived_r0,
        "INITIAL_REWARD must be the derivation the module documents: \
         2,100,000,000,000,000 · ln 2 / HALF_LIFE_BLOCKS"
    );
    assert_eq!(
        reward::TAIL_REWARD.get(),
        21_000_000u64 * 1_000_000 / 262_980,
        "TAIL_REWARD must be the derivation the module documents: 21,000,000 · 1% · 10⁸ / 262,980"
    );

    // Genesis is exact; the exponential region is compared within the fixed-point bound; and the
    // heights past the transition must equal the tail exactly, which is the other branch of the
    // `max` and the one a wrong constant would move.
    assert_eq!(expected_reward(BlockHeight::GENESIS), reward::INITIAL_REWARD);
    assert_eq!(expected_reward(BlockHeight::new(4_400_000)), reward::TAIL_REWARD);
    assert_eq!(expected_reward(BlockHeight::new(100_000_000)), reward::TAIL_REWARD);

    let heights = [
        2u64,
        3,
        1_000,
        10_000,
        100_000,
        reward::HALF_LIFE_BLOCKS,
        reward::HALF_LIFE_BLOCKS + 1,
        reward::HALF_LIFE_BLOCKS * 2,
        2_000_000,
        4_000_000,
        4_300_000,
    ];

    for h in heights {
        let exp = (h - 1) as f64;
        let spec = (r0 * 2f64.powf(-exp / half_life)).max(tail);
        let got = expected_reward(BlockHeight::new(h)).get() as f64;
        let relative = (got - spec).abs() / spec.max(1.0);
        let bound = 1e-9 + 4e-10 * exp;
        assert!(
            relative <= bound,
            "h={h}: the implementation gives {got}, the closed form gives {spec} \
             (relative {relative:e}, bound {bound:e}) — beyond what the fixed-point constant can \
             account for, so a constant or the exponent has moved"
        );
    }
}
