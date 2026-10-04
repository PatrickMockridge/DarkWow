"""The tx pair is the last two `constrain_instance` targets of a circuit.

`OBL-C198` gave the node one job: recompute `poseidon_hash(DOMAIN_TX_BINDING,
tx_commitment, tx_nonce)` for the enclosing transaction and require it to equal the proof's
published `tx_binding`. The node reads that value from the *end* of the public-input vector —
`pubvals[len-2]` and `pubvals[len-1]` — because there is no per-circuit index map anywhere in
`src/linear/`, and inventing one would mean the node carried a table that has to be kept in
step with 166 circuits by hand.

That position is therefore a shape, not a name mapping, and it is the one rule in this area
that admits no heuristic: a circuit's last two instances are `tx_binding` then `tx_nonce`, or
the circuit is wrong. `scripts/check-circuit-metadata-alignment.sh` compares instance order to
the metadata arm only as a WARN, because the circuit-variable-to-Rust-expression mapping is a
guess. For this rule there is nothing to guess.

Measured 2026-10-04 over the whole corpus (166 circuits in `src/contract/*/proof/*.zk`):

    * 136 place the pair last and conform
    * 30 do not, across 13 contracts — escrow 5, game_room 4, pool_stake 4, dao_escrow 3,
      relayer_endowment 3, subscription 3, insurance_market 2, and one each in `bearer_bond`,
      `darktoshi_dice`, `promissory_note`, `roulette`, `slot`, `stablecoin`
    * 0 have no pair at all — the rule is total, every circuit instances both

The 30 are the census in `circuit_tx_pair_last_exceptions.txt`, each naming the register row
that schedules its reorder. Moving an instance renumbers three vectors at once — the circuit's
constraints, the metadata arm's push order, and the client's `to_vec` — so the reorder is a
per-contract unit and not a sweep. A circuit added in the middle shape fails here immediately,
which is the whole point: this is the check that would have caught `mint.zk` sitting at
positions 8 and 9 with `total_pin` after it.

WHAT A PASS MEANS, and what it does not. That every circuit walked instances `tx_binding`
immediately before `tx_nonce`, both last, or is declared below. It is a structural check over
source text: it does not know whether the *derivation* feeding `tx_binding` is the host's
(often it is, via `tx_binding_of(tx_nonce)`), it does not know whether the metadata arm agrees,
and it cannot see a circuit whose pair is correct but whose metadata arm pushes something else.
`scripts/check-circuit-metadata-alignment.sh` and the node's own stage 4 are those checks.
"""

import os
import re

# `circuit "Name" {` at the start of a line. The body runs to the next such block.
_CIRCUIT_RE = re.compile(r'\ncircuit\s+"([^"]+)"\s*\{')
_INSTANCE_RE = re.compile(r"constrain_instance\(([^)]*)\)")

PAIR = ("tx_binding", "tx_nonce")


class Finding:
    """One circuit whose pair is not the last two instances."""

    def __init__(self, path, name, kind, detail, line):
        self.path = path
        self.name = name
        self.kind = kind
        self.detail = detail
        self.line = line


def _strip_comments(text):
    """`#` to end of line, keeping the line count so offsets still map to real lines.

    The dead-values checker's first bug was a comment glued to the statement under it; every
    scanner here strips first for the same reason.
    """
    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())


def check_file(path, repo):
    """Every circuit in `path` whose last two `constrain_instance` targets are not the pair."""
    rel = os.path.relpath(path, repo)
    with open(path, errors="replace") as fh:
        text = _strip_comments(fh.read())

    findings = []
    starts = list(_CIRCUIT_RE.finditer(text))
    for idx, match in enumerate(starts):
        name = match.group(1)
        body_start = match.end()
        body_end = starts[idx + 1].start() if idx + 1 < len(starts) else len(text)
        body = text[body_start:body_end]

        insts = [
            (m.group(1).strip(), body_start + m.start(1))
            for m in _INSTANCE_RE.finditer(body)
        ]

        def line_of(offset):
            return text.count("\n", 0, offset) + 1

        names = [n for n, _ in insts]

        if len(names) >= 2 and names[-2:] == list(PAIR):
            continue

        pos = [k for k, n in enumerate(names) if n in PAIR]

        if not pos:
            findings.append(
                Finding(
                    rel,
                    name,
                    "PAIR-MISSING",
                    "the circuit instances neither tx_binding nor tx_nonce; the node reads "
                    "both from the end of the vector",
                    line_of(insts[-1][1]) if insts else line_of(match.start()),
                )
            )
        elif len(names) < 2:
            findings.append(
                Finding(
                    rel,
                    name,
                    "PAIR-NOT-LAST",
                    f"the circuit has {len(names)} instance(s), so the pair cannot be the last two",
                    line_of(insts[-1][1]),
                )
            )
        else:
            findings.append(
                Finding(
                    rel,
                    name,
                    "PAIR-NOT-LAST",
                    f"pair at position(s) {pos} of {len(names)}; the last two instances are "
                    f"{names[-2]!r}, {names[-1]!r}",
                    line_of(insts[pos[-1]][1]),
                )
            )

    return findings
