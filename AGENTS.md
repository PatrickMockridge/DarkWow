# AGENTS.md — how to work in this repository

Read this before changing anything. It is short on purpose. The rules below were each earned by a defect that
reached the tree, and most were earned **twice** — the lesson was recorded once, in a place the next agent did not
read, and the defect recurred.

`doc/src/arch/ai-index.md` is the map of the documentation. `doc/src/dev/hazop-rules.md` is the normative copy of
these rules, with the incidents written out.

---

## The rules

### On fixing

**R1 — A fix is never blocked by a test, a gate, a pin, or a ceremony.** Fix first; verification follows.
A test that fails because your fix changed the behaviour it pinned was *pinning the defect* — update it as part of
the fix. Never defer a correctness fix behind "the test would need updating", "the pin would move", "that must land
with X", or "that needs sign-off". Those are consequences of doing the fix.

*The one boundary, and it is not optional*: **never weaken a test to make a fix pass.** Deleting an assertion,
widening a tolerance, or skipping a case hides a regression rather than removing one. The test is whether the
**defect** changed the test's subject or the **convenience** did.

**R2 — Less is more: remove the root cause; do not add a compensating guard.** Every new guard is new footgun
surface. If a constant is unjustified, its absence needs no replacement. If a check cannot fail, delete it or fix
it — do not add a second check beside it.

**R3 — Removing a guard is a change like any other.** Name what it protected, on which path, and what now covers
that obligation. A guard derived from a number *dies with the number* unless the obligation is stated separately —
that is how the miner's "do not build what the transport will drop" was lost.

**R4 — Assess before applying.** State the blast radius, quantify before/after, and say what the change
**composes with**. Two changes made in one session can be two halves of one change; neither commit message will
say so.

### On evidence

**R5 — A fix is verified by a run, never by a gate exit code, a typecheck, or a clean diffstat.** Quote the run's
own line. `cargo check` passing is not evidence that behaviour changed.

**R6 — An audit, a HAZOP, an agent summary and a comment are hypotheses, not facts.** Read the code before acting
on any of them. Names lie in this tree: a field called `fn_code` holds a call-tree length prefix; a target called
`check-zkas-version` checks nothing; `BoundaryCodec::MAX_BYTES` has no reader.

**R7 — State what you did not establish.** "Cannot verify" is a result. A confident wrong root cause is worse than
an admitted unknown, because it gets built on.

**R8 — A bound is not a bound until something reads it**, and a check is not a check until something can make it
fail. Every check ships a **negative control**: run it against a planted defect and require a non-zero exit. A
static scan cannot find a check that cannot fail — measured: it finds 2 of 16.

### On authority

**R9 — Nothing is invented.** Every constant, bound, threshold or figure cites a measurement or a clause, and the
citation must resolve to something that exists. A round number inherited from nowhere is a defect. `L1 barrier #7`
— cited by `MAX_BLOCK_SIZE = 4 MiB` for years — exists in no document in this repository.

**R10 — A policy refusal is not a validity verdict.** A bound that *rejects* data is a consensus rule: it needs a
clause and a derivation. A node-local resource policy may drop, throttle, defer or refuse to serve — it must never
declare data invalid.

**R11 — Never leave a false statement in the tree, including in your own fix's comments.** Correct it where it is
**load-bearing** — the normative document, the spec-side description — not where you happened to be reading. Three
of seven false claims written in one session were corrected in one place while an identical copy stood elsewhere.

---

## Binding operating constraints

These are mechanical, not advisory. They are here because agents have broken each of them.

- **Heavy cargo is sequential, never parallel.** Compile memory is `-j` × per-`rustc`, so use `-j 8` — **never
  `-j 1`**, which throttles rather than fixes. Test-phase memory is concurrent in-process proving, so use
  `--test-threads 4` — **never 1**. `RAYON_NUM_THREADS=10` on every cargo invocation (`RAYON` does not affect
  rustc codegen memory).
- **Do not edit sources while a build is running.** cargo reads the tree as it compiles, so an edit landing
  mid-build is compiled in whatever half-finished state it happened to be in. The failure then presents as *the
  run's own result* — a build error where a test verdict was expected — which is worse than a wrong answer,
  because it looks like an answer. Finish the edit, then start the build.
- **Everything heavy runs inside a cgroup scope**, the repository's own pattern
  (`scripts/lean-build.sh`): `systemd-run --user --scope -q --unit=<name> -p MemoryMax=28G -p MemorySwapMax=0
  -- bash -c '<cmd>'`. **Lean is the exception** — it always goes through `scripts/lean-build.sh`, which owns its
  own thread cap and memory ceiling. Never wrap Lean in another scope; never hand-set a Lean cap.
- **Never add a timeout** to a Bash call running cargo or the pipeline. Send all test output to `/tmp/`, complete,
  never truncated.
- **`RUST_MIN_STACK = 67108864`** comes from `.cargo/config.toml`'s `[env]`; do not remove it. halo2's FFT and
  multi-exponentiation recurse deeply and `SIGSEGV` on default 8 MB stacks.
- **The toolchain is pinned** by `rust-toolchain.toml`. `channel = "stable"` silently neutralised that pin once —
  the file's comment records it.
- **`src/sdk/**` and `src/serial/**` are in all 32 contracts' `SOURCE_MANIFEST`.** One edit there stales every
  artifact at once. Rebuild once, after the last such edit.
- **A stale contract artifact cannot be refreshed by `make all`.** It is `make -C src/contract/<name> clean &&
  make -C src/contract/<name> all`; the Makefile exits 1 and says so.
- **`bin/dwowd/genesis_hash.txt`** pins genesis (`c8afdeb7…`). Any change to a genesis artifact's inputs —
  including `src/sdk/**` — moves it: rebuild, recompute, re-record, re-run the pin test
  (`bin/dwowd/src/tests/genesis.rs`) and its negative control. This is a routine, recorded procedure the repository
  has performed repeatedly; it is **not** a reason to defer a fix (R1).
- **Never modify `src/zk/vm.rs`.** The VM is off-limits.
- **Never `sed` Rust.** Use precise edits so changes are auditable.
- **Never `git add -A`, `git stash`, `checkout` or `reset` in the shared tree.** Several sessions share it; commit
  by explicit pathspec (`git commit -F - -- <paths>`). Re-run `git status --porcelain` before every edit, and
  commit early — another session staging whole files can otherwise carry your edit away.
- **`src/contract/deployooor/**` and the other genesis contracts are production-critical.** Never mock, shortcut
  or work around genesis in testing. Changing it deliberately, with the pin re-recorded, is normal work.

---

## How to work

**When asked for a HAZOP**, it is analysis: root causes only, structure before patches, and **no implementation**
until the findings are synthesised and approved. Apply guide words — *NO/NOT, MORE, LESS, AS WELL AS, PART OF,
REVERSE, OTHER THAN* — to every parameter, and fill *"why the safeguard did not fire"* as well as *"what
happened"*. The last column, *"what rule would have prevented this being introduced"*, is what makes a HAZOP
produce doctrine instead of a bug list.

**When fixing**, in order: read the code (R6); state the root cause and what it composes with (R4); remove the
cause rather than adding a guard (R2); name what any removed guard protected (R3); witness it with a run (R5); and
correct every copy of any claim you invalidated (R11).

**When you are about to defer, stop.** If the reason is a test, a gate, a pin or a process, it is R1 and the answer
is to do the fix. If the reason is that you do not understand the mechanism, say so (R7) and measure — but do not
turn the not-knowing into an obstacle for the work.
