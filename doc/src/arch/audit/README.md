# Security Audit Documents

Three independent audit documents were produced on 2026-07-31. They are preserved here as historical snapshots. Current status of all findings is maintained in [safety.md](../../dev/contracts/safety.md).

## Document Relationship

```
RED_TEAM_AUDIT (47 findings)
    │
    ▼
HAZOP_ROOT_CAUSE (9 root cause families, 6 structural changes)
    
SECURITY_AUDIT (~314 findings, independent methodology)
```

1. **[Red Team Findings](red-team-findings.md)** — Independent adversarial audit. 47 findings (11 CRITICAL, 16 HIGH, 15 MEDIUM, 5 LOW). Every finding verified against source code with exact file paths and line numbers. **Highest-confidence source.**

2. **[Red Team HAZOP Analysis](red-team-hazop-analysis.md)** — Root cause analysis of the 47 Red Team findings. Groups into 9 root cause families (RC-A through RC-I). Proposes 6 structural changes (SC-1 through SC-6). Includes implementation priority matrix: 46 fixes across 5 tiers.

3. **[Comprehensive Security Audit](comprehensive-security-audit.md)** — Broader audit: ~314 findings across 7 subsystems, 12 parallel agents. Maps against 23 safety.md lessons and 5 HAZOP root causes. **Independent methodology from the Red Team audit** — some findings contradict the Red Team audit (see below).

## Known Contradictions

| Contradiction | Red Team | Security Audit | Resolution |
|---------------|----------|---------------|------------|
| TLS TOFU pinning | IMPLEMENTED at `tls.rs:156-173` | H1: missing, MITM-able | **Red Team correct.** Blake3 fingerprint comparison with rejection on mismatch |
| SecretKey Debug | FIXED — `<redacted>` at `keypair.rs:91-95` | C14: leaks full key material | **Red Team correct on Debug.** Security Audit valid on Display (base58 leak by design for CLI export) |
| Chain work recomputation | FIXED at `chain_state.rs:168-196` | H7: not recomputed | **Red Team correct.** Full recompute on startup with sled-cache validation |

These contradictions exist because the two audits used independent methodologies on the same codebase within the same 24-hour period. The Red Team audit's file:line verification provides higher confidence for specific claims; the Security Audit's broader sweep caught issues the Red Team did not examine.

## Current Status

As of 2026-08-03, 31+ remediation commits have been applied since these documents were created. See [safety.md](../../dev/contracts/safety.md) for per-finding verified status.
