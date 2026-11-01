#!/usr/bin/env python3
"""The CI gate-coverage ratchet.

Every label the umbrella can execute must have exactly one row in `gate-groups.tsv`; every label a
CI group script defines must be classified too; a row whose `source=fast-only` must be produced by a
group script; a row whose `source=umbrella` must exist in the umbrella; every group must have a
`gates/<group>.sh`. A new gate therefore fails this check until someone says where it belongs.

`--self-test` plants each defect and requires a non-zero exit naming it. The live check runs on
every fast lane, so the contract cannot rot.
"""

import argparse
import os
import sys

import gate_catalog

MANIFEST = os.path.join("contrib", "ci", "gate-groups.tsv")
LANE_ENTRYPOINT = os.path.join("contrib", "ci", "run-ci-lane.sh")
REQUIRED_COLUMNS = 5


def load_manifest(root):
    rows = []
    path = os.path.join(root, MANIFEST)
    if not os.path.exists(path):
        return rows, [f"missing manifest {MANIFEST}"]
    with open(path, encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, 1):
            line = line.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) != REQUIRED_COLUMNS:
                return rows, [f"{MANIFEST}:{lineno}: expected {REQUIRED_COLUMNS} tab columns, "
                              f"got {len(parts)}"]
            rows.append(tuple(parts))
    return rows, []


def validate(root, umbrella, lane_labels, manifest_rows):
    errors = []
    umbrella_set, lane_set = set(umbrella), set(lane_labels)
    seen = {}
    for label, group, lanes, source, budget in manifest_rows:
        if label in seen:
            errors.append(f"{label}: classified {seen[label]} time(s); one row per gate")
        seen[label] = seen.get(label, 0) + 1
        if source not in ("umbrella", "fast-only"):
            errors.append(f"{label}: source must be umbrella or fast-only, got {source!r}")
        if source == "umbrella" and label not in umbrella_set:
            errors.append(f"{label}: source=umbrella but the umbrella does not execute it")
        if source == "fast-only" and label not in lane_set:
            errors.append(f"{label}: source=fast-only but the lane entrypoint does not define it")
        if not group.strip():
            errors.append(f"{label}: empty group")
        if not lanes.strip():
            errors.append(f"{label}: empty lanes")
        if not budget.isdigit():
            errors.append(f"{label}: budget_s must be an integer, got {budget!r}")

    for label in umbrella:
        if label not in seen:
            errors.append(f"{label}: umbrella gate is unclassified — add a row to {MANIFEST}")
    for label in lane_labels:
        if label not in seen:
            errors.append(f"{label}: lane gate is unclassified — add a row to {MANIFEST}")
    if lane_labels and not os.path.exists(os.path.join(root, LANE_ENTRYPOINT)):
        errors.append(f"missing lane entrypoint {LANE_ENTRYPOINT}")
    return errors


def self_test():
    umbrella = ["a", "b"]
    lane = ["a", "fastonly"]
    rows = [
        ("a", "always", "fast", "umbrella", "1"),
        ("b", "docs", "fast", "umbrella", "1"),
        ("fastonly", "ci", "fast", "fast-only", "1"),
    ]
    failures = []
    if validate(".", umbrella, lane, rows):
        failures.append("clean fixture was rejected")
    bad_rows = rows + [("b", "docs", "fast", "umbrella", "1")]
    if not any("one row per gate" in e for e in validate(".", umbrella, lane, bad_rows)):
        failures.append("duplicate row not reported")
    if not any("unclassified" in e for e in validate(".", umbrella + ["new"], lane, rows)):
        failures.append("unclassified umbrella gate not reported")
    if not any("unclassified" in e for e in validate(".", umbrella, lane + ["orphan"], rows)):
        failures.append("orphan lane label not reported")
    bad_lane = rows + [("fastonly2", "ci", "fast", "fast-only", "1")]
    if not any("lane entrypoint does not define it" in e for e in
               validate(".", umbrella, lane, bad_lane)):
        failures.append("fast-only row with no lane command not reported")
    bad_source = rows + [("a", "always", "fast", "other", "1")]
    if not any("source must be" in e for e in validate(".", umbrella, lane, bad_source)):
        failures.append("unknown source not reported")
    if failures:
        for failure in failures:
            print(f"SELF-TEST FAIL: {failure}")
        return 1
    print("SELF-TEST PASS: duplicate, unclassified, orphan, stale-source and bad-source "
          "defects are each named; the clean twin passes")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()

    root = gate_catalog.repo_root()
    try:
        umbrella = gate_catalog.umbrella_labels(root)
    except SystemExit as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    lane = gate_catalog.lane_labels(root)
    rows, errors = load_manifest(root)
    errors += validate(root, umbrella, lane, rows)
    if errors:
        print(f"FAIL: {len(errors)} gate-coverage defect(s):")
        for error in errors:
            print(f"  {error}")
        return 1
    n_controls = sum(1 for label in umbrella if label.startswith("control:"))
    fast_only = sum(1 for row in rows if row[3] == "fast-only")
    print(f"PASS: {len(umbrella)} umbrella gates "
          f"({len(umbrella) - n_controls} instruments, {n_controls} controls), "
          f"{fast_only} fast-only, 0 unclassified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
