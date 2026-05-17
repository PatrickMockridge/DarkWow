#!/usr/bin/env python3
"""Run all failure mode scenarios and produce a remediation report.

Usage:
    python3 sim/run_all.py              # Run all scenarios, output report.md
    python3 sim/run_all.py --scenario crash  # Run a single scenario
    python3 sim/run_all.py --list       # List available scenarios

Output:
    sim/report.md                   # Full remediation and hardening report
"""

import importlib
import json
import os
import sys
import time
from pathlib import Path

# Ensure we can import the sim package
_SIM_DIR = Path(__file__).parent
_REPO_ROOT = _SIM_DIR.parent
sys.path.insert(0, str(_REPO_ROOT))

from sim.config import SimConfig
from sim.scenarios.base import ScenarioResult

SCENARIO_DIR = _SIM_DIR / "scenarios"
REPORT_PATH = _SIM_DIR / "report.md"

SCENARIO_MODULES = [
    "crash",
    "exhaustion",
    "evasion",
    "theft",
    "slash_loop",
    "bank_run",
    "partition",
    "fee_manipulation",
    "pool_tragedy",
    "htlc_race",
]

SCENARIO_CLASSES = {
    "crash": "RelayerCrashScenario",
    "exhaustion": "CapitalExhaustionScenario",
    "evasion": "FeeSettlementEvasionScenario",
    "theft": "MaliciousRelayerTheftScenario",
    "slash_loop": "SlashLoopScenario",
    "bank_run": "BackerBankRunScenario",
    "partition": "NetworkPartitionScenario",
    "fee_manipulation": "FeeManipulationScenario",
    "pool_tragedy": "PoolTragedyScenario",
    "htlc_race": "HtlcRaceScenario",
}


def load_scenario(name: str):
    """Dynamically load a scenario class."""
    module_name = f"sim.scenarios.{name}"
    class_name = SCENARIO_CLASSES[name]
    module = importlib.import_module(module_name)
    return getattr(module, class_name)


def run_all_scenarios(modules: list[str] | None = None) -> list[ScenarioResult]:
    """Run all scenarios and return results."""
    results = []
    to_run = modules if modules is not None else SCENARIO_MODULES
    total = len(to_run)

    for i, name in enumerate(to_run, 1):
        print(f"[{i}/{total}] Running {name}...", end=" ", flush=True)
        try:
            scenario_cls = load_scenario(name)
            scenario = scenario_cls()
            start = time.time()
            result = scenario.run()
            elapsed = time.time() - start

            status = "PASS" if result.passed else "FAIL"
            n_failures = len(result.failure_modes_found)
            print(f"{status} ({n_failures} failures, {elapsed:.1f}s)")

            # Save detailed results
            out_prefix = str(_SIM_DIR / f"results/{name}")
            os.makedirs(str(_SIM_DIR / "results"), exist_ok=True)
            scenario.save_results(out_prefix)

            results.append(result)
        except Exception as e:
            print(f"ERROR: {e}")
            import traceback
            traceback.print_exc()
            result = ScenarioResult(name=name, description="Error during execution")
            result.failure_modes_found = [f"Simulation error: {e}"]
            results.append(result)

    return results


def severity_from_text(text: str) -> str:
    """Extract severity from failure mode text."""
    if text.startswith("CRITICAL"):
        return "CRITICAL"
    elif text.startswith("HIGH"):
        return "HIGH"
    elif text.startswith("MEDIUM"):
        return "MEDIUM"
    return "LOW"


def generate_report(results: list[ScenarioResult]) -> str:
    """Generate a comprehensive remediation report from scenario results."""
    lines = []
    lines.append("# DarkWow Relayer Network — Operational Robustness Report")
    lines.append("")
    lines.append("## Executive Summary")
    lines.append("")

    total_failures = sum(len(r.failure_modes_found) for r in results)
    critical_count = sum(
        1 for r in results
        for f in r.failure_modes_found if f.startswith("CRITICAL")
    )
    high_count = sum(
        1 for r in results
        for f in r.failure_modes_found if f.startswith("HIGH")
    )
    medium_count = sum(
        1 for r in results
        for f in r.failure_modes_found if f.startswith("MEDIUM")
    )

    lines.append(f"**Total failure modes found: {total_failures}** "
                 f"({critical_count} CRITICAL, {high_count} HIGH, {medium_count} MEDIUM)")
    lines.append("")
    lines.append("10 adversarial and degraded-condition scenarios were simulated against "
                 "the DarkWow bridge + relayer_endowment + universal_relayer architecture. "
                 "The simulation modeled block-by-block chain progression, stake coverage, "
                 "fee settlement, and backer capital flows.")
    lines.append("")

    # Build failure catalog
    lines.append("---")
    lines.append("")
    lines.append("## Failure Mode Catalog")
    lines.append("")

    all_failures = []
    for r in results:
        for f in r.failure_modes_found:
            all_failures.append({
                "severity": severity_from_text(f),
                "description": f,
                "scenario": r.name,
            })

    # Sort by severity
    severity_order = {"CRITICAL": 0, "HIGH": 1, "MEDIUM": 2, "LOW": 3}
    all_failures.sort(key=lambda x: severity_order.get(x["severity"], 99))

    lines.append("| # | Severity | Scenario | Description |")
    lines.append("|---|----------|----------|-------------|")
    for i, f in enumerate(all_failures, 1):
        desc = f["description"][:120] + ("..." if len(f["description"]) > 120 else "")
        lines.append(f"| {i} | **{f['severity']}** | {f['scenario']} | {desc} |")

    lines.append("")
    lines.append("---")
    lines.append("")
    lines.append("## Remediation Recommendations")
    lines.append("")

    # Contract-level fixes
    lines.append("### 1. Contract-Level Changes")
    lines.append("")
    lines.append("#### 1.1 Automatic Fee Settlement (CRITICAL)")
    lines.append("- **Problem**: Relayers can earn fees without ever calling SettleFeesV1. "
                 "Backers have zero on-chain recourse.")
    lines.append("- **Fix**: Add `last_settlement_height` to EndowmentAccount. If "
                 "`current_height - last_settlement_height > SETTLEMENT_TIMEOUT`, "
                 "backers can call `ForceSettleV1` that computes pro-rata fee shares "
                 "from the bridge contract's total collected fees for that relayer.")
    lines.append("- **Contract**: `relayer_endowment` — new function `ForceSettleV1`")
    lines.append("")
    lines.append("#### 1.2 Withdrawal Reassignment (CRITICAL)")
    lines.append("- **Problem**: Withdrawals accepted by a crashed/partitioned relayer "
                 "stay stuck until timeout. No reassignment mechanism.")
    lines.append("- **Fix**: Add `reassign_after_blocks` field to PendingWithdrawal. "
                 "If a relayer accepts a withdrawal but doesn't execute within N blocks, "
                 "the withdrawal becomes available for other relayers. The original "
                 "relayer's locked stake is partially slashed for the delay.")
    lines.append("- **Contract**: `bridge` — modify WithdrawalRecord, add reassignment logic")
    lines.append("")
    lines.append("#### 1.3 Dynamic Fee Caps (HIGH)")
    lines.append("- **Problem**: No upper bound on relayer fees. Monopoly relayer can "
                 "charge extortionate rates.")
    lines.append("- **Fix**: Add `max_fee_bp` constant to bridge contract (e.g., 1000 = 10%). "
                 "Withdrawal execution validates that `fee <= amount * max_fee_bp / 10000`. "
                 "Alternatively, let users specify `max_fee` in withdrawal request.")
    lines.append("- **Contract**: `bridge` — add validation to execute_withdrawal")
    lines.append("")
    lines.append("#### 1.4 Proportional Slashing (HIGH)")
    lines.append("- **Problem**: Slash amount is a flat constant (`1_000_000`), regardless "
                 "of withdrawal size. Large guaranteed withdrawals are under-protected.")
    lines.append("- **Fix**: Change slash amount to be proportional: "
                 "`slash = max(MIN_SLASH, amount * slash_bp / 10000)`. "
                 "This ensures the penalty scales with the risk.")
    lines.append("- **Contract**: `bridge` — replace `SLASH_AMOUNT` constant with "
                 "proportional formula")
    lines.append("")
    lines.append("#### 1.5 HTLC State Machine Atomicity (CRITICAL)")
    lines.append("- **Problem**: Claim and refund can both succeed on the same HTLC "
                 "if they arrive in the same block — funds can be doubled.")
    lines.append("- **Fix**: Enforce strict state transitions: claim only valid if "
                 "`status == Pending`, refund only valid if `status == Pending AND "
                 "block_height >= time_lock`. Use `Option<BlockHeight>` for both "
                 "`claimed_at` and `refunded_at` with mutual exclusion check in "
                 "process_update.")
    lines.append("- **Contract**: `bridge` — fix `claim_htlc` and `refund_htlc` logic")
    lines.append("")

    # Relayer-level fixes
    lines.append("### 2. Relayer-Level Changes")
    lines.append("")
    lines.append("#### 2.1 Health Check and Auto-Recovery (HIGH)")
    lines.append("- **Problem**: Relayer crash causes permanent offline until manual restart.")
    lines.append("- **Fix**: Add watchdog process that monitors relayer health and restarts "
                 "on failure. Add graceful shutdown that completes in-flight withdrawals "
                 "before stopping.")
    lines.append("- **Files**: `bin/universal_relayer/` — add health check module")
    lines.append("")
    lines.append("#### 2.2 Withdrawal Handoff Protocol (MEDIUM)")
    lines.append("- **Problem**: No coordination between relayers. Withdrawals are "
                 "picked up by first available relayer with no load balancing.")
    lines.append("- **Fix**: Implement a lightweight handoff protocol: after accepting "
                 "a withdrawal, publish a signed heartbeat every N blocks. If heartbeat "
                 "stops, other relayers can claim the withdrawal after a grace period.")
    lines.append("- **Files**: `bin/universal_relayer/` — add handoff module")
    lines.append("")
    lines.append("#### 2.3 Fee Discovery Endpoint (MEDIUM)")
    lines.append("- **Problem**: Users cannot discover relayer fees before committing "
                 "to a withdrawal.")
    lines.append("- **Fix**: Add a JSON-RPC endpoint on the relayer that returns current "
                 "fee schedule. Wallet UI can query multiple relayers and present options.")
    lines.append("- **Files**: `bin/universal_relayer/` — add RPC endpoint")
    lines.append("")
    lines.append("#### 2.4 Pool Reputation Tracking (MEDIUM)")
    lines.append("- **Problem**: Shared pools have no per-member accountability. One "
                 "reckless member degrades the entire pool.")
    lines.append("- **Fix**: Track per-member slash history in PoolManager. Members with "
                 "high slash rates are automatically ejected. Pool stake allocation is "
                 "proportional to reputation score.")
    lines.append("- **Files**: `bin/universal_relayer/src/pool.rs`")
    lines.append("")

    # Protocol-level fixes
    lines.append("### 3. Protocol-Level Changes")
    lines.append("")
    lines.append("#### 3.1 Circuit Breaker for Stake Exhaustion (CRITICAL)")
    lines.append("- **Problem**: When relayer stake is fully slashed, guaranteed "
                 "withdrawals have zero protection but users still pay premium.")
    lines.append("- **Fix**: Bridge contract rejects new guaranteed withdrawals if "
                 "relayer's available stake is below `MIN_GUARANTEED_COVERAGE_RATIO`. "
                 "Users must use standard withdrawals instead.")
    lines.append("- **Contract**: `bridge` — add coverage check in process_withdraw_instruction")
    lines.append("")
    lines.append("#### 3.2 Gradual Stake Unlocking (MEDIUM)")
    lines.append("- **Problem**: Stake is released immediately on withdrawal execution. "
                 "If external chain later reorgs, the relayer has no skin in the game.")
    lines.append("- **Fix**: Lock stake for N confirmations after execution before "
                 "releasing. N depends on external chain finality (e.g., 12 blocks ETH, "
                 "10 blocks XMR).")
    lines.append("- **Files**: `bin/universal_relayer/src/stake.rs`")
    lines.append("")
    lines.append("#### 3.3 Backer-Initiated Settlement (HIGH)")
    lines.append("- **Problem**: No way for backers to discover how many fees a relayer "
                 "has earned. Information asymmetry enables evasion.")
    lines.append("- **Fix**: Bridge contract emits `FeesEarned` events keyed by relayer. "
                 "Backers can query these events to detect evasion. Endowment contract "
                 "adds `report_unsettled_fees` function that backers can call.")
    lines.append("- **Contracts**: `bridge` + `relayer_endowment`")
    lines.append("")

    # Per-scenario details
    lines.append("---")
    lines.append("")
    lines.append("## Detailed Scenario Results")
    lines.append("")

    for r in results:
        passed_str = "PASSED" if r.passed else "FAILED"
        lines.append(f"### {r.name} — {passed_str}")
        lines.append("")
        lines.append(f"**Description**: {r.description}")
        lines.append("")
        lines.append("**Key Metrics**:")
        lines.append("")
        m = r.metrics
        if m:
            lines.append(f"- Withdrawal success rate: {m.get('withdrawal_success_rate', 'N/A')}")
            lines.append(f"- Withdrawals executed: {m.get('total_withdrawals_executed', 0)}")
            lines.append(f"- Withdrawals failed: {m.get('total_withdrawals_failed', 0)}")
            lines.append(f"- Withdrawals slashed: {m.get('total_withdrawals_slashed', 0)}")
            lines.append(f"- Withdrawals cancelled: {m.get('total_withdrawals_cancelled', 0)}")
            lines.append(f"- Avg withdrawal latency: {m.get('avg_withdrawal_latency_blocks', 'N/A')} blocks")
            lines.append(f"- Total fees settled: {m.get('total_fees_settled', 0)}")
            lines.append(f"- Settlement events: {m.get('total_settlement_events', 0)}")
            lines.append(f"- Stake slashed: {m.get('total_stake_slashed', 0)}")
            lines.append(f"- Slash events: {m.get('slash_events', 0)}")
            lines.append(f"- Avg backer ROI: {m.get('avg_backer_roi', 'N/A')}")
            lines.append(f"- Capital deployed: {m.get('total_capital_deployed', 0)}")
        lines.append("")

        if r.failure_modes_found:
            lines.append("**Failure Modes Found**:")
            for fm in r.failure_modes_found:
                lines.append(f"- {fm}")
        else:
            lines.append("**No failure modes found.**")
        lines.append("")

    lines.append("---")
    lines.append("")
    lines.append("## Simulation Configuration")
    lines.append("")
    lines.append("| Parameter | Value |")
    lines.append("|-----------|-------|")
    lines.append(f"| Scenarios run | {len(results)} |")
    lines.append(f"| Blocks per scenario | 400-800 |")
    lines.append("| Block time | 30 seconds |")
    lines.append("| Withdrawal timeout | 100 blocks |")
    lines.append("| Min deployment | 1,000,000 (1 DAI equivalent) |")
    lines.append("| Standard fee | 1% |")
    lines.append("| Guaranteed premium | 5% |")
    lines.append("| Slash amount | 1,000,000 (1 DAI equivalent) |")
    lines.append("| Stake coverage ratio | 1.5x |")
    lines.append("")

    lines.append("---")
    lines.append("")
    lines.append("## Next Steps")
    lines.append("")
    lines.append("1. **Immediate (CRITICAL)**: Fix HTLC state machine atomicity (1.5), "
                 "add automatic fee settlement (1.1), and implement circuit breaker "
                 "for stake exhaustion (3.1)")
    lines.append("2. **Short-term (HIGH)**: Add withdrawal reassignment (1.2), "
                 "proportional slashing (1.4), fee caps (1.3), and backer-initiated "
                 "settlement (3.3)")
    lines.append("3. **Medium-term (MEDIUM)**: Implement relayer handoff protocol (2.2), "
                 "fee discovery (2.3), pool reputation (2.4), health checks (2.1), "
                 "and gradual stake unlocking (3.2)")
    lines.append("")

    return "\n".join(lines)


def main():
    modules_to_run = list(SCENARIO_MODULES)

    if "--list" in sys.argv:
        print("Available scenarios:")
        for name in SCENARIO_MODULES:
            print(f"  {name}")
        return

    if "--scenario" in sys.argv:
        idx = sys.argv.index("--scenario")
        name = sys.argv[idx + 1]
        if name not in SCENARIO_MODULES:
            print(f"Unknown scenario: {name}")
            print(f"Available: {', '.join(SCENARIO_MODULES)}")
            sys.exit(1)
        modules_to_run = [name]

    print("=" * 60)
    print("DarkWow Relayer Network — Operational Robustness Simulation")
    print("=" * 60)
    print()

    results = run_all_scenarios(modules_to_run)

    # Generate and write report
    report = generate_report(results)
    REPORT_PATH.write_text(report)
    print()
    print(f"Report written to {REPORT_PATH}")

    # Summary
    total = sum(len(r.failure_modes_found) for r in results)
    if total == 0:
        print("\nAll scenarios passed. No failure modes detected.")
    else:
        critical = sum(
            1 for r in results
            for f in r.failure_modes_found if f.startswith("CRITICAL")
        )
        high = sum(
            1 for r in results
            for f in r.failure_modes_found if f.startswith("HIGH")
        )
        print(f"\n{total} failure modes found: {critical} CRITICAL, {high} HIGH")
        print(f"See {REPORT_PATH} for full remediation plan.")


if __name__ == "__main__":
    main()
