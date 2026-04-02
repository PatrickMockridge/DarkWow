# DarkFi Core Boundary

This repository is intentionally **outside** the DarkFi tree.

That does not mean it is completely independent of DarkFi core.

## What Can Live Outside DarkFi

- reusable `.zk` templates
- Rust string exports for those templates
- arithmetic semantics docs
- AMM and solver-facing witness conventions
- vector fixtures and cross-repo migration notes

These are library artifacts, not VM artifacts.

## What Still Lives In DarkFi Core

This repository now has two distinct `zk` tracks:

- a stock-compatible AMM arithmetic kernel that uses only `range_check(64, ...)`
- experimental widened templates that depend on wider audited range profiles

Only the experimental widened track still depends on DarkFi-side support for
audited range profiles beyond the historical `64` and `253` widths:

- `126`
- `128`
- `252`

Those profiles are part of the `zkas` analyzer / VM / native range-check surface,
not part of this external package.

So the split is:

- `darkfi-safemath-zk`
  - relation catalog
  - docs
  - packaging
- DarkFi core
  - `zkas` syntax and compiler
  - VM opcode execution
  - native range-check implementation
  - proof system integration

## Consequence

This package can now be consumed externally in two ways:

- stock-compatible host helpers plus a narrow stock-compatible AMM arithmetic
  kernel on official DarkFi today
- experimental widened templates only against a DarkFi build that already
  supports the wider range profiles

The stock path is intentionally not a generic bigint layer. It is aimed at:

- `u64` state
- `u128` intermediates represented as two `u64` limbs
- comparisons where both sides are directly range-checked or packed from
  already range-checked limbs

So the immediate downstream path no longer requires a DarkFi core delta, but
the experimental widened track still does.
