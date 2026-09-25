# HAZOP and remediation rules

The normative copy of the rules agents in this repository work under. `AGENTS.md` at the repository root is the
short operational form; this page is the same rules with **the incident that earned each one** written out, because
a rule without its incident is a slogan and gets rationalised away.

These were not designed. They were extracted from three HAZOPs and three adversarial audits of the critical path in
September 2026, and every one of them names a defect that reached the tree — most of them **twice**, having been
learned once and recorded where the next agent did not read it.

---

## 1. On fixing

### R1 — A fix is never blocked by a test, a gate, a pin, or a ceremony

Fix first; verification follows. A test that fails because a fix changed the behaviour it pinned was **pinning the
defect**: it is updated as part of the fix. Never defer a correctness fix behind *"the test would need updating"*,
*"the pin would move"*, *"that must land with X"* or *"that needs sign-off"*. Those are consequences of doing the
fix.

**The one boundary: never weaken a test to make a fix pass.** Deleting an assertion, widening a tolerance, or
skipping a case hides a regression rather than removing one. The distinction is whether the **defect** changed the
test's subject, or the **convenience** did.

*Earned by three incidents in one session.* A host-side defect that silently corrupts a contract's memory for any
payload over 1 MiB was deferred behind a "genesis pin move" — a routine, recorded procedure the repository has
performed repeatedly and which that fix did not even touch. Three tests asserting that banning a peer for an
oversized frame is correct were priced as a *cost* of fixing the ban taxonomy rather than as the change itself.
And a batch test's honesty fix was sequenced around a test-ordering constraint. In each case the deferral was
dressed as diligence.

*A tree where a failing or inconvenient test can stop a correctness fix accumulates the defects it cannot name.*

### R2 — Less is more: remove the root cause; do not add a compensating guard

Every new guard is new footgun surface. If a constant is unjustified, its absence needs no replacement. If a check
cannot fail, delete or fix it — do not add a second check beside it.

*Earned by:* a `HAZOP M-14` gas charge introduced to close a gas-accounting item, which charged a contract for
memory growth the **host** performed and whose only lever was to refuse an honest call — the item's real answer was
that the number it was protecting was unreadable. And by nine invented checks being replaced by two mechanisms.

### R3 — Removing a guard is a change like any other

Name what it protected, on which path, and what now covers that obligation.

*Earned by:* the miner's template byte budget, derived from the invented `MAX_BLOCK_SIZE`. Deleting the number
deleted the *obligation* with it — "the miner must not build what the transport will drop" — and nothing replaced
it, so a miner can now assemble a block that no peer will accept, and the peer is banned rather than the block
declined.

### R4 — Assess before applying

State the blast radius, quantify before/after, and say what the change **composes with**. Two changes made in one
session can be two halves of one change, and neither commit message will say so.

*Earned by:* a frame bound raised 8× in one commit and the gas charge that priced that growth removed in another;
the per-call host allocation ceiling rose 8× *and* the metering that discouraged it was removed, which neither
message states.

## 2. On evidence

### R5 — A fix is verified by a run, never by a gate exit code, a typecheck, or a clean diffstat

Quote the run's own line.

*Earned by:* every bound changed in a single commit, none of them witnessed by a run — the stated verification was
`cargo check` plus a gate that only inspects artifacts.

### R6 — An audit, a HAZOP, a subagent summary and a code comment are hypotheses, not facts

Read the code before acting on any of them. Names in this tree lie: a field called `fn_code` holds a call-tree
length prefix; a target called `check-zkas-version` checks nothing; `BoundaryCodec::MAX_BYTES` has twelve
implementations and zero readers; `MAX_CALLS_PER_BLOCK` is documented and does not exist.

*Earned by:* four audit claims corrected by reading the code in one session — including one whose recommended
"fix" would have broken every contract build — and by a confident root-cause claim of the author's own that the
code refuted three times.

### R7 — State what you did not establish

"Cannot verify" is a result. A confident wrong root cause is worse than an admitted unknown, because it gets built
on.

### R8 — A bound is not a bound until something reads it, and a check is not a check until something can make it fail

Every check ships a **negative control**: run it against a planted defect and require a non-zero exit. A static
scan cannot find a check that cannot fail — measured: it found 2 of 16.

*Earned by:* sixteen instruments that could not fail, including one the umbrella counted as green while it printed
FAIL, and one whose pattern was structurally unmatchable (a bare `|` inside a BRE group, where `|` is a literal
character).

## 3. On authority

### R9 — Nothing is invented

Every constant, bound, threshold or figure cites a measurement or a clause, and the citation must resolve to
something that exists. A round number inherited from nowhere is a defect.

*Earned by:* `MAX_BLOCK_SIZE = 4 * 1024 * 1024`, whose doc comment cited "single source of truth pinned across
nodes (**L1 barrier #7**)" — a barrier that appears in no document in this repository, the list having been deleted
in September 2026. Its value came from a commit message reading only "Add 4 MB size cap on block decode", and it
was enforced as block validity, where it rejected a legitimate contract-deployment block and reported a block 2.2×
over its limit as being "within 1%" of it. Eight further dead citations were later found behind live gates.

### R10 — A policy refusal is not a validity verdict

A bound that *rejects* data is a consensus rule: it needs a clause and a derivation. A node-local resource policy
may drop, throttle, defer or refuse to serve — it must never declare data invalid.

*Earned by:* a DoS gate used to reject blocks; and a transport where `Error::MessageInvalid` answers "malformed?",
"too large?" and "unknown?" simultaneously while reputation is keyed on it, so six ban sites carry three different
semantics and an honest-but-large peer is blacklisted.

### R11 — Never leave a false statement in the tree, including in your own fix's comments

Correct it where it is **load-bearing** — the normative document, the spec-side description — not where you
happened to be reading.

*Earned by:* seven false claims written in one session, of which three were corrected in one location while an
identical copy stood elsewhere: "gas is enforced at every height including genesis" survived in `consensus.md`
after being deleted from the code comment; `Blocks=16MiB` survived in a module doc after the constant moved to
32 MiB; and a model asserted `MAX_INBOUND_PAYLOAD == 4 MiB` against its own literal while the code said 32 MiB.

---

## Applying these to a HAZOP

A HAZOP is analysis: root causes only, structure before patches, and **no implementation** until the findings are
synthesised and approved. Apply the guide words **NO/NOT · MORE · LESS · AS WELL AS · PART OF · REVERSE · OTHER
THAN** to every parameter — each constant, bound, rail, phase and guard — and mark a cell *N/A with a reason*
rather than skipping it, because a missing cell is itself a finding.

Fill *"why the safeguard did not fire"* as well as *"what happened"*. The column that produces doctrine is the
last one: **"what rule would have prevented this being introduced"**. A HAZOP that stops at symptoms produces a bug
list; the last column produces rules like the eleven above.
