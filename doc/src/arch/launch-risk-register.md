# Launch risk register — retired 2026-10-22

This was a second ledger over [`verification-hazop.md`](verification-hazop.md). It stored no
obligation status (it read the other file's statuses live, which kept it honest) but it was still a
second document to maintain, and its gate policed drift between two documents rather than the code.

It is retired. Every live defect it ranked is a row in the tracker, and the four decisions of record
(`LR-D*`) are in [`decisions.md`](decisions.md). `scripts/check-launch-register.sh` is deleted;
`scripts/check-defect-register.sh` is the tracker's only gate.

## Release checklist

There is no status table here. A release is clear only when:

1. no tracker row is in the **Intolerable** band (`risk-management.md` §4); every Tolerable row is
   either `CLOSED` or has a signed ALARP record in `residual-risk.md`; the batch approval
   (`reports/register-approval.txt`) names the accepted ids and its `scope_hash` matches the
   tracker, residual register and policy; and
2. the build and pin evidence the release requires exists in the run that produced it: two clean
   byte-identical builds of the changed contracts, 9/9 genesis purity, 32/32 freshness, and the pin
   recorded once from the genesis path's own computed line with its positive and wrong-hash controls.

The checklist names evidence, never a document's opinion of itself. A release that accepts an
Intolerable risk is not a release; it is a recorded exception that needs an explicit authority and a
new approval, not a line in this checklist.
