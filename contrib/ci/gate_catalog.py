#!/usr/bin/env python3
"""Collect the gate labels the CI ratchet compares.

Two sources:
  * the umbrella's own inventory, via `scripts/run-all-tests.sh --list-gates` (the list-only mode
    expands the 119 call sites, including the control loop, without running a gate);
  * the CI lane entrypoint, via its `gate "LABEL" ...` calls (the fast-only commands).

A label containing a shell substitution is refused: an unexpanded dynamic label would silently
escape the classification, which is the drift this catalog exists to prevent.
"""

import argparse
import glob
import os
import re
import subprocess
import sys

GATE_RE = re.compile(r'^\s*gate\s+"([^"]+)"', re.M)


def repo_root():
    return os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def umbrella_labels(root):
    proc = subprocess.run(
        ["bash", os.path.join(root, "scripts", "run-all-tests.sh"), "--list-gates"],
        cwd=root, capture_output=True, text=True)
    if proc.returncode != 0:
        raise SystemExit(f"umbrella --list-gates failed: {proc.stderr.strip()}")
    labels = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    bad = [label for label in labels if "$(" in label or "`" in label]
    if bad:
        raise SystemExit("dynamic gate label(s) in the umbrella; extend gate_catalog.py: "
                         + repr(bad))
    if len(labels) != len(set(labels)):
        dupes = sorted({label for label in labels if labels.count(label) > 1})
        raise SystemExit(f"duplicate gate label(s) in the umbrella: {dupes}")
    return labels


def lane_labels(root):
    """Fast-only labels are the `gate "..."` calls in the lane entrypoint."""
    path = os.path.join(root, "contrib", "ci", "run-ci-lane.sh")
    if not os.path.exists(path):
        return []
    return sorted(set(GATE_RE.findall(open(path, encoding="utf-8").read())))


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--umbrella", action="store_true")
    parser.add_argument("--groups", action="store_true")
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args(argv)
    root = repo_root()

    umb = umbrella_labels(root)
    grp = lane_labels(root)
    if args.umbrella:
        print("\n".join(umb))
    if args.groups:
        print("\n".join(grp))
    if args.list:
        for label in umb:
            print(f"umbrella\t{label}")
        for label in grp:
            if label not in umb:
                print(f"fast-only\t{label}")
    if not (args.umbrella or args.groups or args.list):
        print(f"umbrella={len(umb)} group_scripts={len(grp)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
