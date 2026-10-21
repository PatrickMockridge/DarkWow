#!/usr/bin/env python3
"""One named, expiring list of builds the tree **intends** to fail, and the feature set that
replaces a blanket `--all-features`.

## Why this exists

`src/contract/bridge/src/lib.rs` carries a deliberate `#[cfg(feature = "bridge-verify")]
compile_error!` (OBL-C21): the feature does not compile a working verifier, and enabling it by
accident must be a build failure rather than a silent policy change. Two instruments blanket-enable
every feature of every workspace member — the root `Makefile`'s `test`/`clippy`/... targets and
`contrib/clippy_unwrap_audit.sh`'s `host-allfeatures` configuration — so both were red by
construction, and the census reported the bridge as "unmeasured, not clean".

The register's answer (OBL-C21's appended note) is not to weaken the guard but to teach the
instruments that this one configuration is *intentionally unbuildable*. This script is that lesson:

* `--cargo-features` prints the exact `package/feature` list for `--all-features` minus the listed
  exclusions, derived from `cargo metadata`, so no hand-typed feature list can go stale.
* `--feature-names` prints the excluded bare feature names, for `cargo hack --exclude-features`.
* `--check` validates the list against the tree: the package and feature exist, the guard literal is
  present and gated by that feature, the entry has not expired, every cfg-gated `compile_error!`
  in the workspace is listed, and (without `--source-only`) the configuration still fails with the
  guard literal. A configuration that now compiles is **stale** and fails this check.
* `--self-test` builds a temp mini-workspace and proves each branch can fail: a removed guard is
  stale, an unlisted guarded feature is unlisted, and a past expiry is expired.

The list is the only hand-typed fact: `contrib/build_expected_failures.tsv`, one row per
(package, feature), with the obligation and an expiry date.
"""

import argparse
import datetime
import json
import os
import re
import subprocess
import sys
import tempfile

DEFAULT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIST_PATH = "contrib/build_expected_failures.tsv"

GUARD_RE = re.compile(
    r'#\[cfg\(\s*feature\s*=\s*"([^"]+)"\s*\)\]\s*compile_error!\s*\(',
    re.S,
)
GUARD_NEAR = re.compile(r'#\[cfg\(\s*feature\s*=\s*"([^"]+)"\s*\)\]', re.S)


def load_entries(root):
    path = os.path.join(root, LIST_PATH)
    entries = []
    if not os.path.isfile(path):
        return entries
    for lineno, raw in enumerate(open(path, encoding="utf-8").read().splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        parts = [p.strip() for p in line.split("\t")]
        if len(parts) != 6:
            raise SystemExit(
                f"{LIST_PATH}:{lineno}: expected 6 tab-separated columns "
                f"(package, feature, expires, obligation, guard file, guard literal), got {len(parts)}"
            )
        entries.append(
            {
                "package": parts[0],
                "feature": parts[1],
                "expires": parts[2],
                "obligation": parts[3],
                "guard_file": parts[4],
                "guard_literal": parts[5],
                "line": lineno,
            }
        )
    return entries


def cargo_metadata(root):
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(out)


def workspace_members(metadata):
    member_ids = set(metadata.get("workspace_members", []))
    return [p for p in metadata["packages"] if p["id"] in member_ids]


def excluded_pairs(entries):
    return {(e["package"], e["feature"]) for e in entries}


def cmd_cargo_features(entries, metadata):
    excluded = excluded_pairs(entries)
    feats = []
    for pkg in sorted(workspace_members(metadata), key=lambda p: p["name"]):
        for feature in sorted(pkg.get("features", {})):
            if (pkg["name"], feature) in excluded:
                continue
            feats.append(f"{pkg['name']}/{feature}")
    print(",".join(feats))


def cmd_feature_names(entries, _metadata):
    print(",".join(sorted({e["feature"] for e in entries})))


def source_check(root, entries, metadata=None):
    """Return a list of problems. No cargo builds; cheap enough for every census run."""
    problems = []
    today = datetime.date.today()
    packages = {p["name"]: p for p in workspace_members(metadata)} if metadata else {}
    listed = set()

    for e in entries:
        listed.add((e["package"], e["feature"]))
        if metadata is not None:
            pkg = packages.get(e["package"])
            if pkg is None:
                problems.append(
                    f"{LIST_PATH}:{e['line']}: package '{e['package']}' is not a workspace member"
                )
                continue
            if e["feature"] not in pkg.get("features", {}):
                problems.append(
                    f"{LIST_PATH}:{e['line']}: '{e['package']}' has no feature '{e['feature']}'"
                )
                continue
        try:
            expires = datetime.date.fromisoformat(e["expires"])
        except ValueError:
            problems.append(
                f"{LIST_PATH}:{e['line']}: expiry '{e['expires']}' is not YYYY-MM-DD"
            )
            continue
        if expires < today:
            problems.append(
                f"{LIST_PATH}:{e['line']}: entry for "
                f"{e['package']}/{e['feature']} expired on {e['expires']}"
            )

        guard_path = os.path.join(root, e["guard_file"])
        if not os.path.isfile(guard_path):
            problems.append(
                f"{LIST_PATH}:{e['line']}: guard file '{e['guard_file']}' does not exist"
            )
            continue
        text = open(guard_path, encoding="utf-8").read()
        if e["guard_literal"] not in text:
            problems.append(
                f"{LIST_PATH}:{e['line']}: guard literal not found in {e['guard_file']} — "
                f"the entry is stale; remove it in the commit that removes the guard"
            )
            continue
        # The literal must sit under a cfg for the listed feature, not merely appear in the file.
        pos = text.find(e["guard_literal"])
        window = text[max(0, pos - 800) : pos]
        cfgs = [m.group(1) for m in GUARD_NEAR.finditer(window)]
        if e["feature"] not in cfgs:
            problems.append(
                f"{LIST_PATH}:{e['line']}: the guard literal in {e['guard_file']} is not under "
                f"#[cfg(feature = \"{e['feature']}\")]"
            )

    # Completeness: every cfg-gated compile_error! in a workspace member's sources is listed.
    #
    # The root workspace manifest is also a package (`dwow_core`), so its directory is a prefix of
    # every other member and a naive per-package walk attributes the whole tree to it. Attribute
    # each file to the **longest** manifest-directory prefix instead, and walk each file once.
    if metadata is not None:
        members = workspace_members(metadata)
        pkg_dirs = sorted(
            ((os.path.dirname(p["manifest_path"]), p["name"]) for p in members),
            key=lambda pair: -len(pair[0]),
        )
        seen = set()
        for pkgdir, _name in pkg_dirs:
            src = os.path.join(pkgdir, "src")
            if not os.path.isdir(src):
                continue
            for dirpath, _dirs, files in os.walk(src):
                for name in files:
                    if not name.endswith(".rs"):
                        continue
                    path = os.path.abspath(os.path.join(dirpath, name))
                    if path in seen:
                        continue
                    seen.add(path)
                    owner = next(
                        n for d, n in pkg_dirs
                        if path == d or path.startswith(d + os.sep)
                    )
                    for m in GUARD_RE.finditer(open(path, encoding="utf-8").read()):
                        feature = m.group(1)
                        if (owner, feature) not in listed:
                            problems.append(
                                f"{os.path.relpath(path, root)}: guarded feature "
                                f"'{feature}' of '{owner}' is not in {LIST_PATH}"
                            )
    return problems


def cmd_check(root, entries, metadata, source_only):
    problems = source_check(root, entries, metadata)
    if not source_only:
        for e in entries:
            result = subprocess.run(
                ["cargo", "check", "-p", e["package"], "--all-features"],
                cwd=root,
                capture_output=True,
                text=True,
            )
            combined = result.stdout + result.stderr
            if result.returncode == 0:
                problems.append(
                    f"{LIST_PATH}:{e['line']}: {e['package']} with --all-features now "
                    f"compiles — the entry is stale; remove it and the guard together"
                )
            elif e["guard_literal"] not in combined:
                problems.append(
                    f"{LIST_PATH}:{e['line']}: {e['package']} --all-features failed, but not "
                    f"with the guard literal — a different breakage is being recorded as expected"
                )
    return problems


def write_list(path, rows):
    with open(path, "w", encoding="utf-8") as f:
        for row in rows:
            f.write("\t".join(row) + "\n")


def self_test(template_root):
    """Each branch must be able to fail. Uses source-only checks on a temp tree."""
    failures = []
    with tempfile.TemporaryDirectory(prefix="expected-failures-") as tmp:
        # A minimal workspace with one member and one guarded feature.
        os.makedirs(os.path.join(tmp, "crate", "src"))
        open(os.path.join(tmp, "Cargo.toml"), "w").write(
            '[workspace]\nmembers = ["crate"]\nresolver = "2"\n'
        )
        open(os.path.join(tmp, "crate", "Cargo.toml"), "w").write(
            '[package]\nname = "probe"\nversion = "0.1.0"\nedition = "2021"\n'
            '[features]\ndoomed = []\n'
        )
        guard = os.path.join(tmp, "crate", "src", "lib.rs")
        open(guard, "w").write(
            '#[cfg(feature = "doomed")]\ncompile_error!("SELF-TEST-GUARD");\n'
        )
        list_path = os.path.join(tmp, LIST_PATH)
        os.makedirs(os.path.dirname(list_path))
        write_list(list_path, [["probe", "doomed", "2999-01-01", "OBL-SELF", "crate/src/lib.rs", "SELF-TEST-GUARD"]])

        meta = cargo_metadata(tmp)
        entries = load_entries(tmp)
        if source_check(tmp, entries, meta):
            failures.append("clean twin did not pass")

        # 1. Remove the guard -> stale entry.
        open(guard, "w").write("// guard removed\n")
        problems = source_check(tmp, entries, meta)
        if not any("stale" in p or "guard literal not found" in p for p in problems):
            failures.append("a removed guard was not reported as stale")

        # 2. Restore the guard but add an unlisted guarded feature -> unlisted.
        open(guard, "w").write(
            '#[cfg(feature = "doomed")]\ncompile_error!("SELF-TEST-GUARD");\n'
            '#[cfg(feature = "unlisted")]\ncompile_error!("SELF-TEST-UNLISTED");\n'
        )
        open(os.path.join(tmp, "crate", "Cargo.toml"), "a").write("unlisted = []\n")
        os.remove(list_path)
        write_list(list_path, [["probe", "doomed", "2999-01-01", "OBL-SELF", "crate/src/lib.rs", "SELF-TEST-GUARD"]])
        problems = source_check(tmp, load_entries(tmp), cargo_metadata(tmp))
        if not any("not in" in p and "unlisted" in p for p in problems):
            failures.append("an unlisted guarded feature was not reported")

        # 3. Past expiry -> expired.
        os.remove(list_path)
        write_list(list_path, [["probe", "doomed", "2000-01-01", "OBL-SELF", "crate/src/lib.rs", "SELF-TEST-GUARD"]])
        problems = source_check(tmp, load_entries(tmp), meta)
        if not any("expired" in p for p in problems):
            failures.append("a past expiry was not reported")

    if failures:
        print("SELF-TEST FAILED:")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("SELF-TEST OK: stale, unlisted and expired entries are all reported; the clean twin passes")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=DEFAULT_ROOT)
    parser.add_argument("--cargo-features", action="store_true")
    parser.add_argument("--feature-names", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--source-only", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test(args.root)

    entries = load_entries(args.root)
    metadata = cargo_metadata(args.root) if (args.cargo_features or args.check) else None

    if args.cargo_features:
        cmd_cargo_features(entries, metadata)
        return 0
    if args.feature_names:
        cmd_feature_names(entries, metadata)
        return 0
    if args.check:
        problems = cmd_check(args.root, entries, metadata, args.source_only)
        if problems:
            print(f"FAIL: {len(problems)} problem(s) in {LIST_PATH}:")
            for problem in problems:
                print(f"  {problem}")
            return 1
        kind = "entry" if len(entries) == 1 else "entries"
        print(
            f"PASS: {len(entries)} expected-failure {kind} validated — unexpired, guard intact, "
            f"and the configuration still fails with its guard"
            if len(entries) == 1
            else f"PASS: {len(entries)} expected-failure {kind} validated — unexpired, guards "
            f"intact, and the configurations still fail with their guards"
        )
        return 0

    parser.error("choose one of --cargo-features, --feature-names, --check, --self-test")


if __name__ == "__main__":
    sys.exit(main())
