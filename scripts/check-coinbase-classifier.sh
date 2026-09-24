#!/usr/bin/env bash
# Gate: the coinbase is classified by the SHARED classifier, never by the selector-only probe.
#
# `Transaction::first_call_is_pow_reward` matches `data[0] == 0x05` against ANY contract — its own
# docstring says so ("regardless of contract id … the structural variant used where the block-structure
# rule is checked separately from contract identity"). Around twenty contracts use 0x05 as a real
# function code: IdentityFunction::IssueCapabilityV1, AttestationFunction::ConsumeClaimV1,
# dex/relayer_endowment UpdateConfigV1, escrow CancelV1, auction RefundBidV1, stablecoin RepayStableV1,
# promissory_note OtcSwapV1, drain_protection TransferV1, daos_escrow TreasurySpendV1, and others.
#
# So the probe answers "coinbase" for transactions that are not the coinbase. Where an exemption is
# *skipping a check* that is a soundness bug: until 2026-09-24 the L2 witness loop in
# `src/linear/src/execution.rs` exempted such a transaction from the only proof verification the accept
# path performs, so a fabricated proof rode in. Where the exemption is *rejecting* (the RPC predicates)
# it is over-rejection instead. Both directions are defects, and both are the same root cause.
#
# `Transaction::is_pow_reward_coinbase_tx` — native token contract AND PoWRewardV1 0x05 — is the
# classifier every accept-path site must use. `chain_state.rs::check_coinbase_maturity` states the rule
# in its own docstring: "through the shared classifier … rather than a re-derived predicate".
#
# This gate fails on a *use* of the probe anywhere outside its definition, unless the use is
# position-gated on the line or the two lines above it (`tx_idx == 0`, `.first()`, `transactions[0]`).
# A position gate is the correct form for a site that needs the structural variant, because it makes the
# exemption state the whole classification — position *and* content — so it stays correct even for a
# caller that has not run `validate_block_structure` first.
#
# It is deliberately a source gate rather than a behavioural test: reproducing the bypass end-to-end
# needs a deployed non-native contract whose 0x05 function can be called with a fabricated proof, and the
# fixtures in this tree top out at 0x04 (promissory_note TransferV1). Recorded rather than left implicit —
# the probe's permitted uses below are the whole of what the gate allows.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PROBE='first_call_is_pow_reward'
DEFINITION_SITE='src/linear/src/transaction.rs'
GATE_RE='tx_idx[[:space:]]*==[[:space:]]*0|\.first\(\)|transactions\[0\]'

fail=0
checked=0

# Every .rs file, the probe's definition excluded (the gate's subject is a *use*).
while IFS= read -r file; do
    [ "$file" = "$DEFINITION_SITE" ] && continue
    # Line numbers of the probe's uses in this file.
    while IFS= read -r line_no; do
        [ -z "$line_no" ] && continue
        line="$(sed -n "${line_no}p" "$file")"

        # Comment-only mentions are prose, not uses: `//`, `///`, `//!`, and `*` continuation lines.
        trimmed="${line#"${line%%[![:space:]]*}"}"
        case "$trimmed" in
            //*|/\**|\**) continue ;;
        esac

        checked=$((checked + 1))

        # The position gate may be on the use's own line or within the two lines above it (the
        # builder-chain form `.transactions` / `.first()` / `.filter(...)`).
        window="$(sed -n "$((line_no > 2 ? line_no - 2 : 1)),${line_no}p" "$file")"
        if ! printf '%s' "$window" | grep -qE "$GATE_RE"; then
            echo "FAIL: $file:$line_no uses the selector-only coinbase probe with no position gate:"
            echo "        $trimmed"
            echo "      Use Transaction::is_pow_reward_coinbase_tx (native token AND 0x05), or gate this"
            echo "      use on position. See the header of this script for why."
            fail=1
        fi
    done < <(grep -n "$PROBE" "$file" | cut -d: -f1)
done < <(find src bin -name '*.rs' -not -path '*/target/*' 2>/dev/null | sort)

if [ "$fail" -ne 0 ]; then
    echo
    echo "FAIL: a site classifies a coinbase with the selector-only probe (see above)."
    exit 1
fi

echo "OK: $checked use(s) of the coinbase probe are position-gated or prose; every accept-path"
echo "    exemption classifies with the shared classifier."
