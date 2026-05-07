# darkfi-safemath-zk

External `zkas` safemath catalog for DarkWow-style bounded integer relations.

This folder is the first concrete move out of the DarkWow fork era:

- reusable arithmetic relations live outside the main DarkWow tree
- AMM and intent repos can target one shared safemath package instead of carrying
  local arithmetic semantics
- the remaining DarkWow-core delta becomes easier to isolate and upstream

## Release Shape

This crate is intended to publish cleanly as a standalone library crate.

- the published crate ships embedded `.zk` template strings and small host-side
  arithmetic helpers only
- it has no normal runtime dependency on DarkWow or Halo2
- DarkWow compiler / VM proof checks remain local development-time tests in this
  source tree and are not part of the published crate tarball

That boundary keeps `darkfi-safemath-zk` usable as the first released crate in
the de-forked AMM stack.

## What Lives Here

- reusable `.zk` templates under `templates/safemath/`
- a small Rust catalog crate that exposes those templates as string constants
- boundary docs that separate:
  - what can live outside DarkWow
  - what still requires DarkWow core support

## Current Template Tracks

Stock v0 public templates:

- `assert_u64_v1`
- `assert_nonzero_u64_v1`
- `assert_lt_u64_v1`
- `assert_lte_u64_v1`
- `div_floor_u128_by_u64_to_u64_v1`
- `sqrt_floor_u128_v1`
- `cross_mul_lte_u64_v1`
- `cross_mul_gte_u64_v1`

Stock helper templates:

- `assert_u128_2x64_v1`
- `assert_u128_lt_2x64_v1`
- `assert_u128_lte_2x64_v1`
- `min_select_u128_2x64_v1`

Experimental widened templates:

- `assert_u128_v1`
- `div_floor_v1`
- `sqrt_floor_v1`
- `min_select_v1`
- `ratio_lte_v1`

## Using The Crate

```rust
use darkfi_safemath_zk::{
    host::{floor_div_u128_by_u64_to_u64, floor_sqrt_u128_to_u64, split_u128},
    safemath::stock::{template, DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK},
};

assert_eq!(
    template("div_floor_u128_by_u64_to_u64_v1.zk"),
    Some(DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK)
);
assert_eq!(floor_div_u128_by_u64_to_u64(10, 3).unwrap(), 3);
assert_eq!(floor_sqrt_u128_to_u64(97_408_265_472), 312_102);
assert_eq!(split_u128(1_u128 << 64).hi, 1);
```

Consumers that want to compile or prove these templates should do so with their
own DarkWow-compatible `zkas` toolchain. This crate deliberately does not bundle
DarkWow integration helpers in its runtime API.

This is not yet a general bigint or full-spectrum "safemath" system. The stock
track is intentionally a bounded AMM arithmetic kernel:

- `u64` state values
- `u128` intermediates represented publicly as `(lo, hi)` limbs
- packed `Base` arithmetic only where the exact integer value is provably still
  inside a stock-safe bound
- comparison semantics only when both sides are directly range-checked or packed
  from already range-checked limbs

## Downstream Use Today

If you are integrating this crate into downstream stock-official-DarkWow code
today, prefer:

- `darkfi_safemath_zk::safemath::stock::CATALOG`
- `darkfi_safemath_zk::safemath::stock::template(...)`
- host helpers that mirror the public stock kernel directly:
  - `floor_div_u128_by_u64_to_u64`
  - `floor_sqrt_u128_to_u64`
  - `cross_mul_lte_u64`
  - `cross_mul_gte_u64`

Use `darkfi_safemath_zk::safemath::stock::helpers::*` only when you explicitly
need helper semantics such as limb assertions, packed-limb compares, or the
current branchy `min_select_u128_2x64_v1` relation.

Avoid treating these as the default downstream surface today:

- `darkfi_safemath_zk::safemath::CATALOG`
  Because it mixes stock-public templates with experimental widened templates.
- `darkfi_safemath_zk::safemath::experimental::*`
  Unless you are deliberately targeting a non-stock DarkWow with widened
  `range_check(126|128|252)` support.

## Semantic Examples

- `div_floor_u128_by_u64_to_u64_v1`
  Use this when AMM arithmetic naturally produces a `u128` numerator, the
  divisor is a bounded `u64`, and the exact floor-division result is also part
  of a bounded `u64` state transition.
- `sqrt_floor_u128_v1`
  Use this when you need `floor(sqrt(x))` for a `u128` quantity represented as
  `(lo, hi)` limbs, such as initial LP minting from a bounded product.
- `cross_mul_lte_u64_v1`
  Use this to assert a threshold relation like `lhs_num / lhs_den <= rhs_num /
  rhs_den` for range-checked `u64` quantities. It is not a generic unbounded
  ratio gadget.
- `cross_mul_gte_u64_v1`
  Use this for the dual threshold direction `lhs_num / lhs_den >= rhs_num /
  rhs_den` when the same bounded-`u64` assumptions hold.

## DarkWow Core Boundary

This package does **not** ship the DarkWow VM or `zkas` compiler.

The stock template track is designed to work on official DarkWow using only:

- `range_check(64, ...)`
- existing field comparison / equality gadgets

That stock track is deliberately narrow:

- public AMM-facing templates for division, square root, and cross-multiply
  threshold checks
- helper-only limb assertions / limb compares
- no claim of arbitrary-width integer support
- no claim that branchy helpers like `min` are part of the flagship stock API

The experimental widened track still relies on DarkWow-side support for:

- `range_check(126, ...)`
- `range_check(128, ...)`
- `range_check(252, ...)`

So the package is external, but only the experimental track still expects a
DarkWow build that understands those wider audited range profiles.

See [docs/darkfi_core_boundary.md](docs/darkfi_core_boundary.md).

## Fork-Exit Use

This package is meant to become the shared arithmetic layer for:

- `amm-darkfi`
- `amm-darkfi-intent-settlement-poc`
- `solver-adapter`

Those repos still depend on fork-era AMM and intent interfaces, and they still do
most pool arithmetic in plain Rust helpers today. `darkfi-safemath-zk` is the
first extraction step, not the full migration.

See [docs/fork_exit_roadmap.md](docs/fork_exit_roadmap.md).
See [docs/amm_darkfi_port_matrix.md](docs/amm_darkfi_port_matrix.md).

## Local Verification

This repo now has three distinct verification paths:

1. Root crate release checks

```sh
cargo test
cargo publish --dry-run --allow-dirty
```

These validate the published crate surface only: embedded template exports, host
helpers, doctests, packaging, and publishability. They do not require DarkWow.

2. Stock proof-harness check

```sh
cargo test --manifest-path proof-harness/Cargo.toml
```

This validates the stock-compatible AMM kernel against the pinned official
DarkWow revision on Codeberg. It is a real proof path, not just a build check.
The stock harness may also exercise helper-only stock templates where a full AMM
vector needs them, but those helpers are not part of the flagship public stock
v0 surface.

3. Opt-in experimental widened proof tests

```sh
cargo test --manifest-path proof-harness/Cargo.toml -- --ignored
```

These exercise the older widened template family. They remain opt-in because
stock official DarkWow still rejects the widened `range_check(126|128|252)`
profiles used by that experimental track.

The DarkWow proof harness lives in the separate `proof-harness/` crate so the
main crate stays standalone and publishable while still keeping both the stock
proof vectors and the experimental widened vectors available.
