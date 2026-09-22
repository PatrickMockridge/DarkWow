# Security Audit Documents

The 2026-07-31 audit corpus lived here: the independent Red Team audit (47 findings), its HAZOP
root-cause analysis (9 families, 6 structural changes), the Comprehensive Security Audit
(~314 findings, independent methodology), the two L1 capability write-path HAZOPs, and the
Genesis & Consensus adversarial audit.

**They were removed on 2026-09-22.** A resolved finding is a lesson, and lessons now live as root
causes: `RC1`–`RC12` in [Contract Safety](../../dev/contracts/safety.md), with every legacy ID —
red-team `C-/H-/M-/L-/SC-`, `RC-A`–`RC-I`, the two colliding `RC1–RC6` schemes — mapped in the alias
table at that document's foot. The items these documents left **open** are rows in the
[Verification Obligation Register](../verification-hazop.md), which is now the only place an open
obligation is recorded. Git history holds the full texts.

## What the corpus is worth remembering

**Two independent audits contradicted each other, and the more specific one was right.** The Red
Team audit and the Comprehensive audit examined the same code within 24 hours by different
methodologies and disagreed on three points: TLS TOFU pinning (the Comprehensive audit called it
missing and MITM-able; it was implemented, with a Blake3 fingerprint comparison that rejects on
mismatch), `SecretKey`'s `Debug` impl (called a full key leak; it was already `<redacted>` — though
the Comprehensive audit was right that `Display` leaked, by design, for CLI export), and chain-work
recomputation (called absent; it recomputes in full on startup and validates against the sled cache).
File:line verification beat breadth on all three.

The generalisation is the reason both kinds of audit are worth running and why neither is
authoritative alone: **a broad sweep surfaces what a targeted audit never looks at, and a targeted
audit resolves what a broad sweep can only assert.** When they disagree, read the code — which is
what settled all three.

One finding was withdrawn rather than resolved: the block-size gate was measured against JSON, not
the binary encoding it was meant to bound, so the finding's premise was wrong rather than the code.
That is a third failure mode worth naming beside *right* and *wrong* — an audit claim can be
unfalsifiable as stated. Withdrawn items are recorded as such there, and here.
