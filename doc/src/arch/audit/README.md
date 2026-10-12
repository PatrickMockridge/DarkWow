# Security Audit Documents

The 2026-07-31 audit corpus lived here: the independent Red Team audit (47 findings), its HAZOP
root-cause analysis (9 families, 6 structural changes), the Comprehensive Security Audit
(~314 findings, independent methodology), the two L1 capability write-path HAZOPs, and the
Genesis & Consensus adversarial audit.

**They were removed on 2026-09-22.** A resolved finding is a lesson, and lessons now live as root
causes: `RC1`–`RC12` in [Contract Safety](../../dev/contracts/safety.md). The items these documents
left **open** are rows in the [Verification Obligation Register](../verification-hazop.md), which is
now the only place an open obligation is recorded. Git history holds the full texts.

**What the alias table maps, and what it does not.** This paragraph read "every legacy ID — red-team
`C-/H-/M-/L-/SC-`, `RC-A`–`RC-I`, the two colliding `RC1–RC6` schemes — mapped in the alias table at
that document's foot", and that is wider than the table. The table maps Lessons 1–25, the July-2026
`RC1`–`RC5`, red-team `RC-A`–`RC-I`, the consensus HAZID's `RC1`–`RC6`, and the Lean HAZOP's
`pattern1`–`pattern7`. It does **not** map the red-team's per-finding `C-`/`H-`/`M-`/`L-`/`SC-`
identifiers, and no document now cites one — they died with their texts. It also does not map the
`sync-red-team-audit.md` `R1`–`R10` in this directory, which is a *different*, later document and
still present. The safety document states the same limit from its own side: the register's `V1`–`V7`
and `C1` schemes have no mapping there either.

## What the corpus is worth remembering

**Two independent audits disagreed on three points. The record of how they were settled is wrong,
and the correction is more useful than the claim.**

This section read "Two independent audits contradicted each other, and the more specific one was
right … File:line verification beat breadth on all three", and named three disagreements: TLS TOFU
pinning, `SecretKey`'s `Debug` impl, and chain-work recomputation. What actually settled them is a
**code change, not a reading**. All three were HAZOP findings, and the fixes landed in one commit —
`735e1e0bca`, "HAZOP security remediation — 15 findings", dated **2026-07-31 15:55**, the same day as
the audit corpus itself. So the second audit was not more specific and did not win: it looked at code
that had already moved, and the "contradiction" is between a snapshot and a repair. Naming that is
worth more than the flattering version, because the flattering version teaches a reader to trust one
audit's method over another's when the real lesson is to date the code.

The generalisation stands, with that caveat attached: **a broad sweep surfaces what a targeted audit
never looks at, and a targeted audit resolves what a broad sweep can only assert.** When two disagree,
read the code — and check whether it changed between them.

One finding was withdrawn rather than resolved: the block-size gate was measured against JSON, not
the binary encoding it was meant to bound, so the finding's premise was wrong rather than the code.
That is a third failure mode worth naming beside *right* and *wrong* — an audit claim can be
unfalsifiable as stated. Withdrawn items are recorded as such there, and here.
