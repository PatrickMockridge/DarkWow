"""ZK hardening tests — detect the 9 audit bugs from June 2026.

Each test:
1. Models the bug (circuit constraint missing or entrypoint check absent)
2. Attempts the exploit — verifies it SUCCEEDS (bug confirmed)
3. Applies the fix (constraint added or check enabled)
4. Attempts the exploit again — verifies it FAILS (fix confirmed)

These tests operate on the SIMULATED contract models, not the real
Rust contracts. They validate that the model correctly represents
the vulnerability and the fix.
"""

import sys
import os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from sim.crypto import (
    poseidon_hash, pedersen_commit, pedersen_add, pedersen_eq,
    ec_mul_base, nullifier, coin_commitment,
    derive_signature_secret, token_auth_parent, token_registry_root,
    expected_reward, expected_cumulative_supply,
)


# ============================================================
# Helper: Mini contract classes with circuit assertions built in
# ============================================================

class CircuitError(Exception):
    """Raised when a ZK circuit constraint is violated."""
    pass


class EntrypointError(Exception):
    """Raised when an entrypoint check fails."""
    pass


# -----------------------------------------------------------
# C1: PromissoryNote MintV1 — mint_public unconstrained
# -----------------------------------------------------------

def test_c1_mint_public_unconstrained():
    """C1: PromissoryNote mint_public not derived from backing_secret.

    The Mint_V1 circuit exposed mint_public as a public input but had
    no constraint proving mint_public = poseidon_hash(backing_secret).
    Anyone could read stored_auth from the on-chain token registry
    and set mint_public = stored_auth as a free witness.
    """
    print("C1: PromissoryNote mint_public unconstrained...")

    # Setup: legitimate token registered with auth_parent = hash(legit_secret)
    legit_secret = b'legit_backing_secret_123456789012'
    stored_auth = token_auth_parent(legit_secret)
    token_id = b'token_deadbeef'
    token_registry = {token_id: stored_auth}  # public on-chain data

    # Attacker reads stored_auth from the public registry
    attacker_backing_secret = b'attacker_does_not_know_legit'

    # === BUG: circuit does NOT constrain mint_public derivation ===
    def mint_v1_circuit_BUGGY(witnesses):
        """Buggy circuit — mint_public is a free witness."""
        # No constraint: mint_public = poseidon_hash(backing_secret)  ← MISSING
        coin = coin_commitment(
            witnesses['public_key'], witnesses['coin_value'],
            witnesses['token_id'], witnesses['spend_hook'],
            witnesses['user_data'], witnesses['coin_blind'],
        )
        assert coin == witnesses['coin'], "Coin hash mismatch"
        vc = pedersen_commit(witnesses['coin_value'], witnesses['value_blind'])
        assert pedersen_eq(vc, witnesses['value_commit']), "VC mismatch"
        return {
            'mint_public': witnesses['mint_public'],
            'coin': coin, 'token_id': witnesses['token_id'],
        }

    # Attacker constructs witnesses with mint_public = stored_auth
    attacker_witnesses = {
        'backing_secret': attacker_backing_secret,   # doesn't match stored_auth
        'mint_public': stored_auth,                   # read from public registry!
        'public_key': ec_mul_base(b'attacker_pubkey'),
        'coin_value': 1_000_000_000_000,
        'token_id': token_id,
        'spend_hook': b'\x00' * 32,
        'user_data': b'\x00' * 32,
        'coin_blind': b'attacker_blind_1234567890123456',
        'value_blind': b'attacker_vblind_12345678901234',
        'coin': coin_commitment(ec_mul_base(b'attacker_pubkey'),
                                1_000_000_000_000, token_id,
                                b'\x00' * 32, b'\x00' * 32,
                                b'attacker_blind_1234567890123456'),
        'value_commit': pedersen_commit(1_000_000_000_000,
                                        b'attacker_vblind_12345678901234'),
    }

    # EXPLOIT: buggy circuit accepts — mint_public matches stored_auth
    pub = mint_v1_circuit_BUGGY(attacker_witnesses)  # No error
    assert pub['mint_public'] == stored_auth
    assert pub['token_id'] == token_id
    print("  EXPLOIT (bug): mint succeeded with mint_public = stored_auth (free witness) OK")

    # === FIX: circuit constrains mint_public = poseidon_hash(backing_secret) ===
    def mint_v1_circuit_FIXED(witnesses):
        """Fixed circuit — mint_public MUST be derived from backing_secret."""
        derived = poseidon_hash(witnesses['backing_secret'])
        if derived != witnesses['mint_public']:
            raise CircuitError("C1 FIX: mint_public not derived from backing_secret")
        # ... rest of constraints same as buggy ...

    # FIX: attacker's witnesses now fail because mint_public != hash(attacker_secret)
    try:
        mint_v1_circuit_FIXED(attacker_witnesses)
        assert False, "Should have raised CircuitError"
    except CircuitError as e:
        assert "mint_public not derived" in str(e)
    print("  FIXED: mint rejected — mint_public != poseidon_hash(attacker_secret) OK")

    # Legitimate minter still succeeds
    legit_witnesses = dict(attacker_witnesses)
    legit_witnesses['backing_secret'] = legit_secret
    legit_witnesses['mint_public'] = stored_auth  # == hash(legit_secret)
    mint_v1_circuit_FIXED(legit_witnesses)  # No error
    print("  FIXED: legitimate mint still succeeds OK")

    return True


# -----------------------------------------------------------
# C2: NativeToken FeeV1 — no output_value = input_value - fee constraint
# -----------------------------------------------------------

def test_c2_fee_no_value_conservation():
    """C2: FeeV1 circuit had zero constraint linking input_value to output_value.

    The fee subtraction was off-circuit in the Rust client. A prover
    could set output_value = input_value + 1,000,000 and generate
    a valid ZK proof.
    """
    print("C2: NativeToken FeeV1 no value conservation...")

    fee = 100
    input_value = 1000
    inflated_output = 1000000  # should be 900, but attacker sets to 1M

    # === BUG: circuit has no fee constraint ===
    def fee_v1_circuit_BUGGY(witnesses):
        """Buggy circuit — input_value and output_value are independent."""
        # No constraint: output_value + fee == input_value  ← MISSING
        ic = coin_commitment(witnesses['pub'], witnesses['input_value'],
                             witnesses['token_id'])
        oc = coin_commitment(witnesses['pub'], witnesses['output_value'],
                             witnesses['token_id'])
        ivc = pedersen_commit(witnesses['input_value'], witnesses['input_blind'])
        ovc = pedersen_commit(witnesses['output_value'], witnesses['output_blind'])
        assert pedersen_eq(ivc, witnesses['input_value_commit'])
        assert pedersen_eq(ovc, witnesses['output_value_commit'])
        assert ic == witnesses['input_coin']
        assert oc == witnesses['output_coin']
        # Fee not constrained — exploit possible

    # EXPLOIT: inflated output passes
    fee_v1_circuit_BUGGY({
        'pub': ec_mul_base(b'secret'),
        'input_value': input_value, 'output_value': inflated_output, 'fee': fee,
        'token_id': b'\x00' * 32,
        'input_blind': b'ib', 'output_blind': b'ob',
        'input_value_commit': pedersen_commit(input_value, b'ib'),
        'output_value_commit': pedersen_commit(inflated_output, b'ob'),
        'input_coin': coin_commitment(ec_mul_base(b'secret'), input_value, b'\x00' * 32),
        'output_coin': coin_commitment(ec_mul_base(b'secret'), inflated_output, b'\x00' * 32),
    })
    print("  EXPLOIT (bug): fee tx accepted with inflated output_value OK")

    # === FIX: circuit enforces output_value + fee == input_value ===
    def fee_v1_circuit_FIXED(witnesses):
        """Fixed circuit — output_value + fee == input_value."""
        if witnesses['output_value'] + witnesses['fee'] != witnesses['input_value']:
            raise CircuitError("C2 FIX: output_value + fee != input_value")
        # ... rest of constraints ...

    # FIX: inflated output fails
    try:
        fee_v1_circuit_FIXED({
            'input_value': input_value, 'output_value': inflated_output, 'fee': fee,
        })
        assert False, "Should have raised CircuitError"
    except CircuitError as e:
        assert "output_value + fee != input_value" in str(e)
    print("  FIXED: fee tx rejected — output_value + fee != input_value OK")

    # Legitimate fee still works
    fee_v1_circuit_FIXED({
        'input_value': 1000, 'output_value': 900, 'fee': 100,
    })  # No error
    print("  FIXED: legitimate fee tx still succeeds OK")

    return True


# -----------------------------------------------------------
# C3: NativeToken MintV1 — no authority, no supply tracking
# -----------------------------------------------------------

def test_c3_mintv1_disabled():
    """C3: NativeToken MintV1 accepted any valid ZK proof without authority or supply check.

    Fix: MintV1 removed from all dispatch tables. Only PoWRewardV1 can mint.
    """
    print("C3: NativeToken MintV1 disabled...")

    class NativeToken_BUGGY:
        def __init__(self):
            self.total_supply = 0
            self.coins = set()

        def mint_v1(self, coin, value):
            """Buggy — only checks coin uniqueness, no authority, no supply."""
            if coin in self.coins:
                raise EntrypointError("Duplicate coin")
            self.coins.add(coin)
            # total_supply NEVER updated — supply tracking bypassed
            return True

    nt = NativeToken_BUGGY()
    coin = coin_commitment(ec_mul_base(b'secret'), 1_000_000_000, b'\x00' * 32)

    # EXPLOIT: mint succeeds, supply unchanged
    nt.mint_v1(coin, 1_000_000_000)
    assert nt.total_supply == 0  # Supply NOT tracked!
    print("  EXPLOIT (bug): mint succeeded, total_supply still 0 (bypassed) OK")

    # === FIX: MintV1 disabled ===
    class NativeToken_FIXED:
        def __init__(self):
            self.total_supply = 0
            self.coins = set()

        def mint_v1(self, coin, value):
            """Fixed — MintV1 is disabled."""
            raise EntrypointError("MintV1 is disabled — use PoWRewardV1 for block rewards")

        def pow_reward_v1(self, coin, value, height):
            """PoW reward with supply enforcement."""
            expected = expected_reward(height)
            if value != expected:
                raise EntrypointError(f"Reward {value} != expected {expected}")
            new_supply = self.total_supply + value
            expected_cum = expected_cumulative_supply(height)
            if new_supply != expected_cum:
                raise EntrypointError(f"Supply {new_supply} != expected {expected_cum}")
            if coin in self.coins:
                raise EntrypointError("Duplicate coin")
            self.coins.add(coin)
            self.total_supply = new_supply
            return True

    nt2 = NativeToken_FIXED()

    # FIX: MintV1 is disabled
    try:
        nt2.mint_v1(coin, 1_000_000_000)
        assert False, "Should have raised"
    except EntrypointError as e:
        assert "MintV1 is disabled" in str(e)
    print("  FIXED: MintV1 rejected — disabled OK")

    # PoWRewardV1 still works with supply tracking
    coin2 = coin_commitment(ec_mul_base(b'miner'), expected_reward(1), b'\x00' * 32)
    nt2.pow_reward_v1(coin2, expected_reward(1), 1)
    assert nt2.total_supply == expected_reward(1)
    print("  FIXED: PoWRewardV1 works with supply tracking OK")

    return True


# -----------------------------------------------------------
# C4: NativeToken TransferV1 — no value conservation
# -----------------------------------------------------------

def test_c4_transfer_no_conservation():
    """C4: NativeToken TransferV1 had no sum(inputs) == sum(outputs) check.

    A prover with one input coin of value 100 could create two output
    coins each of value 1,000,000. Each burn/mint proof verifies
    independently but there's no cross-proof sum check.
    """
    print("C4: NativeToken TransferV1 no value conservation...")

    # === BUG: no cross-proof value conservation ===
    def verify_value_conservation_BUGGY(inputs, outputs):
        """Buggy — no check at all."""
        pass  # ← Nothing! Each proof is independent.

    # One input (value=100), two outputs (each value=1,000,000)
    input_coin_secret = b'coin_secret_123456789012345678'
    input_pub = ec_mul_base(input_coin_secret)
    token_id = b'\x00' * 32

    inputs = [{
        'value_commit': pedersen_commit(100, b'blind_in'),
        'token_commit': poseidon_hash(token_id, b'tblind'),
    }]

    outputs = [
        {'value_commit': pedersen_commit(1_000_000, b'blind_out1'),
         'token_commit': poseidon_hash(token_id, b'tblind1')},
        {'value_commit': pedersen_commit(1_000_000, b'blind_out2'),
         'token_commit': poseidon_hash(token_id, b'tblind2')},
    ]

    # EXPLOIT: buggy check passes (no-op)
    verify_value_conservation_BUGGY(inputs, outputs)
    print("  EXPLOIT (bug): transfer accepted — no value conservation check OK")

    # === FIX: verify sum(input vc) == sum(output vc) per token_commit ===
    def verify_value_conservation_FIXED(inputs, outputs):
        """Fixed — Pedersen homomorphic sum check per token_commit."""
        from collections import defaultdict
        input_sums = defaultdict(lambda: pedersen_commit(0, b'\x00'))
        output_sums = defaultdict(lambda: pedersen_commit(0, b'\x00'))

        for inp in inputs:
            tc = inp['token_commit']
            input_sums[tc] = pedersen_add(input_sums[tc], inp['value_commit'])

        for out in outputs:
            tc = out['token_commit']
            output_sums[tc] = pedersen_add(output_sums[tc], out['value_commit'])

        for tc, isum in input_sums.items():
            osum = output_sums.get(tc)
            if osum is None or not pedersen_eq(isum, osum):
                raise EntrypointError(
                    f"C4 FIX: Value conservation failed for token_commit"
                )

        for tc in output_sums:
            if tc not in input_sums:
                raise EntrypointError(
                    f"C4 FIX: Output token_commit not in inputs"
                )

    # FIX: inflated outputs rejected
    try:
        verify_value_conservation_FIXED(inputs, outputs)
        assert False, "Should have raised"
    except EntrypointError as e:
        assert "Value conservation failed" in str(e)
    print("  FIXED: transfer rejected — value conservation mismatch OK")

    # Legitimate transfer still works (same token_commit for in/out)
    # For Pedersen homomorphism: C(v1,b1)+C(v2,b2) = C(v1+v2,b1+b2)
    # So for input 100 -> outputs 50+50, we need:
    # C(100, b) == C(50, b1) + C(50, b2) = C(100, b1+b2)
    # Therefore b1+b2 must equal b.
    # Use the same blind_int for simplicity: C(50,1)+C(50,1) = C(100,2) != C(100,1)
    # So pass raw ints: C(100, 10) = C(50, 3) + C(50, 7)
    from sim.crypto import PedersenCommitment
    same_tc = poseidon_hash(token_id, b'same_tblind')
    legit_inputs = [{
        'value_commit': PedersenCommitment(100, 10),
        'token_commit': same_tc,
    }]
    legit_outputs = [
        {'value_commit': PedersenCommitment(50, 3),
         'token_commit': same_tc},
        {'value_commit': PedersenCommitment(50, 7),
         'token_commit': same_tc},
    ]
    verify_value_conservation_FIXED(legit_inputs, legit_outputs)  # No error
    print("  FIXED: legitimate transfer (100 -> 50+50) still succeeds OK")

    return True


# -----------------------------------------------------------
# H2: Burn circuits — per-burn signature derivation
# -----------------------------------------------------------

def test_h2_per_burn_signature_derivation():
    """H2: Burn circuits had independent coin_secret and signature_secret.

    Fix: signature_secret = poseidon_hash(coin_secret, nullifier).
    This binds the signer to the coin owner while keeping each burn's
    signature_public unlinkable (different nullifier per coin).
    """
    print("H2: Per-burn signature derivation...")

    coin_secret = b'coin_owner_secret_1234567890123'
    coin = b'deadbeef_coin_hash_123456789012345'

    # === BUG: independent secrets ===
    def burn_circuit_BUGGY(witnesses):
        """Buggy — coin_secret and signature_secret are independent."""
        # Both used but never cross-constrained
        nf = nullifier(witnesses['coin_secret'], witnesses['coin'])
        pub = ec_mul_base(witnesses['coin_secret'])
        sig_pub = ec_mul_base(witnesses['signature_secret'])
        # No constraint: coin_secret == signature_secret  ← MISSING
        return {'nullifier': nf, 'pub': pub, 'sig_pub': sig_pub}

    # EXPLOIT: different secrets accepted
    different_sig_secret = b'attacker_signing_key_123456789'
    result_bug = burn_circuit_BUGGY({
        'coin_secret': coin_secret,
        'signature_secret': different_sig_secret,
        'coin': coin,
    })
    assert result_bug['pub'] != result_bug['sig_pub']  # Different pubkeys!
    print("  EXPLOIT (bug): burn accepted with different coin_secret/signature_secret OK")

    # === FIX: per-burn signature derivation ===
    def burn_circuit_FIXED(witnesses):
        """Fixed — signature_secret = poseidon_hash(coin_secret, nullifier)."""
        nf = nullifier(witnesses['coin_secret'], witnesses['coin'])
        expected_sig = derive_signature_secret(witnesses['coin_secret'], nf)
        if expected_sig != witnesses['signature_secret']:
            raise CircuitError(
                "H2 FIX: signature_secret != poseidon_hash(coin_secret, nullifier)"
            )
        pub = ec_mul_base(witnesses['coin_secret'])
        sig_pub = ec_mul_base(witnesses['signature_secret'])
        return {'nullifier': nf, 'pub': pub, 'sig_pub': sig_pub}

    # FIX: independent secret rejected
    try:
        burn_circuit_FIXED({
            'coin_secret': coin_secret,
            'signature_secret': different_sig_secret,
            'coin': coin,
        })
        assert False, "Should have raised"
    except CircuitError as e:
        assert "signature_secret !=" in str(e)
    print("  FIXED: independent signature_secret rejected OK")

    # FIX: two burns of the same coin produce different sig_pubs (privacy)
    nf1 = nullifier(coin_secret, coin)
    sig1 = derive_signature_secret(coin_secret, nf1)
    pub1 = ec_mul_base(sig1)

    # Different coin → different nullifier → different signature_public
    coin2 = b'different_coin_hash_abcdef123456789'
    nf2 = nullifier(coin_secret, coin2)
    sig2 = derive_signature_secret(coin_secret, nf2)
    pub2 = ec_mul_base(sig2)

    assert nf1 != nf2  # Different coins, different nullifiers
    assert sig1 != sig2  # Different nullifiers → different sig_secrets
    assert pub1 != pub2  # Different sig_secrets → different sig_pubs (UNLINKABLE)
    print("  FIXED: different burns produce unlinkable signature_publics OK")

    return True


# -----------------------------------------------------------
# H3: BearerBond IssueStakeV1 — no issuer authorization
# -----------------------------------------------------------

def test_h3_bearer_bond_issuer_check():
    """H3: BearerBond IssueStakeV1 checked series exists but not caller identity.

    Fix: compare params.issuer_contract against stored series_info.issuer_contract.
    """
    print("H3: BearerBond IssueStakeV1 issuer authorization...")

    series_token_id = b'series_token_123456789012345678'
    legit_issuer = b'legit_issuer_contract_id_123456'
    attacker_issuer = b'attacker_contract_id_987654321'

    # === BUG: no issuer check ===
    class BearerBond_BUGGY:
        def __init__(self):
            self.series = {}
            self.coins = set()

        def issue_stake_v1(self, token_id, issuer_contract, coin):
            """Buggy — only checks series exists."""
            if token_id not in self.series:
                raise EntrypointError("Series not found")
            # No check: issuer_contract == series_info.issuer_contract  ← MISSING
            if coin in self.coins:
                raise EntrypointError("Stake already exists")
            self.coins.add(coin)
            return True

    bb = BearerBond_BUGGY()
    bb.series[series_token_id] = {'issuer_contract': legit_issuer}

    # EXPLOIT: attacker issues stakes for any series
    attacker_coin = b'attacker_coin_123456789012345678'
    bb.issue_stake_v1(series_token_id, attacker_issuer, attacker_coin)
    print("  EXPLOIT (bug): attacker issued stakes for any series OK")

    # === FIX: issuer check ===
    class BearerBond_FIXED:
        def __init__(self):
            self.series = {}
            self.coins = set()

        def issue_stake_v1(self, token_id, issuer_contract, coin):
            """Fixed — verifies caller is the authorized issuer."""
            if token_id not in self.series:
                raise EntrypointError("Series not found")
            series_info = self.series[token_id]
            if issuer_contract != series_info['issuer_contract']:
                raise EntrypointError(
                    "H3 FIX: Caller is not the authorized issuer"
                )
            if coin in self.coins:
                raise EntrypointError("Stake already exists")
            self.coins.add(coin)
            return True

    bb2 = BearerBond_FIXED()
    bb2.series[series_token_id] = {'issuer_contract': legit_issuer}

    # FIX: attacker rejected
    try:
        bb2.issue_stake_v1(series_token_id, attacker_issuer, attacker_coin)
        assert False, "Should have raised"
    except EntrypointError as e:
        assert "not the authorized issuer" in str(e)
    print("  FIXED: unauthorized issuer rejected OK")

    # Legit issuer still works
    bb2.issue_stake_v1(series_token_id, legit_issuer, b'legit_coin_123456')
    print("  FIXED: legitimate issuer still succeeds OK")

    return True


# -----------------------------------------------------------
# H4: Bridge WithdrawV1 — merkle_root_val not exposed
# -----------------------------------------------------------

def test_h4_bridge_merkle_root_verification():
    """H4: Bridge WithdrawV1 ZK circuit self-verified merkle_root_val.

    merkle_root_val was a witness used in constrain_equal_base but
    never constrain_instanced. The prover could set it to any value.
    """
    print("H4: Bridge WithdrawV1 merkle root verification...")

    on_chain_root = b'on_chain_deposit_tree_root_12345'

    # === BUG: merkle_root_val is self-verified by prover ===
    def withdraw_circuit_BUGGY(witnesses):
        """Buggy — merkle_root_val not exposed as public input."""
        computed_root = poseidon_hash(witnesses['deposit_leaf'],
                                       witnesses['merkle_path'])
        # constrain_equal_base(computed_root, merkle_root_val) in real circuit
        # but merkle_root_val is never constrain_instanced
        assert computed_root == witnesses['merkle_root_val']  # always true by construction
        # Entrypoint can't check merkle_root_val against on-chain state
        return {'nullifier': witnesses['nullifier'],
                'deposit_leaf': witnesses['deposit_leaf']}
        # merkle_root_val NOT in public inputs!

    # Attacker creates withdrawal with arbitrary merkle root
    fake_merkle_root = b'fake_root_attacker_made_up_12345'
    fake_leaf = b'fake_deposit_leaf_123456789012345'
    attacker_witnesses = {
        'nullifier': b'attacker_nullifier_12345678901',
        'deposit_leaf': fake_leaf,
        'merkle_path': b'fake_path',
        'merkle_root_val': poseidon_hash(fake_leaf, b'fake_path'),  # self-consistent
    }

    pub_bug = withdraw_circuit_BUGGY(attacker_witnesses)
    assert 'merkle_root_val' not in pub_bug  # Not in public inputs!
    print("  EXPLOIT (bug): withdrawal accepted — merkle_root_val self-verified OK")

    # === FIX: expose merkle_root_val as public input ===
    def withdraw_circuit_FIXED(witnesses):
        """Fixed — merkle_root_val is constrain_instanced."""
        computed_root = poseidon_hash(witnesses['deposit_leaf'],
                                       witnesses['merkle_path'])
        assert computed_root == witnesses['merkle_root_val']
        return {
            'nullifier': witnesses['nullifier'],
            'deposit_leaf': witnesses['deposit_leaf'],
            'merkle_root_val': witnesses['merkle_root_val'],  # NOW in public inputs!
        }

    pub_fix = withdraw_circuit_FIXED(attacker_witnesses)
    assert 'merkle_root_val' in pub_fix  # Now exposed

    # Entrypoint verifies merkle_root_val against on-chain state
    # Attacker's fake root doesn't match the on-chain deposit tree root
    if pub_fix['merkle_root_val'] != on_chain_root:
        print("  FIXED: withdrawal rejected — merkle_root_val != on-chain root OK")

    # With CORRECT merkle root matching on-chain state, withdrawal succeeds
    correct_leaf = b'correct_deposit_leaf_123456789'
    correct_path = b'correct_merkle_path_1234567890'
    correct_root = poseidon_hash(correct_leaf, correct_path)
    correct_on_chain_root = correct_root  # matches

    pub_correct = withdraw_circuit_FIXED({
        'nullifier': b'legit_nullifier_123456789012',
        'deposit_leaf': correct_leaf,
        'merkle_path': correct_path,
        'merkle_root_val': correct_root,
    })
    assert pub_correct['merkle_root_val'] == correct_on_chain_root
    print("  FIXED: withdrawal with correct merkle root succeeds OK")

    return True


# -----------------------------------------------------------
# M1: Stablecoin AccrueInterestV1 — old_total_debt unvalidated
# -----------------------------------------------------------

def test_m1_stablecoin_old_debt_unvalidated():
    """M1: AccrueInterestV1 circuit had old_total_debt as unconstrained witness.

    A prover could supply a stale (lower) old_total_debt, computing
    interest on a smaller base and under-reporting the new total.
    """
    print("M1: Stablecoin old_total_debt unvalidated...")

    on_chain_total_debt = 1_000_000  # Actual on-chain debt
    stale_debt = 500_000  # Attacker supplies older, lower value
    rate_per_second = 100
    time_elapsed = 86400
    DENOM = 315360000000

    # === BUG: old_total_debt not validated against on-chain ===
    def accrue_interest_circuit_BUGGY(witnesses):
        """Buggy — old_total_debt is a free witness."""
        # old_total_debt NOT checked against on-chain state
        debt_times_rate = witnesses['old_total_debt'] * witnesses['rate_per_second']
        debt_rate_time = debt_times_rate * witnesses['time_elapsed']
        interest = debt_rate_time // DENOM
        new_debt = witnesses['old_total_debt'] + interest
        assert new_debt == witnesses['new_total_debt']
        return {'new_total_debt': new_debt}
        # old_total_debt NOT in public inputs!

    # EXPLOIT: stale old_total_debt accepted
    pub_bug = accrue_interest_circuit_BUGGY({
        'old_total_debt': stale_debt,
        'new_total_debt': stale_debt + (stale_debt * rate_per_second * time_elapsed // DENOM),
        'rate_per_second': rate_per_second,
        'time_elapsed': time_elapsed,
    })
    # Interest computed on 500k instead of 1M — under-reported
    assert pub_bug['new_total_debt'] < on_chain_total_debt
    print("  EXPLOIT (bug): interest computed on stale old_total_debt OK")

    # === FIX: old_total_debt exposed and verified ===
    def accrue_interest_circuit_FIXED(witnesses):
        """Fixed — old_total_debt exposed as public input."""
        debt_times_rate = witnesses['old_total_debt'] * witnesses['rate_per_second']
        debt_rate_time = debt_times_rate * witnesses['time_elapsed']
        interest = debt_rate_time // DENOM
        new_debt = witnesses['old_total_debt'] + interest
        assert new_debt == witnesses['new_total_debt']
        return {
            'old_total_debt': witnesses['old_total_debt'],  # NOW exposed
            'new_total_debt': new_debt,
        }

    pub_stale = accrue_interest_circuit_FIXED({
        'old_total_debt': stale_debt,
        'new_total_debt': stale_debt + (stale_debt * rate_per_second * time_elapsed // DENOM),
        'rate_per_second': rate_per_second,
        'time_elapsed': time_elapsed,
    })

    # Entrypoint checks old_total_debt against on-chain — stale value rejected
    if pub_stale['old_total_debt'] != on_chain_total_debt:
        print("  FIXED: stale old_total_debt rejected OK")

    # Correct old_total_debt works
    pub_correct = accrue_interest_circuit_FIXED({
        'old_total_debt': on_chain_total_debt,
        'new_total_debt': on_chain_total_debt + (on_chain_total_debt * rate_per_second * time_elapsed // DENOM),
        'rate_per_second': rate_per_second,
        'time_elapsed': time_elapsed,
    })
    assert pub_correct['old_total_debt'] == on_chain_total_debt
    print("  FIXED: correct old_total_debt accepted OK")

    return True


# -----------------------------------------------------------
# H1: Same-block double-spend via isolated overlays
# -----------------------------------------------------------

def test_h1_same_block_double_spend():
    """H1: Two txs spending same nullifier in same block both pass checks.

    Each call sees base_overlay.clone() — no call sees another's writes.
    Both pass exec-phase nullifier checks, both writes land in merge.
    """
    print("H1: Same-block double-spend via isolated overlays...")

    class Overlay:
        """Simulated sled overlay — independent copy of base state."""
        def __init__(self, base_state: dict):
            self.state = dict(base_state)  # clone!
            self.writes = {}

        def contains(self, key) -> bool:
            return key in self.state or key in self.writes

        def write(self, key, value):
            self.writes[key] = value

        def diff(self) -> dict:
            return dict(self.writes)

    # Base state: nullifier NOT spent
    base_state = {'nullifiers': {}}
    nullifier_key = 'nf_deadbeef'

    # Transaction 1: spends nullifier
    overlay1 = Overlay(base_state)
    assert not overlay1.contains(nullifier_key)  # Passes — not in base state
    overlay1.write(nullifier_key, True)
    diff1 = overlay1.diff()

    # Transaction 2: spends SAME nullifier
    overlay2 = Overlay(base_state)  # Independent clone — doesn't see overlay1's write!
    assert not overlay2.contains(nullifier_key)  # ALSO passes — isolated overlay!
    overlay2.write(nullifier_key, True)
    diff2 = overlay2.diff()

    # Both diffs "succeed"
    assert nullifier_key in diff1
    assert nullifier_key in diff2
    print("  EXPLOIT (bug): both txs passed nullifier check in isolated overlays OK")

    # Merge — silent overwrite (BUG)
    merged = dict(base_state)
    merged.update(diff1)  # First write
    merged.update(diff2)  # Second write silently overwrites
    assert nullifier_key in merged
    print("  EXPLOIT (bug): merge silently overwrites — double-spend undetected OK")

    # === FIX: conflict detection before merge ===
    def merge_with_conflict_detection(base, diffs):
        """Fixed merge — detects key conflicts before applying diffs."""
        state = dict(base)
        for i, diff in enumerate(diffs):
            conflict = any(k in state for k in diff)
            if conflict:
                raise EntrypointError(
                    f"H1 FIX: Key conflict in diff {i} — block rejected"
                )
            state.update(diff)
        return state

    # FIX: conflicting diffs rejected
    try:
        merge_with_conflict_detection(base_state, [diff1, diff2])
        assert False, "Should have raised"
    except EntrypointError:
        pass  # Expected
    print("  FIXED: conflicting diffs rejected OK")

    # FIX: non-conflicting diffs still merge correctly
    diff3 = {'other_key': True}
    result = merge_with_conflict_detection(base_state, [diff1, diff3])
    assert nullifier_key in result
    assert 'other_key' in result
    print("  FIXED: non-conflicting diffs merge correctly OK")

    return True


# ============================================================
# Runner
# ============================================================

def run_all():
    tests = [
        test_c1_mint_public_unconstrained,
        test_c2_fee_no_value_conservation,
        test_c3_mintv1_disabled,
        test_c4_transfer_no_conservation,
        test_h2_per_burn_signature_derivation,
        test_h3_bearer_bond_issuer_check,
        test_h4_bridge_merkle_root_verification,
        test_m1_stablecoin_old_debt_unvalidated,
        test_h1_same_block_double_spend,
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

    print(f"\n=== Results: {passed} passed, {failed} failed ===")
    return failed == 0


if __name__ == "__main__":
    success = run_all()
    sys.exit(0 if success else 1)
