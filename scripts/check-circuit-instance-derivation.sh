#!/bin/bash
# Gate: the Orchard-class rule over every `.zk` source (OBL-Z1).
#
# Every `constrain_instance(X)` must be derived in-circuit, bound by a preceding
# `constrain_equal_*`, redundant with respect to another exposed determination, or named in
# `script/circuit_free_instances.txt` with a host-side justification. Anything else is a public
# input the prover sets freely, which is the Orchard-class shape.
#
# WHY THIS IS A GATE AND NOT A SCRIPT IN THE OTHER THREE. `check-circuit-metadata-alignment.sh`
# compares *counts* of `constrain_instance` against metadata pushes; `check-circuit-domain-
# separation.sh` checks that a `poseidon_hash` carries *some* domain prefix; `hooks/pre-commit`
# pattern-matches one binding shape on staged files. None of them asks whether the value exposed
# is determined, so before this gate nothing in the repository did.
#
# WHAT A PASS MEANS. That the checker's model of the derivation held for every instance of every
# circuit it walked. It is a structural check over source text, not a soundness proof — see the
# module docstring of `script/circuit_instance_derivation.py` for the assumptions that carry the
# step from "determined in-circuit" to "means what the contract intends".
#
# Exit 0: every instance classified.  Exit 1: at least one is not — the checker names it.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

exec python3 "$REPO_ROOT/script/circuit_instance_derivation.py"
