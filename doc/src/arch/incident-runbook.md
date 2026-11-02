# Incident runbook

What to do when a defect is suspected in a running node, wallet, contract or the verification
pipeline. The [disclosure policy](disclosure-policy.md) covers reports from outside; this page covers
the project's own response.

The project lead owns the incident, the tracker row and the release decision. No one declares an
incident resolved without a failing-before control and a published record.

## First actions

1. **Preserve evidence before changing anything.** Copy the logs and the database; do not restart,
   re-run or clean the affected process.
2. **Do not delete data to start fresh.** The Monero testnet data directory is an 8-12 hour sync;
   the chain and contract state are evidence.
3. **Do not reconstruct the failure from memory.** Read the recorded output verbatim; a confident
   wrong account is worse than "not established".
4. **Open or reopen the tracker row** and state what is known and what is excluded. While any row is
   undecided, [`scripts/check-tracker-settled.sh`](../../../scripts/check-tracker-settled.sh)
   refuses every verification surface. That is the correct state, not a queue to bypass.
5. **Classify severity** with [`risk-management.md`](risk-management.md) §2-4. An Intolerable row
   blocks a release; it cannot become a residual without an ALARP record and the approval in §7.

## Evidence to capture

- The full output of the failing command, not a tail. Run tests with `DWOW_TEST_LOGS=1` so a
  contract's own messages are not discarded. The PoW path adds `DWOW_POW_FORENSIC=1`, which rehashes
  a rejected block with a fresh VM and prints the validator's key and blob digest.
- `/tmp/hw_*.log`, `reports/ci/`, the CI job's artifacts, and the pipeline's own phase logs.
- The commit, the toolchain, the environment (memory, swap, `RAYON_NUM_THREADS`, `RUST_MIN_STACK`)
  and the exact command line.
- For a block rejection: height, target, nonce, the validator's key and blob digest, and the
  fresh-VM comparison from the forensic path. `OBL-C279` is the worked example.

## Playbooks

### Consensus divergence or a PoW rejection

Stop the affected node; do not let it mine. Capture the block, the header and the validator's log.
Re-run the named control (not a narrowed selection) on the same commit. If the rejection does not
reproduce, record that as evidence and keep the row undecided: repetition is not a root cause. The
`nightly-pow` lane repeats the one test that once showed a rejection, with the forensic instrument
on.

### Genesis pin mismatch

Do not release. Rebuild the changed contracts clean, twice, and require byte-identical artifacts.
Re-derive the pin from the genesis path's own computed line; never hand-edit the pin. Run the
positive and wrong-hash controls. The genesis ceremony is not a test fixture: never mock or shortcut
it.

### Key compromise

Assume the compromised key controls what it controlled. Move value to a fresh wallet. Only wallet
forwarding keys may be copied or rotated; mining keys are never copied or exported. Revoke what the
key authorized and record what cannot be revoked.

### A contract value path

Fail closed: disable the arm with a named error before any state read, as the custody and authority
arms do (`CustodyModelUnavailable`, `AuthorityModelUnavailable`). Add the endpoint-named control. Do
not weaken a test or route around the contract to make a pipeline pass.

### CI or runner compromise

Stop the self-hosted runner's service, remove it from the repository, rotate its token, and audit
every change to `.github/workflows/` and the branch-protection settings. A workflow on a compromised
runner can read the workspace and the caches; treat the caches as suspect.

### Data loss or corruption

Re-sync from genesis. Never delete the Monero testnet data to force a clean start. If the contract or
supply trees are inconsistent, the disconnect path is the repair (OBL-C269); do not hand-edit
consensus state.

## Recovery

A fix is complete only when the nine conditions in [`hazop-rules.md`](../dev/hazop-rules.md) hold: the
defect is impossible in current code, a failing-before control exists and is wired, the run is
quoted, no trade is introduced, the records describe the code, artifacts and pins are rebuilt, and
the tracker gate accepts the closure. If the risk is accepted instead, it moves to
[`residual-risk.md`](residual-risk.md) with an ALARP record and the lead's approval.

Then run the release checklist: no Intolerable row; every Tolerable row closed or residual; the
approval names the accepted ids and its hash matches; two clean builds, 9/9 genesis purity, 32/32
freshness, and the pin recorded from the genesis path with its positive and wrong-hash controls.

## Post-incident

Write the record while the evidence is fresh: root cause, the controls that failed, the fix, the
records corrected, and the residual if any. A new instrument or gate is not a closure by itself; it
is closure only when it can fail on the defect. If the incident exposed a gap in this runbook,
change this page in the same commit as the fix.
