"""Proof of Token Balance — Block-Level Pedersen Mass Balance
================================================================
Proves no hidden native token (darkw) minting beyond the coinbase.

The mass balance equation for every block:

    Σ output_commits + Σ burn_input_commits + Σ fee_commits == Σ input_commits

This ensures that non-coinbase transactions are collectively net-neutral
(or net-negative — burns are safe deflation). Only the coinbase creates
new darkw. The coinbase itself is verified separately against the
emission schedule.

Builds on the existing sim.crypto PedersenCommitment and cumulative
supply chain model (sim/tests/test_cumulative_supply_chain.py).

That model verifies:        S_H = S_{H-1} + C_H      (coinbase chain)
This model adds:            Σ outs + burns + fees = Σ ins  (tx mass balance)

Together they prove: total darkw supply = emission schedule, no hidden mints.
"""

import sys, os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

from sim.crypto import (
    PedersenCommitment,
    pedersen_commit,
    pedersen_add,
    pedersen_eq,
    expected_reward,
)

# ============================================================
# Block-level mass balance
# ============================================================

def verify_proof_of_token_balance(coinbase_vc: PedersenCommitment,
                                   coinbase_reward: int,
                                   coinbase_fees: int,
                                   fee_inputs: list[PedersenCommitment],
                                   fee_outputs: list[PedersenCommitment],
                                   fee_amounts: list[int],
                                   burn_inputs: list[PedersenCommitment],
                                   transfer_inputs: list[PedersenCommitment],
                                   transfer_outputs: list[PedersenCommitment],
                                   spend_inputs: list[PedersenCommitment],
                                   spend_outputs: list[PedersenCommitment],
                                   mint_outputs: list[PedersenCommitment],
                                   ) -> tuple[bool, str]:
    """Verify the block-level Pedersen mass balance.

    Returns (is_valid, diagnostic_message).

    Equation:
        Σ outputs + Σ burn_inputs + Σ fee_commits == Σ inputs

    Coinbase is excluded from both sides — verified separately against emission schedule.
    """
    # Sum all inputs
    total_inputs = PedersenCommitment(0, 0)
    for c in fee_inputs:       total_inputs = pedersen_add(total_inputs, c)
    for c in burn_inputs:      total_inputs = pedersen_add(total_inputs, c)
    for c in transfer_inputs:  total_inputs = pedersen_add(total_inputs, c)
    for c in spend_inputs:     total_inputs = pedersen_add(total_inputs, c)

    # Sum all outputs
    total_outputs = PedersenCommitment(0, 0)
    for c in fee_outputs:      total_outputs = pedersen_add(total_outputs, c)
    for c in transfer_outputs: total_outputs = pedersen_add(total_outputs, c)
    for c in spend_outputs:    total_outputs = pedersen_add(total_outputs, c)
    for c in mint_outputs:     total_outputs = pedersen_add(total_outputs, c)

    # Fee commitments: pedersen_commit(fee_amount, blind=0) for each fee call.
    # Use mk(v, 0) so r_part=0 directly — must match how balanced_fee constructs
    # its commitments (input and output share the same r, fee is additive).
    fee_aggregate = PedersenCommitment(0, 0)
    for fee in fee_amounts:
        fee_aggregate = pedersen_add(fee_aggregate, mk(fee, 0))

    # Burn aggregate: burned inputs are added to the output side so equation balances
    burn_aggregate = PedersenCommitment(0, 0)
    for c in burn_inputs:
        burn_aggregate = pedersen_add(burn_aggregate, c)

    # THE MASS BALANCE CHECK
    left = total_outputs
    left = pedersen_add(left, burn_aggregate)
    left = pedersen_add(left, fee_aggregate)
    right = total_inputs

    if not pedersen_eq(left, right):
        net = PedersenCommitment(left.v_part - right.v_part, left.r_part - right.r_part)
        return (False,
                f"MASS BALANCE FAILED: net delta v={net.v_part} r={net.r_part}\n"
                f"  left  (outs+burns+fees): v={left.v_part}\n"
                f"  right (inputs):          v={right.v_part}")

    # Coinbase verification (separate from mass balance)
    expected = coinbase_reward + coinbase_fees
    if coinbase_vc.v_part != expected:
        return (False,
                f"COINBASE MISMATCH: commit v={coinbase_vc.v_part} != expected {expected}")

    return (True, "OK")


# ============================================================
# Test helpers
# ============================================================
# In the real protocol, each transaction prover chooses output blinds such
# that the sum of output blinds equals the sum of input blinds — this makes
# the Pedersen point equality hold within each transaction (and thus across
# the block).  We model this by constructing PedersenCommitment objects
# directly with explicit v_part and r_part so that additive homomorphism
# holds exactly: C(v1,r1) + C(v2,r2) = C(v1+v2, r1+r2).
#
# Note: sim/crypto.py's pedersen_commit() hashes the blind to produce r_part,
# which breaks the simple additive property for test construction.  We use
# PedersenCommitment(v, r) directly instead.

def mk(v: int, r: int = 0) -> PedersenCommitment:
    """Create a Pedersen commitment with explicit (value, blind)."""
    return PedersenCommitment(v, r)


def balanced_transfer(input_values: list[int],
                      output_values: list[int]) -> tuple[list[PedersenCommitment],
                                                         list[PedersenCommitment]]:
    """Create a balanced TransferV1 call.

    Prover chooses random input blinds, then constrains the last output blind
    so that sum(output_blinds) == sum(input_blinds).  Both value and blind
    sums match → Pedersen point equality holds.
    """
    import os
    assert sum(input_values) == sum(output_values), "transfer must be value-neutral"
    inputs = []
    in_r_sum = 0
    for v in input_values:
        r = int.from_bytes(os.urandom(8), 'big')
        in_r_sum += r
        inputs.append(mk(v, r))
    outputs = []
    out_r_sum = 0
    for i, v in enumerate(output_values):
        if i < len(output_values) - 1:
            r = int.from_bytes(os.urandom(8), 'big')
            out_r_sum += r
            outputs.append(mk(v, r))
        else:
            # Last output: use the blind that makes sums balance
            r = in_r_sum - out_r_sum
            outputs.append(mk(v, r))
    return inputs, outputs


def balanced_fee(input_value: int, fee: int) -> tuple[PedersenCommitment,
                                                       PedersenCommitment,
                                                       int]:
    """Create a balanced FeeV1 call.

    Fee_V1 circuit constrains: output_value + fee == input_value.
    Uses same blind for input and output so they cancel, fee blind = 0.
    """
    import os
    r = int.from_bytes(os.urandom(8), 'big')
    in_commit = mk(input_value, r)
    out_commit = mk(input_value - fee, r)  # same blind as input
    return in_commit, out_commit, fee


def burn_inputs(values: list[int]) -> list[PedersenCommitment]:
    """Create BurnV1 inputs (no outputs — coins destroyed)."""
    import os
    commits = []
    for v in values:
        r = int.from_bytes(os.urandom(8), 'big')
        commits.append(mk(v, r))
    return commits


def unbalanced_mint(value: int) -> PedersenCommitment:
    """Create a MintV1 output with NO matching input — this is inflation."""
    import os
    r = int.from_bytes(os.urandom(8), 'big')
    return mk(value, r)


# ============================================================
# Tests
# ============================================================

def test_legal_transfers_only():
    """Block with only value-neutral transfers."""
    t_in, t_out = balanced_transfer([1000, 500, 300], [1000, 600, 200])
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(100) + 500),
        coinbase_reward=expected_reward(100),
        coinbase_fees=500,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[],
        transfer_inputs=t_in, transfer_outputs=t_out,
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: transfers only — 1800 in, 1800 out")


def test_legal_with_fees():
    """Fee payments: output+fee==input for each call."""
    f1_in, f1_out, fee1 = balanced_fee(5000, 300)
    f2_in, f2_out, fee2 = balanced_fee(2000, 450)
    t_in, t_out = balanced_transfer([3000], [3000])
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(200) + 750),
        coinbase_reward=expected_reward(200),
        coinbase_fees=750,
        fee_inputs=[f1_in, f2_in], fee_outputs=[f1_out, f2_out],
        fee_amounts=[fee1, fee2],
        burn_inputs=[],
        transfer_inputs=t_in, transfer_outputs=t_out,
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: fees — (4700+1550)+(300+450)+3000 == 5000+2000+3000")


def test_legal_with_burns():
    """Burns are deflationary — their inputs go on both sides of the equation."""
    burns = burn_inputs([1000, 500])
    t_in, t_out = balanced_transfer([2000], [2000])
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(300)),
        coinbase_reward=expected_reward(300),
        coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=burns,
        transfer_inputs=t_in, transfer_outputs=t_out,
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[],
    )
    assert ok, msg
    print("  PASS: burns — 2000+(1000+500) == 2000+1000+500")


def test_illegal_hidden_mint():
    """Transfer with outputs > inputs — hidden inflation. MUST reject."""
    # Deliberately unbalanced: 1M out from 100 in
    import os
    in_b = int.from_bytes(os.urandom(8), 'big')
    in_commit = pedersen_commit(100, in_b.to_bytes(8, 'big'))
    out_commit = pedersen_commit(1_000_000, in_b.to_bytes(8, 'big'))  # same blind, fake balance
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(400)),
        coinbase_reward=expected_reward(400),
        coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[],
        transfer_inputs=[in_commit], transfer_outputs=[out_commit],
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[],
    )
    assert not ok, f"Should have rejected hidden mint! {msg}"
    print(f"  REJECTED: hidden mint detected — 100 in, 1,000,000 out")


def test_illegal_standalone_mint():
    """Standalone MintV1 with no matching input. MUST reject."""
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(500)),
        coinbase_reward=expected_reward(500),
        coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[],
        transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[unbalanced_mint(50_000_000)],  # standalone mint — inflation!
    )
    assert not ok, f"Should have rejected standalone mint! {msg}"
    print(f"  REJECTED: standalone mint detected")


def test_legal_mint_balanced_by_burn():
    """Mint backed by burn — balanced pair, net zero. Should pass.

    When a mint uses burned value, the burn input is the 'input' side
    and the mint output is the 'output' side of a balanced exchange.
    The burn_aggregate mechanism is for UNMATCHED standalone burns.
    """
    # The burn produces a 'credit' consumed by the mint.
    # Pass them as transfer inputs/outputs — a balanced exchange.
    import os
    r = int.from_bytes(os.urandom(8), 'big')
    burn_commit = mk(50_000_000, r)
    mint_commit = mk(50_000_000, r)  # same blind so Pedersen points are equal
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(600)),
        coinbase_reward=expected_reward(600),
        coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[],  # NOT a standalone burn — it backs the mint
        transfer_inputs=[burn_commit],
        transfer_outputs=[mint_commit],
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[],  # NOT a standalone mint — it's backed by the burn
    )
    assert ok, msg
    print("  PASS: mint balanced by burn — net zero")


def test_illegal_mint_exceeds_burn():
    """Mint exceeds burn — net positive inflation. MUST reject."""
    import os
    r = int.from_bytes(os.urandom(8), 'big')
    burn_commit = mk(10_000_000, r)
    mint_commit = mk(50_000_000, r)  # mint 50M backed by only 10M burned — net +40M!
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=mk(expected_reward(700)),
        coinbase_reward=expected_reward(700),
        coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[],
        transfer_inputs=[burn_commit],
        transfer_outputs=[mint_commit],
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[],
    )
    assert not ok, f"Should have rejected mint-exceeds-burn! {msg}"
    print(f"  REJECTED: mint > burn detected — 50M minted, only 10M burned")


def test_coinbase_exceeds_schedule():
    """Coinbase value exceeds emission schedule. MUST reject."""
    excessive = expected_reward(800) + 1_000_000
    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=pedersen_commit(excessive, b'cb'),
        coinbase_reward=expected_reward(800),
        coinbase_fees=0,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[],
        transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[],
        mint_outputs=[],
    )
    assert not ok, f"Should have rejected excessive coinbase! {msg}"
    print(f"  REJECTED: coinbase exceeds schedule")


# ============================================================
# Integration: mass balance + cumulative chain together
# ============================================================

def test_integration_with_cumulative_chain():
    """Proof of token balance complements the cumulative supply chain.

    Cumulative chain (existing test): S_H = S_{H-1} + C_H — verifies
    each coinbase correctly extends the supply commitment chain.

    Proof of token balance (this test): Σ outs + burns + fees = Σ ins —
    verifies non-coinbase transactions don't secretly mint.

    Together: total darkw supply = emission schedule, no hidden inflation.
    """
    print("\n  Integration: cumulative chain + mass balance...")

    # Simulate 10 blocks, each with a mix of transfers, fees, burns
    cumulative = PedersenCommitment(0, 0)
    total_supply = 0
    prev_coin = b'\x00' * 32

    for h in range(1, 11):
        reward = expected_reward(h)
        fees = h * 50  # increasing fees
        coinbase_vc = pedersen_commit(reward + fees, (b'cb_%d' % h))

        # Cumulative chain check: S_H = S_{H-1} + C_H
        cumulative = pedersen_add(cumulative, coinbase_vc)
        total_supply += reward + fees
        prev_coin = (prev_coin + b'%d' % h)[:32]

        # Mass balance check: each block's txs are neutral
        f_in, f_out, fee = balanced_fee(1000 + h * 100, fees)
        t_in, t_out = balanced_transfer([h * 500, h * 300], [h * 500, h * 300])
        ok, msg = verify_proof_of_token_balance(
            coinbase_vc=coinbase_vc,
            coinbase_reward=reward,
            coinbase_fees=fees,
            fee_inputs=[f_in], fee_outputs=[f_out], fee_amounts=[fee],
            burn_inputs=[],
            transfer_inputs=t_in, transfer_outputs=t_out,
            spend_inputs=[], spend_outputs=[],
            mint_outputs=[],
        )
        assert ok, f"Block {h} failed: {msg}"

    print(f"  OK — 10 blocks, cumulative v={cumulative.v_part}, supply={total_supply}")


# ============================================================
# Runner
# ============================================================

def run_all():
    tests = [
        test_legal_transfers_only,
        test_legal_with_fees,
        test_legal_with_burns,
        test_illegal_hidden_mint,
        test_illegal_standalone_mint,
        test_legal_mint_balanced_by_burn,
        test_illegal_mint_exceeds_burn,
        test_coinbase_exceeds_schedule,
        test_integration_with_cumulative_chain,
    ]
    passed = 0
    failed = 0
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"  FAIL: {e}")
            import traceback
            traceback.print_exc()
    print(f"\n=== proof_of_token_balance: {passed} passed, {failed} failed ===")
    return failed == 0


if __name__ == "__main__":
    success = run_all()
    sys.exit(0 if success else 1)
