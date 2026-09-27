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
#
# PURSE'S 8 and BOX'S 4 RETIRED (2026-09-26): `purse_id`/`state_nonce` on all three purse circuits,
# `asset_id`/`balance` on Balance, and `box_id`/`state_nonce` — their slots now read `note:` from the
# wallet's record (`CapRecord.object_id`, `.state_nonce`, `.value`, `.asset_id`, mapped in
# `bin/dww/src/lib.rs`'s `cap_record_note_fields`, and read out of the note by
# `bin/dww/src/scan.rs`, which now tries the schemas' several spellings instead of one).
# **What stays, and why each is measured rather than preferred:**
#   * the purse balances — the note's `value` is declared `u64`, `encode_params_values` refuses any
#     other type (`src/sdk/src/manifest.rs:645-666`), a `witness = N` source yields the circuit's
#     `Base`, and `NoteFieldValue::as_u64()` matches only `U64`, so a balance that leaves the params
#     cannot reach the note. **This is the only reason left**: box's four below were retired on
#     2026-09-27 and the purse's six are blocked on the note's declared type, whose design is recorded
#     in `OBL-C176`.
#
# **BOX'S FOUR RETIRED (2026-09-27), and one of these reasons was FALSE.** The declaration for
# `new_state_nonce` read: *"the successor nonce; the prover supplies it and no note field yields it"*,
# and the comment above it made the contrast explicit — *"where purse's circuit derives its own"*.
# **`put.zk` derives it too**: `:63-64` computes `computed_nsn = base_add(old_state_nonce, ONE)` and
# constrains it equal to `new_state_nonce`, so the successor is fully determined by the circuit and the
# `derived = "increment:1"` rule — the mechanism purse already uses — yields it exactly. The three
# contents commitments need no note either: they are opaque `pallas::Base` elements the circuit folds
# into a leaf, "commitment" is a naming convention and nothing in-circuit produces or verifies them, so
# tagging them `witness = N` moves them off the wire and nothing else in the tree has to change. All
# four are witness-tagged now and none is published; the note still does not carry what a box holds,
# which is §C.8.2's separate gap and not a wire requirement.
DECLARED = {
    ("purse", "Deposit", 1, "old_balance"): "how much, published — blocked on the note's `value` type, see above",
    ("purse", "Deposit", 3, "deposit_amount"): "how much moved, published — the record holds the balance, not the amount",
    ("purse", "Deposit", 5, "new_balance"): "how much, published — same block as slot 1",
    ("purse", "Withdraw", 1, "old_balance"): "as Deposit, slot 1",
    ("purse", "Withdraw", 3, "withdraw_amount"): "as Deposit, slot 3",
    ("purse", "Withdraw", 5, "new_balance"): "as Deposit, slot 5",
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
        # Which params each function declares **witness-tagged**, keyed by the circuit that proves it.
        # A witness-tagged param is skipped by `encode_params_values` and filled from the prover's bound
        # values, so it is *not* in the call data — and `param:<field>` as a witness-map source says only
        # "the caller supplies this", which is true of a witness-tagged param as much as of a published
        # one. Without this the rule reports a value as "published for nothing" when nothing publishes
        # it: measured on 2026-09-27, after box's three contents commitments were tagged, this gate
        # named all three as leaks that no longer existed.
        #
        # **The criterion is CONTINGENT, and `OBL-C179` is why.** It assumes a tag means "off the
        # wire" — which is what `box` now implements, since its decoder stopped expecting those fields.
        # `purse` contradicts it: ten of `deposit`'s sixteen params are tagged, *including every public
        # input*, while `DepositParams::decode` still expects all sixteen. So under this rule purse's
        # public inputs would be skipped as off-wire when they are read by the decoder — the rule is
        # blind to exactly the disagreement it should fail on. Settling which meaning holds is the first
        # item `OBL-C179` owes, and unit 5 is where it has to happen, because purse's six retirements
        # move the same fields.
        tagged = {}
        for fn in man.get("functions", []):
            circ = fn.get("proof_circuit")
            if not circ:
                continue
            for spec in man.get("parameters", []):
                if spec.get("function") != fn["name"]:
                    continue
                tagged[circ] = {f["name"] for f in spec.get("fields", []) if f.get("witness") is not None}
        for circ in man.get("circuits", []):
            zk_path = d / "proof" / f"{circ['name'].lower()}.zk"
            if not zk_path.exists():
                continue
            body = circuit_body(zk_path.read_text())
            exposed = set(re.findall(r"constrain_instance\(\s*(\w+)\s*\)", body))
            off_wire = tagged.get(circ["name"], set())
            for slot, src in enumerate(circ.get("witness_map", [])):
                if not src.startswith("param:"):
                    continue
                field = src.split(":", 1)[1]
                if field in off_wire:
                    continue                                    # (0) witness-tagged: not on the wire
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
            # Plant a leak: a new param-sourced witness-only slot on Put. The anchor is the
            # `witness_map` header rather than a field line, because the field lines are exactly
            # what this batch changes — the first attempt anchored on `"param:box_id"` and, once
            # that field left the wire, planted nothing and failed its own assertion instead.
            anchor = 'name = "Put"\nnamespace = "Put"\nwitness_map = [\n'
            if anchor not in m:
                print("FAIL: --self-test could not find box Put's witness_map to plant into")
                return 1
            m = m.replace(anchor, anchor + '    "param:probe_field",\n', 1)
            # The probe needs a params entry as well, or the second control below has nothing to tag —
            # a `param:` source with no field is a third case again, and not the one under test.
            anchor_p = 'function = "put"\nfields = [\n'
            if anchor_p not in m:
                print("FAIL: --self-test could not find box Put's parameters to plant into")
                return 1
            m = m.replace(anchor_p, anchor_p + '    { name = "probe_field", type = "pallas_base" },\n', 1)
            (dst / "manifest.toml").write_text(m)
            os.environ["L1_WIRE_ROOT"] = tmp
            ROOT = pathlib.Path(tmp)
            f = leaks()
            probe = [k for k in f if k[3] == "probe_field"]
            if not probe:
                print("FAIL: --self-test planted a leak and the checker did not see it")
                return 1
            print(f"OK: --self-test — the planted leak is reported ({probe[0][0]}/{probe[0][1]} slot {probe[0][2]})")

            # ── The second control, and it tests the opposite direction. ──
            # Tag the same probe field as a witness param and require the checker to stop reporting it.
            # Without this the rule could pass the first control while being unable to tell a published
            # value from a witness-borne one — which is exactly the defect this pair was written for: a
            # witness-tagged param is skipped by `encode_params_values`, so nothing publishes it, and
            # before this the rule named it as "published for nothing".
            m = (dst / "manifest.toml").read_text()
            anchor2 = '{ name = "probe_field", type = "pallas_base" },'
            if anchor2 not in m:
                print("FAIL: --self-test could not find the probe param to tag")
                return 1
            m = m.replace(anchor2, '{ name = "probe_field", type = "pallas_base", witness = 99 },', 1)
            (dst / "manifest.toml").write_text(m)
            f = leaks()
            still = [k for k in f if k[3] == "probe_field"]
            if still:
                print("FAIL: --self-test tagged the probe as a witness param and the checker still "
                      "reports it as published — a `param:` source is not the same as a wire field")
                return 1
            print("OK: --self-test — a witness-tagged param is not reported (the source kind is not "
                  "the wire kind)")
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
