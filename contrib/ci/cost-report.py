#!/usr/bin/env python3
"""Nightly cost report for the CI lanes.

The pipeline's Pareto claim is not a slogan: each lane records measured seconds per gate
(`reports/ci/gate-times-<lane>.tsv`, and a 6-column global row in
the runner's `~/.cache/darkwow/metrics/gate-times.tsv`). This report reads those rows and prints per-label
percentiles, per-lane totals, the slowest gates, gates with no data in 14 days, duplicate
(commit,label) pairs across lanes, and budget breaches.

Report-only by default; `--check` makes the budget and duplicate findings fail. The repo's gates
are allowed to make a claim fail only when the check that makes it can fail too, so the same
thresholds are used by `--self-test` on a planted row set.
"""

import argparse
import collections
import datetime as dt
import os
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "contrib" / "ci" / "gate-groups.tsv"
METRICS = pathlib.Path(os.environ.get("CI_METRICS_FILE", os.path.expanduser("~/.cache/darkwow/metrics/gate-times.tsv")))
BUDGETS = {"fast": 300, "merge": 3600, "nightly": 7200}


def read_rows(paths):
    rows = []
    for path in paths:
        if not path.exists():
            continue
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            parts = line.split("\t")
            if len(parts) == 6:
                date, commit, lane, label, elapsed, status = parts
            elif len(parts) == 4:
                date, label, elapsed, status = parts
                lane = path.stem.replace("gate-times-", "")
                commit = "?"
            else:
                continue
            try:
                rows.append({
                    "date": date, "commit": commit, "lane": lane, "label": label,
                    "elapsed": int(elapsed), "status": int(status),
                })
            except ValueError:
                continue
    return rows


def percentile(values, q):
    if not values:
        return 0
    values = sorted(values)
    idx = min(len(values) - 1, int(round((len(values) - 1) * q)))
    return values[idx]


def report(rows, now=None):
    now = now or dt.datetime.now(dt.timezone.utc)
    by_label = collections.defaultdict(list)
    by_lane = collections.defaultdict(list)
    pairs = collections.defaultdict(set)
    for row in rows:
        by_label[row["label"]].append(row["elapsed"])
        by_lane[row["lane"]].append(row["elapsed"])
        pairs[(row["commit"], row["label"])].add(row["lane"])

    print("CI cost report")
    print(f"  rows={len(rows)} labels={len(by_label)} lanes={len(by_lane)}")
    print("")
    print("Per lane (sum of gate seconds):")
    for lane, values in sorted(by_lane.items()):
        print(f"  {lane:18s} gates={len(values):3d} sum={sum(values):7d}s")
    print("")
    print("Top 10 gates by p95:")
    ranked = sorted(by_label.items(), key=lambda kv: percentile(kv[1], 0.95), reverse=True)[:10]
    for label, values in ranked:
        print(f"  p50={percentile(values, 0.5):6d}s p95={percentile(values, 0.95):6d}s "
              f"max={max(values):6d}s n={len(values):2d}  {label}")
    print("")
    print("Duplicate (commit,label) pairs across lanes:")
    dupes = [key for key, lanes in pairs.items() if len(lanes) > 1]
    for commit, label in sorted(dupes)[:20]:
        print(f"  {commit} {label}")
    print(f"  total={len(dupes)} (target 0)")

    breaches = []
    for lane, budget in BUDGETS.items():
        used = sum(by_lane.get(lane, []))
        if used > budget:
            breaches.append((lane, used, budget))
    print("")
    print("Budget breaches:")
    for lane, used, budget in breaches:
        print(f"  {lane}: {used}s > {budget}s")
    print(f"  total={len(breaches)}")

    if MANIFEST.exists():
        labels = {
            line.split("\t", 1)[0]
            for line in MANIFEST.read_text(encoding="utf-8").splitlines()
            if line and not line.startswith("#") and "\t" in line
        }
        seen_at = {}
        for row in rows:
            try:
                when = dt.datetime.fromisoformat(row["date"].replace("Z", "+00:00"))
            except ValueError:
                continue
            if when.tzinfo is None:
                when = when.replace(tzinfo=dt.timezone.utc)
            seen_at[row["label"]] = max(seen_at.get(row["label"], when), when)
        stale = sorted(label for label in labels
                       if (now - seen_at.get(label, now - dt.timedelta(days=15))).days > 14)
        print("")
        print(f"Gates with no data in 14 days: {len(stale)}")
        for label in stale[:20]:
            print(f"  {label}")

    return breaches, dupes


def self_test():
    now = dt.datetime.now(dt.timezone.utc)
    rows = [
        {"date": now.isoformat(), "commit": "c1", "lane": "fast", "label": "a",
         "elapsed": 10, "status": 0},
        {"date": now.isoformat(), "commit": "c2", "lane": "fast", "label": "a",
         "elapsed": 20, "status": 0},
        {"date": now.isoformat(), "commit": "c1", "lane": "merge", "label": "a",
         "elapsed": 30, "status": 0},
        {"date": now.isoformat(), "commit": "c1", "lane": "merge", "label": "b",
         "elapsed": 4000, "status": 0},
    ]
    breaches, dupes = report(rows, now=now)
    if not dupes:
        print("SELF-TEST FAIL: duplicate (commit,label) pair not reported")
        return 1
    if not any(lane == "merge" and used == 4030 for lane, used, _ in breaches):
        print(f"SELF-TEST FAIL: merge budget breach not reported: {breaches}")
        return 1
    print("SELF-TEST PASS: duplicate pair and budget breach are named")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    paths = [METRICS] + sorted((ROOT / "reports" / "ci").glob("gate-times-*.tsv"))
    rows = read_rows(paths)
    breaches, dupes = report(rows)
    if args.check and (breaches or dupes):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
