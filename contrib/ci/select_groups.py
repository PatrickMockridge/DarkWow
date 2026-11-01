#!/usr/bin/env python3
"""Map a changed-file list to CI groups.

The selector is deliberately a pure function of paths: it is tested with a planted list, so a PR
cannot silently skip a group. It does not read the tree or the diff itself except in `--base/--head`
mode, where it shells out to `git diff`.

Groups: docs, contracts, shared-sdk, genesis, consensus, wallet, lean, models, ci, docker.
`always` is not a selected group — every lane runs it unconditionally.
"""

import argparse
import os
import subprocess
import sys

ALL_GROUPS = (
    "docs", "contracts", "shared-sdk", "genesis", "consensus",
    "wallet", "lean", "models", "ci", "docker",
)

GENESIS = frozenset({
    "deployooor", "native_token", "promissory_note", "identity", "oracle",
    "attestation", "purse", "box", "multisig",
})

ROOT_DOCS = {"AGENTS.md", "CLAUDE.md", "REPO-MAP.md", "README.md", "LICENSE.md"}

SHARED_ROOT = {
    "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
    "script/contract-build-recipe.mk", "script/contract_build_recipe.txt",
}

CONSENSUS_PREFIXES = (
    "src/linear/", "src/runtime/", "src/tx/", "src/blockchain/", "src/net/",
    "src/transport/", "src/rpc/", "src/event_graph/", "src/dht/", "src/geode/",
    "src/core/", "src/util/", "src/concurrency/", "bin/dwowd/",
    "crates/dwow-mempool/",
)

LEAN_PREFIXES = ("proofs/lean/",)
LEAN_FILES = {
    "scripts/lean-build.sh",
    "scripts/check-lean-suite.sh",
    "scripts/gen_circuit_transcription.py",
    "scripts/gen_circuit_index.py",
    "script/check_lean_axioms.py",
    "script/circuit_index_map.txt",
}

MODEL_PREFIXES = ("contrib/model/",)
MODEL_FILES = {"contrib/docker/darkwow-testnet/merge_mining_model.py"}

CI_PREFIXES = ("contrib/ci/", "scripts/", "hooks/", ".github/")
CI_FILES = {"Makefile"}


def classify_path(path):
    """Return the set of groups a single changed path selects."""
    path = path.strip()
    if not path:
        return set()
    base = os.path.basename(path)
    groups = set()

    if path.startswith("doc/") or path.endswith(".md") or path in ROOT_DOCS:
        groups.add("docs")
    if path.startswith("src/contract/") or path.startswith("proofs/core/"):
        groups.add("contracts")
        if path.startswith("src/contract/"):
            parts = path.split("/")
            if len(parts) > 2 and parts[2] in GENESIS:
                groups.add("genesis")
        if "/proof/" in path:
            groups.add("lean")
    if path.startswith(("src/sdk/", "src/serial/", "src/zk/", "src/zkas/", "vendor/")):
        groups.update(("shared-sdk", "contracts"))
    if path in SHARED_ROOT or path.startswith(".cargo/"):
        groups.update(("shared-sdk", "contracts", "consensus"))
    if path.startswith("src/linear/"):
        groups.update(("consensus", "models"))
    if path.startswith(CONSENSUS_PREFIXES):
        groups.add("consensus")
    if path.startswith(("bin/dww/", "crates/dwow-accounts/", "bin/darkwow/")):
        groups.update(("wallet", "models"))
    if path.startswith(LEAN_PREFIXES) or path in LEAN_FILES:
        groups.add("lean")
    if path.startswith(MODEL_PREFIXES) or path in MODEL_FILES:
        groups.add("models")
    if path.startswith(CI_PREFIXES) or path in CI_FILES:
        groups.add("ci")
    if path.startswith("contrib/docker/") or "Dockerfile" in base:
        groups.update(("docker", "shared-sdk"))
    if not groups and path.startswith(("bin/", "src/", "crates/")):
        groups.add("consensus")
    if not groups:
        return {"unmatched"}
    return groups


def classify(paths):
    groups = set()
    for path in paths:
        groups |= classify_path(path)
    unmatched = "unmatched" in groups
    if unmatched:
        groups = set(ALL_GROUPS)
    return groups, unmatched


def changed_paths(base, head):
    if not base or not head:
        return None
    cmd = ["git", "diff", "--name-only", "--diff-filter=ACMRD", f"{base}...{head}"]
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return [line for line in out.splitlines() if line.strip()]


def self_test():
    failures = []

    def expect(path, want):
        got = classify_path(path)
        if want not in got:
            failures.append(f"{path}: expected {want}, got {sorted(got)}")

    expect("doc/src/arch/type-system.md", "docs")
    expect("AGENTS.md", "docs")
    expect("src/sdk/src/lib.rs", "shared-sdk")
    expect("src/sdk/src/lib.rs", "contracts")
    expect("src/serial/src/lib.rs", "contracts")
    expect("Cargo.lock", "consensus")
    expect("src/contract/game_room/src/lib.rs", "contracts")
    expect("src/contract/box/src/lib.rs", "genesis")
    expect("src/contract/box/proof/put.zk", "lean")
    expect("src/linear/src/chain_state.rs", "models")
    expect("bin/dwowd/src/lib.rs", "consensus")
    expect("bin/dww/src/scan.rs", "wallet")
    expect("proofs/lean/src/DarkFi/Gadgets.lean", "lean")
    expect("contrib/model/chain_model.py", "models")
    expect("scripts/run-all-tests.sh", "ci")
    expect("contrib/docker/darkwow-testnet/Dockerfile", "docker")

    got, unmatched = classify(["totally/unknown.txt"])
    if not unmatched or got != set(ALL_GROUPS):
        failures.append("an unmatched path must select every group and report unmatched")

    # Assembled at run time: `scripts/check-doc-index.sh` scans .py files for `.md` citations and
    # would read a literal fixture name as a citation of a missing document.
    fixture_doc = "doc/src/" + "x" + ".m" + "d"
    got, unmatched = classify([fixture_doc])
    if unmatched or got != {"docs"}:
        failures.append(f"a docs-only change selected {sorted(got)}")

    if failures:
        for f in failures:
            print(f"SELF-TEST FAIL: {f}")
        return 1
    print("SELF-TEST PASS: selector groups docs/shared/contracts/genesis/lean/models/ci/docker "
          "and the unmatched fallback")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default="")
    parser.add_argument("--head", default="")
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--github-output", default="")
    parser.add_argument("--list", action="store_true", help="print the changed paths and groups")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    paths = None
    if not args.all:
        paths = changed_paths(args.base, args.head)
    if paths is None:
        groups = set(ALL_GROUPS)
        unmatched = False
        reason = "all (no usable diff)"
    else:
        groups, unmatched = classify(paths)
        reason = "unmatched path present — all groups" if unmatched else "diff"

    ordered = [g for g in ALL_GROUPS if g in groups]
    code = bool(groups & {"contracts", "shared-sdk", "consensus", "wallet"})
    models = "models" in groups
    lean = "lean" in groups
    proof = bool(groups & {"contracts", "shared-sdk", "consensus"})
    values = {
        "groups": ",".join(ordered),
        "code": "true" if code else "false",
        "models": "true" if models else "false",
        "lean": "true" if lean else "false",
        "proof": "true" if proof else "false",
        "unmatched": "true" if unmatched else "false",
        "reason": reason,
    }

    if args.list:
        for path in paths or []:
            print(f"{path}\t{','.join(sorted(classify_path(path)))}")
        print(f"# {values}")
    if args.github_output:
        with open(args.github_output, "a", encoding="utf-8") as fh:
            for key, value in values.items():
                fh.write(f"{key}={value}\n")
    if not args.list:
        for key in ("groups", "code", "models", "lean", "proof", "unmatched", "reason"):
            print(f"{key}={values[key]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
