# Fork Exit Roadmap

This note defines the first realistic path out of the DarkWow fork era.

## Current Facts

As of March 27, 2026:

- `amm-darkfi` depends on fork-era AMM interface modules and payload helpers
- `amm-darkfi-intent-settlement-poc` depends on the same surface plus intent-set types
- `solver-adapter` depends on those AMM/intent types and on DarkWow proof artifacts
- the direct AMM PoCs still compute core arithmetic in plain Rust helpers like
  `floor_div_u128` and `floor_sqrt_u128`

So there are two separate problems:

1. reusable arithmetic semantics were living inside the DarkWow fork line
2. AMM / intent interface types were also living in that fork line

This repository addresses problem 1 first.

## What This Extraction Solves

- safemath relations now have a home outside the DarkWow tree
- future consumers can depend on one shared arithmetic catalog
- the remaining DarkWow-core delta becomes smaller and easier to reason about
- the direct AMM type surface now also has an external home in
  `darkfi-amm-types`

## What This Extraction Does Not Solve Yet

- the experimental widened templates still need DarkWow-side support for
  `range_check(126|128|252)`
- it does not remove the remaining dependency on fork-era intent / intent-set SDK modules
- it does not automatically convert the AMM PoCs from plain Rust arithmetic helpers to
  `zkas` relation consumption

## Migration Sequence

1. Keep `darkfi-safemath-zk` external.
   Treat this repo as the canonical arithmetic layer.

2. Make the stock-compatible path real first.
   Use the host helpers and the narrow stock-compatible AMM kernel as the
   immediate downstream path on official DarkWow.
   That stock v0 surface should stay honest:
   - `u64` state
   - `u128` intermediates as two `u64` limbs
   - public floor-div / sqrt / cross-multiply relations
   - helper-only branchy templates such as `min`

3. Keep the widened template family, but treat it as experimental.
   That path remains useful for future DarkWow cores with widened range-profile support.

4. Move the direct AMM type surface out of the fork line first.
   `darkfi-amm-types` is the external replacement for the direct pool model,
   interface metadata, and payload helpers.

5. Decide where the remaining intent-facing type surface belongs.
   There are still two coherent options:
   - upstream intent / intent-set SDK modules into DarkWow
   - move them into a second external exchange-types package

6. Port `amm-darkfi` first.
   This is the cleanest direct path because it does not require intent settlement to be
   valid.

7. Port `amm-darkfi-intent-settlement-poc`.
   This adds intent-settlement behavior on top of the direct AMM port.

8. Port `solver-adapter`.
   This should follow the direct AMM and intent-settlement ports, not lead them.

## Immediate Next Step

The next high-signal task is a direct-path retarget inside `amm-darkfi`:

- switch direct AMM imports from fork `darkfi_sdk::crypto::*` to `darkfi_amm_types::*`
- keep arithmetic semantics pinned to the stock-compatible `darkfi-safemath-zk`
  host helpers and limb templates
- defer the intent / CoW path until the remaining exchange-facing types are extracted

See `docs/amm_darkfi_port_matrix.md`.
