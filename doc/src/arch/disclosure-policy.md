# Security disclosure policy

DarkWow is a proof-of-work blockchain with anonymous WASM contracts. A defect in consensus,
cryptography, a contract or the wallet can destroy value or privacy. Reports are welcome, and they
are handled privately until a fix exists.

This page is the disclosure policy named in [`risk-management.md`](risk-management.md) §10.7 as the
next bounded workstream. It states the project's commitments; it is not a bug bounty.

## Scope

In scope: the code in this repository — consensus and chain state (`src/linear/`), the zkVM and the
circuits (`src/zk/`, `proofs/`), the contracts (`src/contract/`), the SDK and serialization
(`src/sdk/`, `src/serial/`), the nodes and wallets (`bin/`), the transport and P2P stack, and the CI
definitions under `.github/`.

Out of scope: social engineering, physical access, spam, denial of service against a public
network, and defects that exist only in a third-party dependency. Report those upstream; if the
dependency is used here, the project will coordinate.

Known open items are recorded in the [defect tracker](verification-hazop.md). Do not report those
as new vulnerabilities; if you have evidence that one is worse than recorded, say so.

## How to report

Use the repository's **GitHub private vulnerability reporting** ("Report a vulnerability" on the
Security tab). Do not open a public issue, discussion or pull request for a suspected
vulnerability.

Include the affected commit or release, a reproduction (commands, a fixture, or a minimal test), the
impact you believe it has, and any logs. If a proof of concept is involved, state what it touches.

Do not test against other people's funds or data, a public network, or a running service you do not
control. Test locally or on a test network you run yourself. Do not run denial-of-service tests.

## What happens next

| Step | Target |
|---|---|
| Acknowledgment | within 72 hours |
| Initial triage and severity | within 7 days |
| Fix target | Critical/High: before the next release. Medium/Low: scheduled in the tracker. |
| Coordinated disclosure | 90 days after the report, or when a fix is released, whichever is earlier |

Severity uses the bands in [`risk-management.md`](risk-management.md) §2-4. While any tracker row is
undecided, every verification surface is blocked by
[`scripts/check-tracker-settled.sh`](../../../scripts/check-tracker-settled.sh). A confirmed report
becomes an `OBL-` row and is closed only with a code change and a failing-before control, or accepted
as a residual with an ALARP record.

If the report is valid, the project will credit you on release unless you ask to remain anonymous.
If it is not a defect, the project will say why and point at the code or the decision record.

## Safe harbor

Good-faith research that follows this policy is welcome. The project will not pursue legal action
for research that stays within scope and does not access, modify or disclose other users' funds,
keys or data. This is not a bug bounty: no payment is offered.

## Changes

This policy is a living document. A change lands in the same commit as the page and, when it changes
the disclosure boundary, is called out in the commit message.
