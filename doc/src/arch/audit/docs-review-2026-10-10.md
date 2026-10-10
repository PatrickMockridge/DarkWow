# Documentation-vs-Code Review — Findings Ledger

**Started 2026-10-10.** This is the intake ledger for a documentation review against the code it
describes. It is **not a home**: remediation moves each confirmed finding to where it belongs (a doc
edit, a new `OBL-*` row in [the register](../verification-hazop.md), or a new gate) and stamps the
row's `promoted` cell. A finding is never in two homes.

## Why this exists

The repository instruments its **code** heavily (≈30 gates pin circuit ↔ entrypoint ↔ client ↔
manifest) and its **docs** *structurally* (`scripts/check-doc-index.sh`: SUMMARY ↔ tree, links,
include anchors, every `OBL-*` citation resolves). Nothing read a doc's **prose claim about a code
fact** and compared it to the code. This campaign is the instrument that was missing.

## Method

Each finding names **both sides** — the doc claim and the code reality, each with `path:line` — and
is a **hypothesis** until a reader opens the code (`R6`). No agent in this campaign writes or fixes
anything; remediation is a separate exercise. Mechanical classes are read by scripts, which ship a
**negative control** (`R8`); semantic classes are read by agents.

| Class | Meaning | Remediation home |
|---|---|---|
| **DOC-STALE** | doc asserts a code fact that is false; code is truth | edit the doc where load-bearing (`R11`) |
| **CODE-VIOLATES-ITS-OWN-SPEC** | code does not enforce a rule a normative doc states | new `OBL-*` row; safety-class → `safety.md` |
| **AMBIGUOUS-SPEC** | normative text admits two readings | restate the clause |
| **REGISTER-ROW-UNSOUND** | a register row's proposition does not imply its property | amend the row in place |
| **INSTRUMENT-GAP** | a claim class no gate catches | build the detector (+ control) |

Severity uses the register's legend (**C** mints/destroys/steals consensus value, **H** breaks a
stated property, **M** defence-in-depth or liveness). A documentation defect caps at **M**: it cannot
mint value — unless it states a rule the code fails to enforce, which is reclassified.

## Findings

| ID | Class | Sev | Doc (claim) | Code (reality) | Status | Promoted |
|---|---|---|---|---|---|---|
| F-001 | DOC-STALE | M | `contract/attestation.md:261` — "All 10 circuits compiled" | `src/contract/attestation/proof/` holds **12** (`expire_attestation`, `revoke_attestation` added) | proposed | |
| F-002 | DOC-STALE | M | `contract/escrow.md:320` — "All 4 circuits"; trace table omits `cancel` | `src/contract/escrow/proof/` holds **5** | proposed | |
| F-003 | DOC-STALE | M | `contract/game_room.md:214` — "All 5 circuits"; table lists 5 | `src/contract/game_room/proof/` holds **12**; table omits 7 (`call`, `close_pot`, `contribute_entropy`, `create_pot`, `fold`, `raise`, `withdraw`) | proposed | |
| F-004 | DOC-STALE | M | `contract/slot.md` — circuit table lists 2 | `src/contract/slot/proof/` holds **3**; `reveal_spin` omitted | proposed | |
| F-005 | DOC-STALE | M | `contract/stablecoin.md:193` — "All 9 circuits"; table lists 9 | `src/contract/stablecoin/proof/` holds **10**; `update_config` omitted | proposed | |
| F-006 | DOC-STALE | M | `arch/verification-hazop.md:461` — "**Inventory: 180 circuits.**" | the Orchard-class gate walks **178** (166 contract + `proofs/core/` 10 + `bin/darkirc/proof/` 2); the same file prints 178 at :820 | proposed | |
| F-007 | DOC-STALE | M | `for-researchers.md:14` — "**228 ZK circuits** across 32 contracts" | `src/contract/*/proof/*.zk` holds **166** (178 walked) | proposed | |

## Coverage observations (not findings)

These are surfaces the instruments see but do not judge; they feed the agent pass:

- **No circuit table.** Fourteen contract docs have a `proof/` directory but no circuit table at all:
  `baccarat`, `bearer_bond`, `betting_stake`, `darkbet_exchange`, `darktoshi_dice`, `drain_protection`,
  `insurance_market`, `lottery`, `native_token`, `pool_stake`, `relayer_endowment`, `roulette`,
  `subscription`, `tender`. A doc that documents no circuits cannot drift on that axis, but it also
  cannot be checked — a reader copying the surface has no list.
- **Incomplete function tables.** Six docs' Function tables list fewer functions than the manifest
  declares: `bearer_bond` (9/10, missing `register_series`), `bridge` (1/3), `game_room` (11/12,
  missing `create_pot`), `labor_market` (3/15 — documents its functions in a state-machine diagram),
  `native_token` (6/8), `stablecoin` (10/12). Not by itself drift (a diagram may be the list), which is
  why the gate reports it as a coverage note rather than a failure.

## Instruments built

Three sound, negative-controlled detectors (`R8`), each committed on `linear-master`:

| Detector | Pins | Commit |
|---|---|---|
| `scripts/check-contract-doc-circuits.sh` | a contract doc's circuit table + "All N circuits" ↔ `proof/*.zk` | `6ee5fbc447` |
| `scripts/check-contract-doc-functions.sh` | a doc's function/opcode table ↔ `manifest.toml [[functions]]` | `ae6e869ccb` |
| `scripts/check-doc-canonical-counts.sh` (+ `script/doc_count_exceptions.txt`) | the tree-population counts ↔ the tree | `7b2b2d6dbd` |

## What is not established (R7)

- The detectors judge the **countable** surface (circuit sets, function codes, population figures).
  They do not read a doc's **state machine**, **struct fields**, **cross-contract call flows**, or
  whether a normative clause **says what a referrer claims** — those are the agent pass's work and it
  has not run.
- A blanket "every *N* circuits" scanner was **built and rejected as unsound**: docs legitimately
  state dozens of local counts ("2 circuits" for a feature), and the draft produced more than ten
  false positives. The count gate fires only on the global phrasings; the residual class is left to
  the agent pass rather than closed by pretending.
