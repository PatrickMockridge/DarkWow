#!/usr/bin/env bash
#
# clippy_unwrap_audit.sh — a census of surviving `unwrap()`/`expect()` under the per-crate
# `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`.
#
# WHY IT EXISTS. `contrib/clippy_critical_counts.sh` enumerates the same lints but over a hand-typed
# list of 12 crates, and it classifies any non-zero cargo exit as "failed to lint" (unmeasured) rather
# than as a hit. With the deny active, a surviving `.unwrap()` *is* the non-zero exit — so that
# classification hides exactly the sites this audit is for. `make clippy` (a `--workspace` run) aborts
# on the first crate that errors, so it never lints the crates downstream of the abort; that is how a
# rollout could claim the tree clean while survivors remained. This script is the census that can
# fail: it derives its scope instead of typing it, lints every configuration a crate is actually
# built in, and distinguishes "clean" from "unmeasured".
#
# WHAT IT DOES.
#   * Scope is DERIVED: a package is in scope iff a lib/bin target root file carries the deny. The
#     in-scope/out-of-scope partition is asserted complete and disjoint, and cross-checked against an
#     independent `grep -rl`; the out-of-scope set is printed (so "no deny" is a recorded decision).
#   * Three configurations per crate, each recorded: host `--all-features` (`--all-targets`, so benches
#     and examples — which are `cfg(not(test))` — are covered); host `--no-default-features` when any
#     source is `cfg(not(feature = …))`; and `--target=wasm32-unknown-unknown` default when any source
#     is `cfg(target_arch = "wasm32")` (the configuration the shipped contract `.wasm` is built in).
#   * Classification is by lint code, never rendered text. Per (crate, config):
#       hits > 0                          -> DIRTY
#       hits == 0, exit == 0              -> clean
#       hits == 0, exit != 0              -> INSTRUMENT FAILURE (unmeasured, never "clean") -> exit 2
#
# USAGE
#   contrib/clippy_unwrap_audit.sh                  whole-repo census
#   contrib/clippy_unwrap_audit.sh --crate <pkg>    one package (all its configurations)
#   contrib/clippy_unwrap_audit.sh --scope-only     print the derived scope, lint nothing
#   contrib/clippy_unwrap_audit.sh --self-test      negative control (planted defect); exit 0 iff caught
# EXIT: 0 clean · 1 hits remain · 2 instrument failure · 3 self-test failure.
#
# Every cargo invocation runs inside the repository's heavy envelope: a flock around a cgroup scope,
# so concurrent agents serialize and never exceed the memory ceiling.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

DENY_PAT='cfg_attr(not(test), deny(clippy::unwrap_used'
HOST="$(rustc -Vv | awk '/^host:/{ print $2 }')"
WASM_TARGET="wasm32-unknown-unknown"
LOCK=/tmp/darkfi-heavy.lock
OUT_DIR="${CLIPPY_AUDIT_DIR:-/tmp}"
ERRS="$OUT_DIR/clippy-unwrap-audit.stderr.log"
command -v jq >/dev/null 2>&1 || { echo "clippy_unwrap_audit: jq is required" >&2; exit 2; }

JQ_HITS='select(.reason=="compiler-message") | .message
  | select(.code != null)
  | select(.code.code | test("^(clippy::)?(unwrap_used|expect_used)$"))
  | .code.code as $l
  | (.spans[] | select(.is_primary) | "\($l)\t\(.file_name):\(.line_start)")'

# ── derived scope ─────────────────────────────────────────────────────────────────────────────
# package -> lib/bin target root file(s).  Prints "pkg<TAB>root1,root2".
derive_scope() {
  cargo metadata --no-deps --format-version=1 --offline 2>/dev/null | jq -r '
    .packages[] | . as $p
    | ([ $p.targets[]
         | select(([.kind[] | (. == "test" or . == "bench" or . == "custom-build" or . == "example")] | any) | not)
         | .src_path ] | unique) as $roots
    | select($roots | length > 0)
    | "\($p.name)\t\($roots | join(","))"'
}

# A package is in scope iff a root file carries the deny literal.
is_in_scope() {  # $1 = comma-separated root files
  local f
  local -a rs
  IFS=',' read -ra rs <<< "$1"
  for f in "${rs[@]}"; do
    [[ -f "$f" ]] && grep -qF "$DENY_PAT" "$f" && return 0
  done
  return 1
}

print_scope() {
  local total=0 ins=0 outs=0 line pkg roots
  while IFS=$'\t' read -r pkg roots; do
    total=$((total + 1))
    if is_in_scope "$roots"; then
      ins=$((ins + 1)); echo "IN   $pkg"
    else
      outs=$((outs + 1)); echo "OUT  $pkg"
    fi
  done < <(derive_scope)
  echo "---"
  echo "packages=$total in-scope=$ins out-of-scope=$outs"
  # independent cross-check of the in-scope membership (grep vs cargo-metadata derivation)
  local grep_set derived_set gn dn
  grep_set="$(grep -rlF "$DENY_PAT" --include='lib.rs' --include='main.rs' src bin crates 2>/dev/null | sed "s#^#$REPO_ROOT/#" | sort -u)"
  derived_set="$(while IFS=$'\t' read -r pkg roots; do
      is_in_scope "$roots" || continue
      for rf in ${roots//,/ }; do grep -qF "$DENY_PAT" "$rf" && echo "$rf"; done
    done < <(derive_scope) | sort -u)"
  gn="$(printf '%s\n' "$grep_set" | grep -c .)"
  dn="$(printf '%s\n' "$derived_set" | grep -c .)"
  echo "grep root files:    $gn"
  echo "derived root files: $dn"
  if [[ "$gn" != "$dn" ]] || ! diff <(printf '%s\n' "$grep_set") <(printf '%s\n' "$derived_set") >/dev/null; then
    echo "SCOPE MISMATCH (exit 2 territory):"
    diff <(printf '%s\n' "$grep_set") <(printf '%s\n' "$derived_set") | head -20
  fi
}

# ── one configuration ─────────────────────────────────────────────────────────────────────────
# lint_config <pkg> <label> <cargo-args...>   -> prints hits; sets LAST_EXIT, LAST_HITS
lint_config() {
  local pkg="$1" label="$2"; shift 2
  local tag json
  tag="$(printf '%s-%s' "$pkg" "$label" | tr -c 'a-zA-Z0-9-' '-')"
  json="$OUT_DIR/clippy-audit-$tag.json"
  { printf '\n===== %s [%s] =====\n' "$pkg" "$label"; } >> "$ERRS"
  flock "$LOCK" systemd-run --user --scope -q --unit="df-audit-$tag" \
    -p MemoryMax=28G -p MemorySwapMax=0 -- \
    bash -c "cd '$REPO_ROOT' && RAYON_NUM_THREADS=10 cargo clippy -j 8 -p '$pkg' $* --no-deps --message-format=json -- -W clippy::unwrap_used -W clippy::expect_used" \
    > "$json" 2>>"$ERRS"
  LAST_EXIT=$?
  LAST_HITS="$(jq -r "$JQ_HITS" "$json" 2>/dev/null | sort -u)"
  LAST_COUNT="$(printf '%s\n' "$LAST_HITS" | grep -c .)"
}

# configs_for <pkg> — prints the labels+arg-sets to run, one per line as "label|args"
configs_for() {
  local pkg="$1" src
  src="$(cargo metadata --no-deps --format-version=1 --offline 2>/dev/null \
        | jq -r --arg p "$pkg" '.packages[] | select(.name==$p) | .manifest_path' | xargs dirname)"
  echo "host-allfeatures|--target=$HOST --release --all-features --all-targets"
  if grep -rqE 'cfg\(not\(feature' "$src/src" 2>/dev/null; then
    echo "host-nodefaults|--target=$HOST --release --no-default-features --all-targets"
  fi
  if grep -rqE 'target_arch = "wasm32"' "$src/src" 2>/dev/null || ls "$src"/*.wasm >/dev/null 2>&1; then
    echo "wasm-default|--target=$WASM_TARGET --release --lib"
  fi
}

audit_pkg() {  # $1 = package; returns 0 clean, 1 dirty, 2 instrument failure
  local pkg="$1" dirty=0 fail=0 cfg label args
  echo "### $pkg"
  while IFS='|' read -r label args; do
    [[ -z "$label" ]] && continue
    # shellcheck disable=SC2086
    lint_config "$pkg" "$label" $args
    if [[ "$LAST_COUNT" -gt 0 ]]; then
      dirty=1
      printf 'DIRTY  %s [%s]: %s hit(s)\n' "$pkg" "$label" "$LAST_COUNT"
      printf '%s\n' "$LAST_HITS" | sed 's/^/    /'
    elif [[ "$LAST_EXIT" -ne 0 ]]; then
      fail=1
      printf 'UNMEASURED  %s [%s]: exit %s, no lint hits (see %s)\n' "$pkg" "$label" "$LAST_EXIT" "$ERRS"
    else
      printf 'clean  %s [%s]\n' "$pkg" "$label"
    fi
  done < <(configs_for "$pkg")
  [[ "$fail" -eq 1 ]] && return 2
  [[ "$dirty" -eq 1 ]] && return 1
  return 0
}

# ── self-test (R8) ─────────────────────────────────────────────────────────────────────────────
# Plant a `.unwrap()` and an `.expect(` in a dependency-free crate carrying the identical deny, and a
# clean twin; run the SAME cargo->jq->count pipeline; require dirty -> 2 hits and clean -> 0.
self_test() {
  local tmp; tmp="$(mktemp -d)"; trap 'rm -rf "${tmp:-}"' EXIT
  mkdir -p "$tmp/dirty/src" "$tmp/clean/src"
  for d in dirty clean; do
    cat > "$tmp/$d/Cargo.toml" <<EOF
[package]
name = "audit_$d"
version = "0.0.0"
edition = "2021"
[lib]
path = "src/lib.rs"
EOF
  done
  cat > "$tmp/dirty/src/lib.rs" <<'EOF'
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
pub fn f(x: Option<u8>) -> u8 { x.unwrap() }
pub fn g(x: Option<u8>) -> u8 { x.expect("planted") }
EOF
  cat > "$tmp/clean/src/lib.rs" <<'EOF'
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
pub fn f(x: Option<u8>) -> u8 { x.unwrap_or(0) }
pub fn g(x: Option<u8>) -> u8 { x.unwrap_or(0) }
EOF
  local bad=0 hits ex
  for d in dirty clean; do
    ( cd "$tmp/$d" && RAYON_NUM_THREADS=10 cargo clippy -q --message-format=json -- -W clippy::unwrap_used -W clippy::expect_used ) \
      > "$tmp/$d.json" 2>/dev/null
    ex=$?
    hits="$(jq -r "$JQ_HITS" "$tmp/$d.json" 2>/dev/null | sort -u | grep -c .)"
    case "$d" in
      dirty) [[ "$hits" -eq 2 ]] || { echo "SELF-TEST FAIL: planted crate -> $hits hits (want 2)"; bad=1; } ;;
      clean) [[ "$hits" -eq 0 && "$ex" -eq 0 ]] || { echo "SELF-TEST FAIL: clean crate -> $hits hits, exit $ex"; bad=1; } ;;
    esac
  done
  [[ "$bad" -eq 0 ]] && { echo "SELF-TEST PASS: the pipeline detects a planted unwrap/expect and clears a clean twin"; return 0; }
  return 1
}

# ── dispatch ─────────────────────────────────────────────────────────────────────────────────
mode="census"; only_crate=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --self-test) self_test; exit $? ;;
    --scope-only) mode="scope"; shift ;;
    --crate) only_crate="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ "$mode" == "scope" ]]; then print_scope; exit 0; fi

rc=0
if [[ -n "$only_crate" ]]; then
  audit_pkg "$only_crate"; rc=$?
else
  while IFS=$'\t' read -r pkg roots; do
    is_in_scope "$roots" || continue
    audit_pkg "$pkg"; r=$?
    [[ "$r" -eq 2 ]] && rc=2
    [[ "$r" -eq 1 && "$rc" -ne 2 ]] && rc=1
  done < <(derive_scope)
fi
echo "---"
case "$rc" in
  0) echo "AUDIT: clean";;
  1) echo "AUDIT: hits remain";;
  2) echo "AUDIT: INSTRUMENT FAILURE (a crate/config went unmeasured)";;
esac
exit "$rc"
