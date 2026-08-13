"""Dex lock-proof model — the dex-money Merkle lock-proof integration.

Spec: `src/contract/dex/src/entrypoint/{create_swap,accept_swap}.rs`
`verify_lock_proof`.

The dex `accept_swap`/`create_swap` verify that a party owns a coin in the money
contract's coin tree by checking a "lock proof": a Merkle path from the party's
coin commitment (the leaf) to a trusted root set at DEX initialization.

The verifier recomputes the root POSITION-AWARE: at each level the accumulator
is the left or right child depending on the leaf position's bit:

    current = leaf
    idx = position
    for sibling in siblings:
        current = H(sibling, current) if idx & 1 else H(current, sibling)
        idx >>= 1
    assert current == trusted_root

A DEX swap has TWO coins (proposer + acceptor), so the tree must hold both, and
each coin's path carries a distinct position (proposer at 0, acceptor at 1).
The node hash `H` is poseidon (`poseidon_hash([left, right])`), matching the
temporary-workaround verifier (NOT the on-chain Sinsemilla coin tree).

The Rust harness MUST build this same two-coin poseidon tree and pass each
party's (siblings, position) for the verifier to succeed.
"""

from typing import List, Tuple

import halo2_math


def merkle_node(left: int, right: int) -> int:
    """Poseidon node hash — matches the dex verify_lock_proof."""
    return halo2_math.poseidon_hash([left, right])


def build_coin_tree(leaves: List[int]) -> Tuple[int, List[List[int]]]:
    """Build a binary Merkle tree over `leaves`.

    Returns `(root, levels)` where `levels[0]` is the leaf layer (zero-padded to
    the next power of two) and `levels[-1]` is the single root.
    """
    n = len(leaves)
    size = 1
    while size < n:
        size <<= 1
    padded = leaves + [0] * (size - n)

    levels: List[List[int]] = [padded]
    current = padded
    while len(current) > 1:
        current = [merkle_node(current[i], current[i + 1]) for i in range(0, len(current), 2)]
        levels.append(current)

    return current[0], levels


def build_lock_proof(leaves: List[int], coin_index: int) -> List[int]:
    """Derive the lock-proof siblings for the coin at `coin_index`, leaf-to-root."""
    _, levels = build_coin_tree(leaves)
    siblings: List[int] = []
    idx = coin_index
    for level in levels[:-1]:
        sibling = level[idx + 1] if idx % 2 == 0 else level[idx - 1]
        siblings.append(sibling)
        idx //= 2
    return siblings


def verify_lock_proof(trusted_root: int, leaf: int, siblings: List[int], position: int) -> bool:
    """Recompute the root position-aware, matching the Rust verifier."""
    current = leaf
    idx = position
    for sibling in siblings:
        current = merkle_node(sibling, current) if idx & 1 else merkle_node(current, sibling)
        idx >>= 1
    return current == trusted_root


def test_lock_proof():
    proposer_coin = halo2_math.poseidon_hash([1, 42, 7])
    acceptor_coin = halo2_math.poseidon_hash([2, 43, 8])
    other = [halo2_math.poseidon_hash([2, i, i]) for i in range(1, 7)]

    leaves = [proposer_coin, acceptor_coin] + other
    root, _ = build_coin_tree(leaves)

    # Proposer at index 0, acceptor at index 1 — both must verify.
    for idx, coin in [(0, proposer_coin), (1, acceptor_coin)]:
        proof = build_lock_proof(leaves, idx)
        assert verify_lock_proof(root, coin, proof, idx), f"coin {idx} must verify"

    # Wrong leaf, wrong root, and wrong position must all fail.
    assert not verify_lock_proof(root, other[0], build_lock_proof(leaves, 0), 0)
    assert not verify_lock_proof(root + 1, proposer_coin, build_lock_proof(leaves, 0), 0)
    assert not verify_lock_proof(root, proposer_coin, build_lock_proof(leaves, 0), 1)

    print("dex_lock_proof: two-coin position-aware round-trip PASSED")


if __name__ == "__main__":
    test_lock_proof()
