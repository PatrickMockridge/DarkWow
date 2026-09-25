#!/usr/bin/env bash
#
# The L1 wire: a value that reaches the call params is a value every observer has.
#
# WHY THIS EXISTS. `privacy.md` states L1's privacy claim twice and specifically. §2: an observer sees
# "only a nullifier and a Merkle root — not which resource was operated on, not by whom, not how much".
# §2.4's table: Purse hides the balance "in Pedersen commitment … not which purse or how much", Box hides
# its contents "in Poseidon commitment … not which box or what it holds", and §5.5 says of `box_id` that
# it "is never a public input — an observer sees only nullifiers and Merkle roots". Part C then states the
# *transport*: §C.8.1 has the wallet identifying trajectories by "trial-decrypting AEAD notes on the new
# Merkle leaves", and §C.8.2 makes the note mandatory — "An L1 note SHALL include: `nullifier` …
# `merkle_root` … `leaf_position`" — because without them a note is trajectory-ambiguous.
#
# Measured 2026-09-26, **Box and Purse do the opposite**: their witness maps source the object identity,
# the state nonces, the contents commitments and the balances from `param:<field>`, and a `param:` value
# is serialized into the call data, which is plaintext and committed to by the transaction hash. Their
# own manifests said so in a comment ("params-based (no AEAD note): witness values travel in the call
# params") — a description of the deviation, not of a design. The values are not needed there: no
# host reads them (this gate reads the entrypoints, not the prose) and the wallet's discovery path builds
# capabilities from the decrypted note, not from params.
#
# WHAT IT CHECKS. For every L1 contract, for every circuit, every `param:<field>` witness slot must earn
# its place on the wire in one of two ways:
#
#   (a) the circuit exposes the field — `constrain_instance(<field>)` — so the value is a public input
#       anyway and §5.1's witness-only rule is what licenses the params route; or
#   (b) the entrypoint reads it — `p.<field>` / `params.<field>` — so a host check or a metadata echo
#       needs it.
#
# Neither, and the value is published for no reason a verifier needs. That is the leak this gate names.
#
# THE DECLARED LIST EXPIRES, deliberately. The 22 slots found on 2026-09-26 are declared below with the
# reason each is still there, and the gate fails if one of them *stops* appearing — the declaration is a
# debt, and removing a field from the wire is what pays it. An allowlist that never expires is the
# "instrument that cannot report its own failure" defect with a longer half-life; this one expires.
#
# WHAT IT DOES NOT CHECK. Whether the AEAD note actually carries what §C.8.2 requires (that is a separate
# reading, and today both schemas are missing `merkle_root` and `leaf_position`), whether a `param:` value
# that *is* host-read should be public, and nothing outside the three L1 contracts below — which are named
# from `privacy.md` §2's own list, not derived, because a contract's level is a specification decision.
#
# Usage: scripts/check-l1-wire-conformance.sh
#        scripts/check-l1-wire-conformance.sh --self-test   (planted defect; requires a non-zero exit)
# Exit status: 1 on an undeclared leak, or on a declared leak that has gone; 0 otherwise.

set -uo pipefail

ROOT="${L1_WIRE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export L1_WIRE_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, sys, tomllib, pathlib

ROOT = pathlib.Path(os.environ["L1_WIRE_ROOT"])

# privacy.md §2: "L1 contracts: PromissoryNote, Box, Purse."
L1_CONTRACTS = ["promissory_note", "box", "purse"]

# The measured debt, expiry = removing the field from the wire (this batch's B1-iii).
# key: (contract, circuit, witness_map slot, params field)
DECLARED = {
    ("box", "Put", 0, "box_id"): "the object identity, published; the wallet learns it from its own scan record",
    ("box", "Put", 1, "old_state_nonce"): "the consumed nonce, published; the record holds it",
    ("box", "Put", 2, "new_state_nonce"): "the successor nonce, published; derived in-circuit since 2026-09-25",
    ("box", "Put", 3, "old_contents_commit"): "what the box held, published; the note is the transport §C.8.1 names",
    ("box", "Put", 4, "new_contents_commit"): "what the box will hold, published; same",
    ("box", "Take", 0, "box_id"): "as Put, slot 0",
    ("box", "Take", 1, "contents_commit"): "as Put, slot 3",
    ("box", "Take", 2, "state_nonce"): "as Put, slot 1",
    ("purse", "Deposit", 0, "purse_id"): "the object identity, published",
    ("purse", "Deposit", 1, "old_balance"): "how much, published — §2.4's claim is that the Pedersen commitment hides this",
    ("purse", "Deposit", 3, "deposit_amount"): "how much moved, published",
    ("purse", "Deposit", 5, "new_balance"): "how much, published",
    ("purse", "Deposit", 7, "state_nonce"): "the consumed nonce, published",
    ("purse", "Withdraw", 0, "purse_id"): "as Deposit, slot 0",
    ("purse", "Withdraw", 1, "old_balance"): "as Deposit, slot 1",
    ("purse", "Withdraw", 3, "withdraw_amount"): "as Deposit, slot 3",
    ("purse", "Withdraw", 5, "new_balance"): "as Deposit, slot 5",
    ("purse", "Withdraw", 7, "state_nonce"): "as Deposit, slot 7",
    ("purse", "Balance", 0, "purse_id"): "as Deposit, slot 0",
    ("purse", "Balance", 1, "asset_id"): "the token, published",
    ("purse", "Balance", 2, "balance"): "how much, published",
    ("purse", "Balance", 4, "state_nonce"): "the nonce, published",
}

def circuit_body(text):
    """The `circuit "..." { ... }` block, with `#` comments stripped."""
    out, inside = [], False
    for raw in text.splitlines():
        line = raw.split("#", 1)[0]
        if re.match(r'\s*circuit\s+"', line):
            inside = True
        if inside:
            out.append(line)
    return "\n".join(out)

def leaks():
    found = {}
    for contract in L1_CONTRACTS:
        d = ROOT / "src" / "contract" / contract
        man = tomllib.loads((d / "manifest.toml").read_text())
        host = (d / "src" / "entrypoint" / "mod.rs").read_text()
        for circ in man.get("circuits", []):
            zk_path = d / "proof" / f"{circ['name'].lower()}.zk"
            if not zk_path.exists():
                continue
            body = circuit_body(zk_path.read_text())
            exposed = set(re.findall(r"constrain_instance\(\s*(\w+)\s*\)", body))
            for slot, src in enumerate(circ.get("witness_map", [])):
                if not src.startswith("param:"):
                    continue
                field = src.split(":", 1)[1]
                if field in exposed:
                    continue                                    # (a)
                if re.search(rf"\bp\.{field}\b|\bparams\.{field}\b", host):
                    continue                                    # (b)
                found[(contract, circ["name"], slot, field)] = True
    return found

def main():
    global ROOT
    if "--self-test" in sys.argv:
        import tempfile, shutil
        with tempfile.TemporaryDirectory() as tmp:
            ig = shutil.ignore_patterns("target", "*.wasm")
            for c in L1_CONTRACTS:
                shutil.copytree(ROOT / "src" / "contract" / c,
                                pathlib.Path(tmp) / "src" / "contract" / c, ignore=ig)
            dst = pathlib.Path(tmp) / "src" / "contract" / "box"
            m = (dst / "manifest.toml").read_text()
            # plant a leak: a new param-sourced witness-only slot on Put
            m = m.replace('    "param:box_id",', '    "param:box_id",\n    "param:probe_field",', 1)
            (dst / "manifest.toml").write_text(m)
            os.environ["L1_WIRE_ROOT"] = tmp
            ROOT = pathlib.Path(tmp)
            f = leaks()
            probe = [k for k in f if k[3] == "probe_field"]
            if not probe:
                print("FAIL: --self-test planted a leak and the checker did not see it")
                return 1
            print(f"OK: --self-test — the planted leak is reported ({probe[0][0]}/{probe[0][1]} slot {probe[0][2]})")
        return 0

    found = leaks()
    undeclared = sorted(k for k in found if k not in DECLARED)
    stale = sorted(k for k in DECLARED if k not in found)
    for contract, circuit, slot, field in undeclared:
        print(f"FAIL: {contract}/{circuit} witness slot {slot} sources `param:{field}`, which no circuit "
              f"exposes and no host reads — it is published for nothing")
    for contract, circuit, slot, field in stale:
        print(f"FAIL: the declared leak {contract}/{circuit} slot {slot} (`param:{field}`) is gone — the "
              f"declaration is stale; remove it and let the count fall")
    if undeclared or stale:
        print(f"FAIL: {len(undeclared)} undeclared and {len(stale)} stale declaration(s); "
              f"{len(DECLARED)} declared")
        return 1
    print(f"PASS: {len(found)} declared wire leak(s) in {', '.join(L1_CONTRACTS)}; none new, none stale. "
          f"Removing a field from the wire is what retires each entry.")
    return 0

sys.exit(main())
PY
