"""Dex lock-proof model — the dex-money Merkle lock-proof integration.

Spec: `src/contract/dex/src/entrypoint/accept_swap.rs` `verify_lock_proof`.

The dex `accept_swap` verifies that the acceptor owns a coin in the money
contract's coin tree by checking a "lock proof": a Merkle path from the
acceptor's coin commitment (the leaf) to a trusted root that was set at DEX
initialization.

`verify_lock_proof` (the temporary-workaround implementation) recomputes the
root as a CHAINED poseidon hash, always placing the accumulator on the left:

    current = leaf
    for sibling in siblings:
        current = poseidon_hash([current, sibling])
    assert current == trusted_root

This is a standard binary Merkle path in the special case where the coin is at
position 0 (the left-most leaf), so it is always the left child at every level
and the "siblings" are exactly the right siblings along the path. This module
models that case exactly, and validates the round-trip: build tree -> derive
lock proof -> verify.

The Rust harness MUST build the coin tree with the acceptor's coin at index 0
and pass the right-siblings as the lock proof, and set the trusted root to the
tree root, for `verify_lock_proof` to succeed.
"""

from typing import List, Tuple

import halo2_math


def merkle_node(left: int, right: int) -> int:
    """Poseidon node hash — matches accept_swap verify_lock_proof."""
    return halo2_math.poseidon_hash([left, right])


def build_coin_tree(leaves: List[int]) -> Tuple[int, List[List[int]]]:
    """Build a binary Merkle tree over `leaves`.

    Returns `(root, levels)` where `levels[0]` is the leaf layer and
    `levels[-1]` is the single root. The tree is padded with zero leaves to the
    next power of two. The node hash is `merkle_node(left, right)` (no altitude
    domain separation, matching the temporary-workaround verifier).
    """
    n = len(leaves)
    # Pad to the next power of two with zero leaves.
    size = 1
    while size < n:
        size <<= 1
    padded = leaves + [0] * (size - n)

    levels: List[List[int]] = [padded]
    current = padded
    while len(current) > 1:
        nxt = [merkle_node(current[i], current[i + 1]) for i in range(0, len(current), 2)]
        levels.append(nxt)
        current = nxt

    return current[0], levels


def build_lock_proof(leaves: List[int], coin_index: int) -> List[int]:
    """Derive the lock-proof siblings for the coin at `coin_index`.

    Mirrors the verifier: for a coin at index 0 (always the left child), the
    siblings are the right siblings along the path, ordered leaf-to-root.
    Returns the list of siblings as field-element ints.
    """
    _, levels = build_coin_tree(leaves)
    siblings: List[int] = []
    idx = coin_index
    # Walk up from the leaf layer (levels[0]) to just below the root.
    for level in levels[:-1]:
        # The node's pair index: idx is the left child (even), sibling is idx+1;
        # or idx is the right child (odd), sibling is idx-1.
        if idx % 2 == 0:
            sibling = level[idx + 1]
        else:
            sibling = level[idx - 1]
        siblings.append(sibling)
        idx //= 2
    return siblings


def verify_lock_proof(trusted_root: int, leaf: int, siblings: List[int]) -> bool:
    """Recompute the root by chaining poseidon, matching the Rust verifier."""
    current = leaf
    for sibling in siblings:
        current = merkle_node(current, sibling)
    return current == trusted_root


def test_lock_proof():
    # The acceptor's coin (leaf) plus three unrelated coins.
    acceptor_coin = halo2_math.poseidon_hash([1, 42, 7])
    other = [halo2_math.poseidon_hash([2, i, i]) for i in range(1, 8)]

    leaves = [acceptor_coin] + other
    root, _ = build_coin_tree(leaves)

    # The acceptor's coin is at index 0 -> always the left child.
    proof = build_lock_proof(leaves, 0)
    assert verify_lock_proof(root, acceptor_coin, proof), "lock proof must verify"

    # A wrong leaf must NOT verify.
    assert not verify_lock_proof(root, other[0], proof), "wrong leaf must fail"

    # A tampered trusted root must NOT verify.
    assert not verify_lock_proof(root + 1, acceptor_coin, proof), "wrong root must fail"

    print("dex_lock_proof: lock proof round-trip PASSED")


if __name__ == "__main__":
    test_lock_proof()
