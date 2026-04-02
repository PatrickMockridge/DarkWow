# Proof Harness

This crate holds the optional DarkFi-backed proof harness for
`darkfi-safemath-zk`.

It is intentionally separate from the root crate so the published library stays
standalone and does not assume any local checkout layout.

## Running

Default stock-compatible proof run:

```sh
cargo test --manifest-path proof-harness/Cargo.toml
```

That command proves the stock-compatible AMM-kernel track against the pinned
official DarkFi revision and keeps the default path green on stock upstream
DarkFi.

It runs:

- public stock v0 templates directly
- helper-only stock templates where a full AMM vector needs them

Opt-in experimental widened proof run:

```sh
cargo test --manifest-path proof-harness/Cargo.toml -- --ignored
```

The harness pins the official DarkFi repository on Codeberg to:

- `bbcfa2f33bba31c92e72a70ba3992ef1147723d2`

## Current Status

At that upstream revision, stock official DarkFi supports the stock-compatible
track in this repo, which uses only:

- `range_check(64, ...)`
- existing field comparison / equality gadgets

That means:

- `cargo test --manifest-path proof-harness/Cargo.toml` should pass and prove
  the stock-compatible AMM-kernel templates
- `cargo test --manifest-path proof-harness/Cargo.toml -- --ignored` still
  requires DarkFi support for the widened experimental safemath profiles

Those widened profiles are:

- `126`
- `128`
- `252`

This harness is intentionally not a generic bigint testbed. The stock path is
aimed at:

- `u64` state
- `u128` intermediates represented as `(lo, hi)` limbs
- floor division, floor square root, and cross-multiply threshold relations

Branchy helpers such as `min_select_u128_2x64_v1` remain available and are
still exercised when needed by full AMM vectors, but they are not the flagship
public stock v0 surface.
