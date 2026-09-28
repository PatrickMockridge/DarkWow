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
# THE DECLARED LIST EXPIRES, deliberately. Every slot this rule finds is declared below with the reason
# it is still there, and the gate fails if one of them *stops* appearing — the declaration is a debt, and
# removing a field from the wire is what pays it. An allowlist that never expires is the "instrument that
# cannot report its own failure" defect with a longer half-life; this one expires. The count is not
# written here: it was 22 on 2026-09-26, 10 on 2026-09-27 and 9 on 2026-09-28, and a number in prose is
# what goes stale first.
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
#     cannot reach the note. **This is the only reason left** for the six; the design that removes it is
#     recorded in `OBL-C176`.
#
# **BOX'S THREE ARE DECLARED AGAIN, 2026-09-28, because 2026-09-27 retired them on a false rule.**
# That day's comment said tagging "moves them off the wire and nothing else in the tree has to change."
# Nothing in the tree moved them: `encode_params_values` writes every field in the schema. The tag only
# stopped the caller's JSON from having to carry them. `new_state_nonce` was never among the three —
# its slot is `derived:increment:1`, not `param:`, so this rule never flagged it, and the day's count of
# "four" included a field that was never here. What IS true of the day's change, and worth keeping: the
# declaration's *reason* for `new_state_nonce` was false (`put.zk:63-64` derives the successor, as
# `purse` does), and `box`'s decoder stopped expecting the three — which is a manifest/decoder
# *disagreement*, the class the owed agreement rule under `OBL-C179` covers, not a retirement.
# **PURSE'S SIX ARE GONE (2026-09-28), and the block that held them was a type, not a preference.**
# `old_balance` and `new_balance` are no longer parameters at all — witness slots 1 and 5, reading
# `note:value` from the wallet's record and the circuit's own `base_add`/`base_sub`. The amount stays
# a parameter and carries `off_wire`, the tag this gate does skip because the encoders honour it.
# So purse declares nothing here, and the six entries that used to sit in this dict would now be
# reported as stale — which is what the declaration expiring is for.
# **BOX'S THREE ARE GONE TOO (2026-09-28): this list is empty, and the count is 0.** `Put`'s
# `old_contents_commit` and `Take`'s `contents_commit` are `note:user_data`, served from the wallet's
# own record; `Put`'s `new_contents_commit` carries `off_wire`. So nothing in the three L1 contracts
# this gate covers is published for a reason no verifier needs.
#
# **An empty declaration list is the state this gate was built to reach, and it is not the state
# where the gate stops being useful** — it still fails on a *new* `param:` slot, and still fails if a
# field it used to see stops appearing, which is what would say a contract started publishing one
# again. But its reach has a floor this comment should carry rather than leave implied: the rule
# reads witness-map `param:` slots, so a wire field in no witness map is invisible to it, and
# `asset_id` sat published and undeclared on purse for exactly that reason until unit 6 removed it.
DECLARED = {}

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
        # **A `witness = N` tag is NOT consulted here, and the reason is measured.** Between 2026-09-27
        # and 2026-09-28 this rule carried a skip for witness-tagged params, on the belief that a tag
        # means "off the wire". It does not: `encode_params_values` (`src/sdk/src/manifest.rs:630-680`)
        # walks the whole schema and special-cases only `param_type == "proof"`, and `contract_client.rs`
        # `:474` encodes that full schema after `:456-464` has pushed the tagged fields in from the
        # prover's bound values. Three consumers read the tag three ways — `decode_params_from_json`
        # (`manifest.rs:551-557`) skips it, the assembly loop fills it, the encoder writes it — so the
        # tag decides *who supplies* a value and never *whether it is published*. A field in
        # `[[parameters]]` is in the call data.
        #
        # The skip made this gate blind in exactly the case it exists to catch: `purse` tags ten of
        # `deposit`'s sixteen params, *including every public input*, while `DepositParams::decode`
        # expects all sixteen — so under the old rule purse's published inputs were skipped as off-wire.
        # Whether a manifest's `[[parameters]]` agrees with its contract's decoder is a *different*
        # question from this one, and it belongs to the agreement rule owed under `OBL-C179`.
        #
        # **`off_wire` IS skipped, and that skip is sound where the old one was not.** The old rule
        # keyed on `witness`, which both encoders ignore, so it hid fields that were published. This
        # keys on `off_wire`, which `encode_params_values`, `encode_params_by_schema`,
        # `leaf_field_offset` and `field_offset_by_name` all honour — so a field carrying it is
        # genuinely not in the call data and this rule has nothing to report. The two are one word
        # apart and opposite in effect, which is why the `--self-test` carries a control for each.
        off_wire = set()
        for spec in man.get("parameters", []):
            off_wire |= {f["name"] for f in spec.get("fields", []) if f.get("off_wire")}
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
                if field in off_wire:
                    continue                                    # (0) declared off_wire: not on the wire
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

            # ── The second control, and it pins the correction made on 2026-09-28. ──
            # Tag the same probe field as a witness param and require the checker to **still** report it.
            # The rule this replaces did the opposite: it skipped witness-tagged params on the belief
            # that a tag means "off the wire", which made the gate blind to `purse`'s ten tagged public
            # inputs. A control that requires the report to *stop* would re-introduce that blindness and
            # pass; this one fails if the skip ever comes back.
            m = (dst / "manifest.toml").read_text()
            anchor2 = '{ name = "probe_field", type = "pallas_base" },'
            if anchor2 not in m:
                print("FAIL: --self-test could not find the probe param to tag")
                return 1
            m = m.replace(anchor2, '{ name = "probe_field", type = "pallas_base", witness = 99 },', 1)
            (dst / "manifest.toml").write_text(m)
            f = leaks()
            still = [k for k in f if k[3] == "probe_field"]
            if not still:
                print("FAIL: --self-test tagged the probe as a witness param and the checker stopped "
                      "reporting it — a `witness = N` tag does not remove a field from the call data")
                return 1
            print("OK: --self-test — a witness-tagged param is still reported (the tag decides who "
                  "supplies a value, not whether it is published)")

            # ── The third control, and it is the other half of the same distinction. ──
            # Replace the tag with `off_wire` — one word apart, opposite in effect — and require the
            # checker to STOP reporting it. Two controls one word apart is what makes the pair mean
            # something: a rule that keys on the wrong one passes the first and fails this.
            m = (dst / "manifest.toml").read_text()
            anchor3 = '{ name = "probe_field", type = "pallas_base", witness = 99 },'
            if anchor3 not in m:
                print("FAIL: --self-test could not find the tagged probe param")
                return 1
            m = m.replace(anchor3, '{ name = "probe_field", type = "pallas_base", off_wire = true },', 1)
            (dst / "manifest.toml").write_text(m)
            f = leaks()
            still = [k for k in f if k[3] == "probe_field"]
            if still:
                print("FAIL: --self-test declared the probe off_wire and the checker still reports "
                      "it — `off_wire` is the tag the encoders honour, so nothing publishes it")
                return 1
            print("OK: --self-test — an off_wire param is not reported (that tag IS honoured by "
                  "both encoders, unlike `witness` one word away)")
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
