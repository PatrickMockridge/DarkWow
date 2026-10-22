#!/usr/bin/env bash
#
# The nine genesis o-cap primitives stay primitives: no consumer code, no foreign ids, no child
# calls, and every parent-facing helper registered with a generic justification.
#
# WHY THIS EXISTS. The genesis contracts are o-cap primitives, not applications. The doctrine is
# `doc/src/arch/genesis-purity.md`: class A generic mechanics, class B generic helpers and one-way
# tags, class C consumer-specific debt that must be declared and expire. The one surface that had
# become consumer-driven was `box::Take`'s `contents_commit`, on the wire because `escrow::ClaimV1`
# reads it; the sweep retired it for a generic one-way tag.
#
# This is NOT `scripts/check-genesis-purity.sh`. That is the G1/G3 ambient-authority check over the
# five genesis-building function bodies; this checks the primitive surface of the nine crates.
#
# WHAT IT CHECKS.
#   1. Set closure: the nine crates exist and their package names match. Seven carry a
#      manifest; deployooor is plaintext infrastructure with none, and class_a = [].
#   2. No foreign code or manifest dependency: no `dwow_*_contract` dependency other than the
#      crate itself, no `[contract].dependencies`, no foreign `dwow_*_contract` token, and no
#      foreign `*_CONTRACT_ID`. `ContractId::from_bytes` is a decode, not a naming, and is not
#      flagged: deployooor and promissory_note use it for ids that arrive in their own records.
#   3. Zero cross-contract child calls: no `children_indexes` in a genesis entrypoint or lib.rs.
#   4. Every `pub fn` in a `validation*.rs` file is a registered B helper.
#   5. The L1 wire ledger and the register's live `class_c` declarations are the same set.
#   6. Register/source consistency: the nine rows match the manifests' sections, every live B
#      row resolves to a symbol, and every live C row is an L1 declaration or an accepted debt
#      with an expiry.
#
# WHAT IT DOES NOT ESTABLISH (R7). It is structural: a macro, alias, trait object, code generator
# or raw-index dispatch can hide a child call or a consumer id. It cannot decide whether a
# generic-shaped helper encodes a policy for one caller; that is the register's human decision.
# It also cannot prove the tag's privacy — a one-way pseudonym is not a secrecy proof.
#
# Usage: scripts/check-genesis-primitive-purity.sh
#        scripts/check-genesis-primitive-purity.sh --self-test   (planted defects; non-zero exit)
# Exit status: 0 when clean; 1 on any finding or a self-test control that does not fire.

set -uo pipefail

ROOT="${GENESIS_PRIMITIVE_PURITY_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
export GENESIS_PRIMITIVE_PURITY_ROOT="$ROOT"

python3 - "$@" <<'PY'
import os, re, shutil, subprocess, sys, tempfile, tomllib
import pathlib

ROOT = pathlib.Path(os.environ["GENESIS_PRIMITIVE_PURITY_ROOT"])

# Canonical list: doc/src/arch/genesis.md.
GENESIS = [
    ("deployooor", "dwow_deployooor_contract", 2, "consensus"),
    ("promissory_note", "dwow_promissory_note_contract", 3, "ecosystem"),
    ("native_token", "dwow_native_token_contract", 4, "consensus"),
    ("identity", "dwow_identity_contract", 5, "ecosystem"),
    ("oracle", "dwow_oracle_contract", 6, "ecosystem"),
    ("attestation", "dwow_attestation_contract", 7, "ecosystem"),
    ("purse", "dwow_purse_contract", 8, "ecosystem"),
    ("box", "dwow_box_contract", 9, "ecosystem"),
    ("multisig", "dwow_multisig_contract", 10, "ecosystem"),
]
GEN_NAMES = {g[0] for g in GENESIS}
GEN_CRATES = {g[1] for g in GENESIS}
CRATE_OF = {g[0]: g[1] for g in GENESIS}
COUNTER_OF = {g[0]: g[2] for g in GENESIS}

REGISTER = ROOT / "doc" / "src" / "arch" / "genesis-purity.md"
WIRE_GATE = ROOT / "scripts" / "check-l1-wire-conformance.sh"


def strip_rust(text):
    """Remove comments and string/char literals so a token scan reads code only."""
    out, i, n = [], 0, len(text)
    state = "code"
    depth = 0
    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if state == "code":
            if c == "/" and nxt == "/":
                state = "line"; i += 2; continue
            if c == "/" and nxt == "*":
                state = "block"; depth = 1; i += 2; continue
            if c == '"':
                state = "str"; out.append(" "); i += 1; continue
            if c == "'":
                # Char literal or lifetime: treat 'x' / '\n' as a literal, lifetimes as code.
                if i + 2 < n and (text[i + 2] == "'" or (c == "\\" and i + 3 < n)):
                    state = "char"; i += 1; continue
                out.append(c); i += 1; continue
            out.append(c); i += 1
        elif state == "line":
            if c == "\n":
                state = "code"; out.append("\n")
            i += 1
        elif state == "block":
            if c == "/" and nxt == "*":
                depth += 1; i += 2; continue
            if c == "*" and nxt == "/":
                depth -= 1; i += 2
                if depth == 0:
                    state = "code"
                continue
            if c == "\n":
                out.append("\n")
            i += 1
        elif state == "str":
            if c == "\\":
                i += 2; continue
            if c == '"':
                state = "code"
            i += 1
        elif state == "char":
            if c == "\\":
                i += 2; continue
            if c == "'":
                state = "code"
            i += 1
    return "".join(out)


def strip_test_modules(text):
    """Drop `#[cfg(test)] mod tests { ... }` via brace matching (best effort, fail-open)."""
    out = text
    while True:
        m = re.search(r"#\[cfg\(test\)\]\s*(?:pub\s+)?mod\s+\w+\s*\{", out)
        if not m:
            return out
        start = out.find("{", m.start())
        depth, i = 0, start
        while i < len(out):
            if out[i] == "{":
                depth += 1
            elif out[i] == "}":
                depth -= 1
                if depth == 0:
                    i += 1
                    break
            i += 1
        out = out[:m.start()] + out[i:]


def read_code(path):
    return strip_rust(strip_test_modules(path.read_text()))


def register_ledger(root):
    text = (root / "doc" / "src" / "arch" / "genesis-purity.md").read_text()
    m = re.search(
        r"<!-- genesis-purity-register:begin -->\s*```toml\n(.*?)\n```\s*<!-- genesis-purity-register:end -->",
        text, re.S,
    )
    if not m:
        raise RuntimeError("the genesis-purity register has no parseable ledger block")
    return tomllib.loads(m.group(1))


def wire_declared(root):
    """The DECLARED keys in scripts/check-l1-wire-conformance.sh, normalised to a/b/c/d."""
    src = (root / "scripts" / "check-l1-wire-conformance.sh").read_text()
    block = re.search(r"DECLARED\s*=\s*\{(.*?)\}", src, re.S)
    if not block:
        raise RuntimeError("could not parse DECLARED from check-l1-wire-conformance.sh")
    keys = re.findall(r'\(\s*"([\w]+)",\s*"([\w]+)",\s*(\d+),\s*"([\w]+)"\s*\)', block.group(1))
    return {f"{a}/{b}/{c}/{d}" for a, b, c, d in keys}


def run_checks(root):
    errors = []
    notes = []

    # 1. Set closure.
    for name, crate, counter, tier in GENESIS:
        d = root / "src" / "contract" / name
        cargo, man = d / "Cargo.toml", d / "manifest.toml"
        # Deployooor is plaintext infrastructure, not a capability contract: it
        # has no manifest. Every other genesis primitive still does.
        if not cargo.exists() or (name != "deployooor" and not man.exists()) \
                or not (d / "src" / "lib.rs").exists():
            errors.append(f"{name}: missing Cargo.toml, manifest.toml or src/lib.rs")
            continue
        try:
            pkg = tomllib.loads(cargo.read_text()).get("package", {}).get("name")
        except Exception as e:
            errors.append(f"{name}: Cargo.toml does not parse: {e}")
            continue
        if pkg != crate:
            errors.append(f"{name}: Cargo package is {pkg!r}, expected {crate!r}")

    # 2. Foreign dependencies and ids.
    for name, crate, _counter, _tier in GENESIS:
        d = root / "src" / "contract" / name
        if not d.exists():
            continue
        try:
            cargo = tomllib.loads((d / "Cargo.toml").read_text())
        except Exception:
            cargo = {}
        for section in ("dependencies", "build-dependencies"):
            for dep in cargo.get(section, {}):
                if dep.startswith("dwow_") and dep.endswith("_contract") and dep != crate:
                    errors.append(f"{name}: {section} depends on foreign contract crate {dep}")
        try:
            man = tomllib.loads((d / "manifest.toml").read_text())
        except Exception:
            man = {}
        for dep in man.get("contract", {}).get("dependencies", []) or []:
            errors.append(f"{name}: manifest [contract].dependencies names {dep!r}; a genesis primitive depends on no other contract")

        for p in sorted((d / "src").rglob("*.rs")):
            code = read_code(p)
            rel = p.relative_to(root)
            for m in re.finditer(r"\bdwow_([a-z0-9_]+)_contract\b", code):
                if m.group(0) != crate:
                    errors.append(f"{rel}: references foreign contract crate {m.group(0)}")
            for m in re.finditer(r"\b([A-Z][A-Z0-9_]*)_CONTRACT_ID\b", code):
                if m.group(1).lower() != name:
                    errors.append(f"{rel}: references foreign contract id {m.group(0)}")

    # 3. Zero cross-contract child calls in entrypoints / lib.rs.
    for name, _crate, _counter, _tier in GENESIS:
        d = root / "src" / "contract" / name
        if not d.exists():
            continue
        for p in sorted((d / "src").rglob("*.rs")):
            parts = p.parts
            if "client" in parts:
                continue
            if "entrypoint" not in parts and p.name != "lib.rs":
                continue
            code = read_code(p)
            if "children_indexes" in code:
                errors.append(f"{p.relative_to(root)}: a genesis entrypoint reads children_indexes; genesis has zero cross-contract child calls")

    # 4. Every validation helper is registered as a live/planned B surface.
    led = register_ledger(root)
    surfaces = led.get("surface", [])
    b_helpers = {(s.get("contract"), s.get("symbol"))
                 for s in surfaces if s.get("class") == "B" and s.get("kind") == "helper"
                 and s.get("status", "live") in ("live", "planned")}
    for name, _crate, _counter, _tier in GENESIS:
        d = root / "src" / "contract" / name
        if not d.exists():
            continue
        files = list(d.glob("src/validation*.rs")) + list((d / "src").glob("validation/**/*.rs"))
        for p in files:
            code = read_code(p)
            for fn in re.findall(r"pub fn\s+(\w+)", code):
                if (name, fn) not in b_helpers:
                    errors.append(f"{p.relative_to(root)}: pub fn {fn} is a parent-facing helper with no B row in the purity register")

    # 5. L1 ledger equals the register's live class C declarations.
    try:
        wire = wire_declared(root)
    except RuntimeError as e:
        errors.append(str(e)); wire = None
    register_c = set()
    for row in led.get("class_c", []):
        if row.get("status", "live") != "live":
            continue
        if not row.get("expiry"):
            errors.append(f"{row.get('id')}: live class C row has no expiry")
        if row.get("l1_declared"):
            register_c.add(row.get("l1_key", ""))
        elif "accepted" not in (row.get("decision") or "").lower():
            errors.append(f"{row.get('id')}: live class C row is neither an L1 declaration nor an accepted debt")
    if wire is not None and wire != register_c:
        errors.append(f"L1 wire ledger {sorted(wire)} != register live class C {sorted(register_c)}")
    gate = root / "scripts" / "check-l1-wire-conformance.sh"
    if gate.exists():
        env = dict(os.environ, L1_WIRE_ROOT=str(root))
        r = subprocess.run(["bash", str(gate)], cwd=root, env=env, capture_output=True, text=True)
        notes.append(f"L1 wire: {r.stdout.strip().splitlines()[-1] if r.stdout.strip() else 'no output'}")
        if r.returncode != 0:
            errors.append("check-l1-wire-conformance.sh failed; the wire ledger is not current")

    # 6. Register/source consistency.
    contracts = led.get("contract", [])
    if {c.get("name") for c in contracts} != GEN_NAMES:
        errors.append("register [[contract]] set is not the canonical nine")
    for c in contracts:
        name = c.get("name")
        d = root / "src" / "contract" / name
        if not d.exists():
            continue
        if c.get("crate") != CRATE_OF.get(name):
            errors.append(f"register: {name} crate {c.get('crate')!r} != {CRATE_OF.get(name)!r}")
        if c.get("counter") != COUNTER_OF.get(name):
            errors.append(f"register: {name} counter {c.get('counter')!r} != {COUNTER_OF.get(name)!r}")
        try:
            man = tomllib.loads((d / "manifest.toml").read_text())
        except Exception:
            man = {}
        keys = {k for k in man if k != "contract"}
        declared = set(c.get("class_a", []))
        if keys != declared:
            errors.append(f"register: {name} class_a {sorted(declared)} != manifest sections {sorted(keys)}")

    for s in surfaces:
        if s.get("class") != "B":
            errors.append(f"{s.get('id')}: [[surface]] must be class B")
            continue
        if s.get("status", "live") != "live":
            continue
        f = root / s.get("file", "")
        if not f.exists():
            errors.append(f"{s.get('id')}: file {s.get('file')} does not exist")
            continue
        code = f.read_text()
        parts = [p for p in re.split(r"[.:]", s.get("symbol", "")) if p]
        if not all(p in code for p in parts):
            errors.append(f"{s.get('id')}: symbol {s.get('symbol')} not found in {s.get('file')}")
        if not s.get("verifier"):
            errors.append(f"{s.get('id')}: B surface has no verifier")

    for row in led.get("class_c", []):
        if row.get("status", "live") != "live":
            continue
        f = root / row.get("file", "")
        if not f.exists():
            errors.append(f"{row.get('id')}: file {row.get('file')} does not exist")
        expiry = row.get("expiry") or ""
        if not expiry:
            errors.append(f"{row.get('id')}: live class C row has no expiry")

    return errors, notes


def report(errors, notes):
    for n in notes:
        print(f"  {n}")
    for e in errors:
        print(f"FAIL: {e}")
    if errors:
        print(f"FAIL: {len(errors)} genesis primitive purity finding(s)")
        return 1
    print("PASS: nine genesis primitives — no foreign code/manifest dependency, no foreign id, "
          "no cross-contract child call, every validation helper registered, L1 ledger and "
          "register agree")
    return 0


def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        ig = shutil.ignore_patterns("target", "*.wasm", "*.bin")
        for name, _crate, _counter, _tier in GENESIS:
            shutil.copytree(ROOT / "src" / "contract" / name, tmp / "src" / "contract" / name, ignore=ig)
        (tmp / "doc" / "src" / "arch").mkdir(parents=True)
        shutil.copy2(REGISTER, tmp / "doc" / "src" / "arch" / "genesis-purity.md")
        (tmp / "scripts").mkdir()
        shutil.copy2(WIRE_GATE, tmp / "scripts" / "check-l1-wire-conformance.sh")

        def run(label, expect):
            errs, _ = run_checks(tmp)
            hit = [e for e in errs if expect in e]
            if not hit:
                print(f"FAIL: --self-test planted {label} and the checker did not report it "
                      f"({len(errs)} finding(s): {errs[:3]})")
                return False
            print(f"OK: --self-test — {label} is reported")
            return True

        errs, _ = run_checks(tmp)
        if errs:
            print(f"FAIL: --self-test clean twin is not clean: {errs[:5]}")
            return 1

        ok = True
        # 1. A child call in a genesis entrypoint.
        p = tmp / "src" / "contract" / "box" / "src" / "entrypoint" / "mod.rs"
        orig = p.read_text()
        p.write_text(orig + "\nfn probe_child() { let _ = self_.children_indexes; }\n")
        ok &= run("a cross-contract child call", "children_indexes")
        p.write_text(orig)

        # 2. A foreign contract crate import in a genesis entrypoint.
        p = tmp / "src" / "contract" / "purse" / "src" / "entrypoint" / "mod.rs"
        orig = p.read_text()
        p.write_text("use dwow_promissory_note_contract::validation::validate_child_value_commit;\n" + orig)
        ok &= run("a foreign contract import", "dwow_promissory_note_contract")
        p.write_text(orig)

        # 3. A foreign manifest dependency.
        p = tmp / "src" / "contract" / "oracle" / "manifest.toml"
        orig = p.read_text()
        p.write_text(orig.replace('name = "oracle"', 'name = "oracle"\ndependencies = ["promissory_note"]', 1))
        ok &= run("a foreign manifest dependency", "manifest [contract].dependencies")
        p.write_text(orig)

        # 4. An unregistered parent-facing helper.
        p = tmp / "src" / "contract" / "promissory_note" / "src" / "validation.rs"
        orig = p.read_text()
        p.write_text(orig + "\npub fn probe_helper() {}\n")
        ok &= run("an unregistered validation helper", "probe_helper")
        p.write_text(orig)

        # 5. A consumer id in primitive code.
        p.write_text(orig + "\npub fn probe_consumer() { let _ = STABLECOIN_CONTRACT_ID; }\n")
        ok &= run("a foreign contract id", "STABLECOIN_CONTRACT_ID")
        p.write_text(orig)

        # 6. The register and the L1 ledger disagreeing.
        p = tmp / "doc" / "src" / "arch" / "genesis-purity.md"
        orig = p.read_text()
        # The declaration is retired and the wire ledger is empty; make it live again with its old
        # key, so the register claims a leak the wire gate does not report.
        p.write_text(orig.replace('status = "retired"', 'status = "live"', 1))
        ok &= run("an L1 ledger mismatch", "L1 wire ledger")
        p.write_text(orig)

        return 0 if ok else 1


if "--self-test" in sys.argv:
    sys.exit(self_test())

try:
    errs, notes = run_checks(ROOT)
except Exception as e:
    print(f"FAIL: the checker could not run: {e}")
    sys.exit(1)
sys.exit(report(errs, notes))
PY
