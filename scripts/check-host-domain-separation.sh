#!/bin/bash
# OBL-C196: host-side `poseidon_hash` derivations separate their calls by domain.
#
# `check-circuit-domain-separation.sh` walks `.zk` circuits. `check-circuit-dead-values.sh` walks
# the same. Neither can see a hash computed in Rust, and `contract-wasm-type-system.md` §8.1
# requires domain separation on *every* semantically distinct invocation — circuit or host. This is
# the sibling for the host half: it scans the contract Rust that derives ids and keys and flags a
# `poseidon_hash(...)` whose argument list carries no `DOMAIN` reference.
#
# It is REPORT-ONLY, deliberately, and for the reason `check-pubkey-binding.sh` was report-only for
# a day: a gate that fails on unadjudicated findings is a gate whose authority cannot be defended.
# The first run names the corpus; adjudicating it (per-site, in `script/host_domain_exceptions.txt`)
# is what turns it into a gate. The rule is PRESENCE and syntactic — matching the circuit gate's
# rule 1 — so a call reaching its domain through a named constant or a `// DOMAIN_x` comment passes,
# and a bare literal hash does not.
#
# Exit 0: report printed (advisory), or every finding adjudicated.
# Exit 1: --self-test only — a planted bare derivation was NOT reported (the checker is broken).
# Exit 2: the reviewed list is malformed — a broken list must not read as a pass.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

run_scan() {
  REPO_ROOT="$1" python3 - <<'PYEOF'
import glob, os, re, sys

repo = os.environ["REPO_ROOT"]

# Scope: the contract host code that mints ids and keys. `entrypoint*.rs` and `entrypoint/*.rs` for
# the exec arms, `lib.rs` (game_room keeps its metadata there), the model's derivation helpers.
paths = []
for pat in ("src/contract/*/src/entrypoint.rs",
            "src/contract/*/src/entrypoint/*.rs",
            "src/contract/*/src/lib.rs",
            "src/contract/*/src/model/mod.rs"):
    paths += glob.glob(os.path.join(repo, pat))
paths = sorted(set(paths))

findings = []
for path in paths:
    rel = os.path.relpath(path, repo)
    src = open(path, errors="replace").read()
    for m in re.finditer(r'poseidon_hash\s*\(', src):
        i = m.end() - 1  # opening '('
        depth, j, end = 0, i, None
        while j < len(src):
            if src[j] == "(":
                depth += 1
            elif src[j] == ")":
                depth -= 1
                if depth == 0:
                    end = j
                    break
            j += 1
        if end is None:
            continue
        # The domain is the FIRST element. Read it — up to the first top-level comma — and accept a
        # named constant (contains DOMAIN), or the inline-literal idiom this tree uses where the
        # domain is a bare number: `Base::from(3)`, `from_raw([11, …])`, or a naked integer.
        inner = src[i + 1:end].lstrip()
        if inner.startswith("["):
            inner = inner[1:]
        d, comma = 0, len(inner)
        for k, ch in enumerate(inner):
            if ch in "([{":
                d += 1
            elif ch in ")]}":
                d -= 1
            elif ch == "," and d == 0:
                comma = k
                break
        first = inner[:comma]
        if re.search(r'DOMAIN', first) or re.search(r'Base::from\s*\(|from_raw\s*[\(\[]', first) \
                or re.match(r'\s*\d', first):
            continue
        lineno = src[:m.start()].count("\n") + 1
        findings.append((f"{rel}:{lineno}",
                         f"{rel}:{lineno}: poseidon_hash(...) with no DOMAIN constant"))

exceptions = {}
exc_path = os.path.join(repo, "script", "host_domain_exceptions.txt")
if os.path.isfile(exc_path):
    for raw in open(exc_path, errors="replace").read().splitlines():
        entry = raw.split("#")[0].strip()
        if not entry:
            continue
        parts = [p.strip() for p in re.split(r'\s+:\s+', entry, maxsplit=1)]
        if len(parts) != 2:
            print(f"ERROR: malformed exception line in {os.path.basename(exc_path)}: {entry!r}")
            print("       expected: <rel path>:<line> : <reason citing a register ID>")
            sys.exit(2)
        exceptions[parts[0]] = parts[1]

failing = [(key, msg) for key, msg in findings if key not in exceptions]

# **This is the check this file's own header promised and did not have.** It said "a stale entry is
# reported by the checker so the list cannot outlive its sites" — while the checker reported
# `stale-mentions=0`, nothing compared the entries to the findings, and the list is keyed on a Rust
# *line*, which is precisely the key that drifts when an edit lands above a site. With the list empty
# the gap was invisible; the moment entries land, every one of them becomes a claim nobody re-reads.
# So the claim is now true: an entry whose `path:line` no longer yields a finding is reported as stale,
# and the report says whether the derivation moved, was repaired, or the file was renamed.
used = set()
for key, _msg in findings:
    if key in exceptions:
        used.add(key)
stale = sorted(set(exceptions) - used)

if not findings and not stale:
    print("OK: every contract host poseidon_hash carries a DOMAIN reference")
    sys.exit(0)

print(f"REPORT (advisory): {len(findings)} host poseidon_hash call(s) with no DOMAIN reference; "
      f"{len(failing)} unadjudicated; {len(stale)} stale")
for _key, msg in failing:
    print(f"  {msg}")
if stale:
    print("STALE: declared entries matching no finding — the site moved, was repaired, or was renamed:")
    for key in stale:
        print(f"  {key}")
        print(f"          {exceptions[key]}")
    print("  Fix: correct the entry's path:line, or retire it.")
print("Adjudicate each into script/host_domain_exceptions.txt, or give the derivation its domain.")
sys.exit(0)
PYEOF
}

if [[ "${1:-}" == "--self-test" ]]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/src/contract/good_probe/src" "$tmp/src/contract/bad_probe/src"
  cat > "$tmp/src/contract/good_probe/src/entrypoint.rs" <<'RS'
fn good() -> pallas::Base {
    poseidon_hash([model::SLASH_ATTESTATION_ID_DOMAIN, rx, ry, amount])
}
RS
  cat > "$tmp/src/contract/bad_probe/src/entrypoint.rs" <<'RS'
fn bad() -> pallas::Base {
    poseidon_hash([rx, ry, amount, nonce, extra])
}
RS
  set +e
  out="$(run_scan "$tmp")"
  set -e
  if ! grep -q "bad_probe" <<<"$out" || grep -q "good_probe" <<<"$out"; then
    echo "FAIL: --self-test — the planted bare derivation was not reported exactly once"
    echo "$out"
    exit 1
  fi
  # The stale-entry control: an entry naming a line in a file that exists, which carries no finding.
  # The list's own header promised this check ("a stale entry is reported by the checker") while the
  # checker had none — with an empty list the gap was invisible, and this is what makes the claim true
  # before the list starts filling.
  mkdir -p "$tmp/script"
  printf 'src/contract/bad_probe/src/entrypoint.rs:999 : OBL-C196 — planted stale entry\n' \
      > "$tmp/script/host_domain_exceptions.txt"
  set +e
  stale_out="$(run_scan "$tmp")"
  set -e
  if ! grep -q "STALE" <<<"$stale_out" || ! grep -q "bad_probe/src/entrypoint.rs:999" <<<"$stale_out"; then
    echo "FAIL: --self-test — a declared entry matching no finding was not reported as stale"
    echo "$stale_out"
    exit 1
  fi
  echo "PASS: --self-test — the planted bare derivation is reported, the domained one is not, and a"
  echo "                   declared entry matching no finding is reported as stale"
  exit 0
fi

run_scan "$(cd "$SCRIPT_DIR/.." && pwd)"
