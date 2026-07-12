# Vendored halo2 (security-pinned)

Vendored copy of the halo2 proving-system crates used by the DarkWow ZK stack:
`halo2_proofs 0.3.2`, `halo2_gadgets 0.5.0`, `halo2_poseidon 0.1.0`.

**Pinned source revision:** `98d449b854010ca8e3d6cdbaa9b87376c3ed2ef5`

## Why this is vendored

`halo2_gadgets 0.5.0` is a **mandatory security upgrade** that fixes an exploit
discovered in the ZCash Orchard circuit. Earlier releases (0.4.x and below)
predate the fix and **must not be used**.

Vendoring pins the exploit-fixed code into the repository, making the build
deterministic and independent of any external git host: the fixed revision
cannot be lost, force-pushed, or silently regressed to a vulnerable version.

Consumed via `[patch.crates-io]` `path` entries in the workspace-root and in
`bin/app`'s `Cargo.toml`. These crates are **not** workspace members — they are
kept out via `[workspace] exclude` and used only as patch sources.

## Updating

A future upstream security advisory requires a **manual re-vendor**: re-copy the
crate sources at the corresponding fixed revision and update the pinned revision
recorded above. Never regress below the ZCash-Orchard fix.

License: MIT OR Apache-2.0 (see `LICENSE-APACHE`, `LICENSE-MIT`, `COPYING.md`).
