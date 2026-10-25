#!/usr/bin/env python3
"""Control for OBL-C109: the model's coinbase-maturity predicate is nullifier-keyed.

The consensus rule (`CChainState::check_coinbase_maturity`) keys by nullifier through the
per-nullifier set; it deliberately rejected a commitment-keyed rule because the commitment set is
pruned at the maturity window. The model used to expose a commitment-keyed helper that claimed to
mirror the code. This file is the control for the corrected model: it lives beside the model, not
inside the file under test, so the register gate's "control is the locus itself" rule is not
bypassed — the model is the subject, this suite is the check.

Run: python3 contrib/model/chain_model_test.py
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from chain_model import COINBASE_MATURITY, ChainState  # noqa: E402


def test_coinbase_maturity_is_nullifier_keyed() -> None:
    chain = ChainState()
    nullifier = b"\x11" * 32
    # A nullifier with no recorded height is not the maturity rule's business; the replay gate
    # judges it. The code passes it, and so must the model.
    assert chain.is_nullifier_mature(nullifier, 1), \
        "an unknown nullifier must not be refused by the maturity rule"
    chain.track_nullifier(nullifier, 1)
    assert not chain.is_nullifier_mature(nullifier, 1 + COINBASE_MATURITY - 1), \
        "a recorded nullifier inside the window must be refused"
    assert chain.is_nullifier_mature(nullifier, 1 + COINBASE_MATURITY), \
        "a recorded nullifier at the boundary must pass"
    # A commitment in the commitment set does not answer the nullifier-keyed rule.
    chain.track_coinbase(b"\x22" * 32, 1)
    assert chain.is_nullifier_mature(b"\x33" * 32, 1), \
        "a commitment-keyed lookup must not make an untracked nullifier immature"


if __name__ == "__main__":
    test_coinbase_maturity_is_nullifier_keyed()
    print("PASS: chain model maturity is nullifier-keyed (OBL-C109).")
