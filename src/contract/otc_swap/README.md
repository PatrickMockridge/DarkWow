# OTC Swap

Atomic over-the-counter token swap. The canonical specification is
[`doc/src/contract/otc_swap.md`](../../../doc/src/contract/otc_swap.md); the manifest is
[`manifest.toml`](manifest.toml) and the circuits are under [`proof/`](proof/).

The swap consumes the maker's promissory note and the taker's counter-note in one transaction,
with value conservation per asset enforced by the PN circuits and the contract's metadata.
Read the contract page and the manifest for the current function codes; do not hand-copy them
here.

Tests: `tests/`; heavyweight control: `test_heavyweight_otc_swap`.
