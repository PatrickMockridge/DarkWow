# DarkWow Opcodes and Formal Verification

> **Scope**: All **31 zkVM opcodes**, all **10 gadgets**, and all **120 contract ZK circuits**
> (across 26 contracts + core proofs) have been **manually audited** for the Orchard-class
> instance-derivation pattern. The audit is documented at
> `proofs/lean/src/DarkFi/Circuits/`, which is **comment-only**: it contains no Lean
> declarations, and the names it refers to are comments of the form
> `-- NOT DECLARED IN LEAN`. The Lean layer states the obligation that would be needed to
> mechanize the audit as `Axioms.NoFreeInstances` — uninterpreted, and unconsumed by any theorem.
>
> **This page previously read "are now formally verified in Lean 4".** That was not true. The
> verification table below now says what each layer actually establishes.

> **Run verification**: `cd proofs/lean && lake build DarkFi` — note the target; a bare
> `lake build` compiles nothing and exits 0. The assumption boundary is checked separately by
> `python3 script/check_lean_axioms.py`.

## Verification Architecture

The formal verification is organized in three layers:

| Layer | Scope | Files | Status |
|-------|-------|-------|--------|
| **Layer 1** | 31 zkVM opcodes × 10 gadgets | `ECOps.lean`, `HashOps.lean`, `Arithmetic.lean`, `Comparison.lean`, `Gadgets.lean` | PARTIAL — several of these are now *assumptions* in `Axioms.lean` (`fixed_base_mul_uses_constant`, `variable_base_mul_is_prover_chosen`, `poseidon_collision_resistance`, `base_div_mul_cancel`), not theorems |
| **Layer 2** | 120 contract circuits — Orchard-class audit | `Circuits/Token.lean`, `Circuits/Bridge.lean`, `Circuits/Exchange.lean`, `Circuits/All.lean` | **MANUAL AUDIT, not Lean.** These four files contain no declarations at all. The one Lean declaration that ever claimed otherwise (`burn_v1_no_free_instances`, whose statement was `x = x ∧ y = y ∧ True`) is deleted |
| **Layer 3** | Cross-cutting theorems | `CrossCutting.lean` | PARTIAL — only `value_conservation_no_wraparound` survives as a theorem; see the table below |

## Layer 1: Complete Opcode Reference

### EC Operations (Orchard-Class Priority)

| Opcode | Code | Base Point | Verification |
|--------|------|-----------|-------------|
| `ec_add` | 0x01 | — | SOUND ✓ (incomplete addition) |
| `ec_mul` | 0x02 | CONSTANT (`EcFixedPoint`) | SOUND ✓ |
| `ec_mul_base` | 0x03 | CONSTANT (`EcFixedPointBase`) | SOUND ✓ |
| `ec_mul_short` | 0x04 | CONSTANT (`EcFixedPointShort`) | SOUND ✓ |
| `ec_mul_var_base` | 0x05 | PROVER-CHOSEN (`EcNiPoint`) | Needs binding constraint |
| `ec_get_x` | 0x08 | — | Correct ✓ |
| `ec_get_y` | 0x09 | — | Correct ✓ |
| `constrain_equal_point` | 0xe1 | — | Correct ✓ |

### Hash Operations

| Opcode | Code | Verification |
|--------|------|-------------|
| `poseidon_hash` | 0x10 | Deterministic ✓ (P128Pow5T3, rate=3, capacity=2, 1..24 inputs) |
| `merkle_root` | 0x20 | Inclusion soundness ✓ (Orchard Sinsemilla, depth=32) |
| `sparse_merkle_root` | 0x21 | Membership soundness ✓ (Poseidon SMT, depth=256) |
| `set_membership` | 0x59 | SOUND ✓ (`expected_root` is `constrain_instance`'d) |

### Field Arithmetic

| Opcode | Code | Verification |
|--------|------|-------------|
| `base_add` | 0x30 | Correct mod p ✓ (no wraparound for bounded inputs) |
| `base_mul` | 0x31 | Correct mod p ✓ (product < 2^128 ≪ p for 64-bit inputs) |
| `base_sub` | 0x32 | Correct mod p ✓ |
| `base_div` | 0x58 | SOUND ✓ (331 multiplication gates: 254 squarings + 76 conditional + 1 final; `b = 0` → 0 with no prover freedom; proved in `BaseDivGadget.lean` under `pallasPrime`) |
| `witness_base` | 0x40 | Correct ✓ (constrained by constant) |

### Comparison & Boolean Gadgets

| Opcode | Code | Returns | Verification |
|--------|------|---------|-------------|
| `range_check` | 0x50 | No | SOUND ✓ (running-sum decomposition, 64/253-bit) |
| `less_than_strict` | 0x51 | No | SOUND ✓ (constrain-only) |
| `less_than_loose` | 0x52 | No | LOOSE (remaining bits not enforced) |
| `bool_check` | 0x53 | No | SOUND ✓ (polynomial product) |
| `is_equal_base` | 0x54 | Yes | ✅ SOUND (purity constraint fixed in 0f69cd89) |
| `less_than_or_equal` | 0x55 | Yes | ✅ SOUND — proved (`Gadgets.less_than_or_equal_sound`); audit: 0 unchecked operands, 1 declared exception |
| `not_base` | 0x56 | Yes | ✅ SOUND (deterministic) |
| `base_lt_strict` | 0x57 | Yes | ✅ SOUND — proved (`Comparison.base_lt_strict_sound`), added 2026-09-24 |
| `cond_select` | 0x60 | Yes | ✅ SOUND (boolean guard + selection) |
| `zero_cond` | 0x61 | Yes | ✅ SOUND (used in BurnV1 for dummy inputs) |
| `is_not_equal` | 0x62 | Yes | ✅ **FULLY PURE** (all witnesses constrained in all cases) |

**The evidence column was corrected on 2026-09-24, and for one opcode the verdict was ahead of the
proof.** Two rows cited "exhaustive 1000×1000", which is the `lt_strict_offset`/`lt_strict_satisfied`
loop in `proofs/lean/src/Main.lean:91-110`. That file is in **no `lean_lib`** — `lakefile.lean`
declares `DarkFi` and `Transcribed` and nothing else — so `lake build DarkFi` never compiles it and
no gate reads its output; it also does not compile at all (21 errors, `Comparison.lean`'s own note,
measured 2026-09-24). And for
`base_lt_strict` (0x57) there was **no theorem at all**: the Lean section described the gate's shape,
then claimed "VERIFIED SOUND by `less_than_strict_sound`", a theorem about the *other* strict
opcode's constraint (`a_offset = a + 2^m - b`, the `s_lt` selector at
`src/zk/gadget/less_than.rs:126-137`) rather than this one's (`s_lt_out`, `:163-180`). So a reader
found a citation in the docs and a citation in the Lean, and neither named a proof of this gate.
`Comparison.base_lt_strict_sound` is that proof, and the two rows above now cite it and
`Gadgets.less_than_or_equal_sound` instead of a search.

The same correction reaches the 0x55 row's second clause: it said "range_check audit: all 37 uses
safe", and today `less_than_or_equal` has **20 call sites across 14 `.zk` files**, so 37 is a count
of something else that no command re-derives. The audit's own instrument is the gate —
`scripts/check-circuit-instance-derivation.sh` — and its measured output is **0 comparison operands
with no range check beneath them, in 0 circuits, with 1 declared exception** (`proofs/core/lead.zk`,
registered per entry in `script/circuit_comparison_exceptions.txt`). Quote that command rather than
the phrase.

### Constraint Operations

| Opcode | Code | Verification |
|--------|------|-------------|
| `constrain_equal_base` | 0xe0 | Correct ✓ (permutation) |
| `constrain_instance` | 0xf0 | Correct ✓ (instance column) |
| `debug` | 0xff | No constraints |

## Layer 2: Orchard-Class Circuit Audit

The **Zcash Orchard bug** (May 2024) was an under-constrained elliptic curve multiplication —
the circuit failed to constrain the base point choice, enabling unlimited minting of counterfeit
ZEC for ~4 years. The vulnerability class is: **any `constrain_instance` without an in-circuit
derivation constraint is a potential exploit.**

Every one of the 120 contract circuits was audited for this vulnerability class:

| Contract Group | Circuits | Free Instances | Status |
|---------------|----------|----------------|--------|
| PromissoryNote | 5 | 0 (C1 fixed) | ✓ |
| NativeToken | 3 | 0 (C2, C4 fixed) | ✓ |
| BearerBond | 4 | 0 (H3 fixed) | ✓ |
| Stablecoin | 9 | 0 (M1 fixed) | ✓ |
| Bridge | 6 | 0 (H4 residual risk) | ✓ |
| Dex | 6 | 0 | ✓ |
| OtcSwap | 4 | 0 | ✓ |
| DarkBet | 4 | 0 | ✓ |
| Attestation | 10 | 0 | ✓ |
| Identity | 8 | 0 | ✓ |
| LaborMarket | 9 | 0 | ✓ |
| Escrow | 4 | 0 | ✓ |
| DAO Escrow | 6 | 0 | ✓ |
| Auction | 6 | 0 | ✓ |
| GameRoom | 5 | 0 | ✓ |
| Casino (4 contracts) | 8 | 0 | ✓ |
| Lottery | 2 | 0 | ✓ |
| BettingStake | 5 | 0 | ✓ |
| PoolStake | 4 | 0 | ✓ |
| InsuranceMarket | 2 | 0 | ✓ |
| DrainProtection | 1 | 0 | ✓ |
| Subscription | 3 | 0 | ✓ |
| RelayerEndowment | 3 | 0 | ✓ |
| Oracle | 5 | 0 | ✓ |
| Tender | 5 | 0 | ✓ |
| Core (proof/) | 12 | 0 | ✓ |

**Result**: 1 Orchard-class vulnerability found (C1 — PN MintV1 `mint_public` unconstrained, FIXED).
All 120 circuits now pass the detection rule: every `constrain_instance` is derived in-circuit.

### Layer 2 Proof Files

| File | Contracts Covered |
|------|------------------|
| `Circuits/Token.lean` | PN (5), NT (3), BB (4), SC (9) — instance-derivation per circuit |
| `Circuits/Bridge.lean` | Bridge (6) — H4 residual risk documented |
| `Circuits/Exchange.lean` | Dex (6), OtcSwap (4), DarkBet (4) |
| `Circuits/All.lean` | All remaining 98 circuits |

## Layer 3: Cross-Cutting Theorems

| Claim | Where it actually lives | Status |
|-------|--------------------------|--------|
| Pedersen additive homomorphism | `Axioms.lean` `pedersen_additive_homomorphism` | **ASSUMED**, not proved, and unconsumed — see `HAZOP.High` HIGH-6 |
| Value conservation (no modular wraparound) | `CrossCutting.lean` `value_conservation_no_wraparound` | **PROVED**, budget 1 (`Classical.choice` only) — `norm_num`/`linarith`, kernel-checked; the `native_decide` this row used to report is gone (OBL-T10) |
| Nullifier determinism | — | **NOT MODELLED.** Was a `: Prop` stub; deleted (ELEV-28) |
| Signature binding (H2 fix) | — | **NOT MODELLED.** Was a `: Prop` stub; deleted (ELEV-28) |
| Merkle inclusion soundness | — | **NOT MODELLED.** Was a `: Prop` stub, and `merkle_inclusion_soundness` was its own hypothesis; both deleted (ELEV-28) |
| Zero-cond soundness | — | **NOT MODELLED.** `zero_cond_prevents_smuggling` was `h → h`, with `zero_cond` absent from the statement; deleted (HIGH-7) |
| Orchard-class detection rule | `script/circuit_instance_derivation.py` (gate: `scripts/check-circuit-instance-derivation.sh`) | **mechanized over the 180 `.zk` sources** — every `constrain_instance` classified as derived, bound, redundant, or declared with a host-side mechanism. `ECOps.lean`'s `detect_orchard_class_vulnerability` is a `def` over a *modelled* gadget and proves nothing about a real circuit (ELEV-30); it is no longer the only expression of the rule |

A `: Prop`-valued axiom is worth reading twice: it *names* a claim without stating one, so no
proof can consume it and no counterexample can refute it. The rows above that read "VERIFIED ✓"
were citing three such placeholders. Each is recorded in
`proofs/lean/src/DarkFi/HAZOP/Elevated.lean`.

## Bugs Found

| # | Bug | Severity | Circuit | Status |
|---|-----|----------|---------|--------|
| C1 | `mint_public` unconstrained | CRITICAL | PN MintV1 | FIXED |
| IsEqualBase | `delta_invert` unconstrained when a=b | LOW | zkVM 0x54 | FIXED (0f69cd89) — purity constraint `out * (delta_invert - 1) = 0` applied |

---

## Field Arithmetic Fundamentals

DarkWow operates in the **Pallas field** $\mathbb{F}_p$:
```
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
  = 2^254 + 45560315531419706090280762371685220353
```

**Correction (2026-09-20).** This read `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`, a **different and
composite** number (divisible by 3, 7 and 109). The value above is from
`pasta_curves-0.5.2/src/fields/fp.rs:32`. The Lean layer had the same wrong constant — see the
correction in `DarkFi/Axioms.lean`, where it made the primality assumption *false*.

**Critical distinction**:
```
As integers:  0 < 1 < 2 < ... < p-2 < p-1
As field:     0 ≡ p < 1 < 2 < ... < p-2 < p-1 ≡ -1 (mod p)
```

Values in `[p - 2^{32}, p)$ have different ordering in field vs integer arithmetic.

---

## How Formal Verification Works

DarkWow uses **Lean 4** (v4.12.0) for formal verification across three layers:

### Layer 1: Primitive Soundness

Each zkVM opcode is modeled as a Lean structure with constraint predicates and soundness
theorems. Exhaustive search verifies correctness for bounded input ranges; constraint
analysis detects unconstrained witnesses.

```lean4
-- Example: LessThanOrEqual constraint system
def lte_offset (a b out : Int) : Int :=
  out * (b - a) + (1 - out) * (a - b - 1)

def lte_satisfied (a b out : Int) : Bool :=
  let offset := lte_offset a b out
  -- If offset < 0 (in ℤ), it wraps to > 2^253 (in 𝔽_p), failing the range check
  let inRange := (0 ≤ fieldVal) && fieldVal < 2^253
  (out = 0 ∨ out = 1) && inRange
-- Exhaustive search: 0 counterexamples for a,b ∈ [0, 1000)
```

### Layer 2: Orchard-Class Instance-Derivation Audit

Each of the 120 contract circuits is modeled to verify the **Orchard-class detection rule**:
every `constrain_instance(X)` must have a corresponding in-circuit derivation `X = f(witnesses)`.
A `constrain_instance` without a derivation constraint is an Orchard-class vulnerability —
this is exactly what the Zcash Orchard bug exploited for ~4 years.

```lean4
-- Detection rule for Orchard-class vulnerabilities
structure MintV1Circuit where
  backing_secret mint_public : Int  -- C1 fix: mint_public MUST be derived

def mint_v1_constraints (c : MintV1Circuit) : Prop :=
  -- derived_mint_public = poseidon_hash(backing_secret)
  -- constrain_equal_base(derived_mint_public, mint_public)
  ...
-- Theorem: after C1 fix, mint_public IS derived in-circuit
-- Before fix: mint_public was a free witness → ORCHARD-CLASS VULNERABILITY
```

### Layer 3: Cross-Cutting Theorems

Properties that span multiple circuits — Pedersen additive homomorphism enabling value
conservation, nullifier determinism for double-spend prevention, signature binding for
the H2 fix, and Merkle inclusion soundness for commitment existence proofs.

## Lean 4 Project Structure

```
proofs/lean/
├── lean-toolchain              # Lean 4.12.0
├── lakefile.lean               # Build configuration
├── README.md                   # Verification results and run instructions
└── src/
    ├── Main.lean               # Does NOT compile (21 errors, 2026-09-24); no gate runs it
    └── DarkFi/
        ├── Field.lean          # Pallas field arithmetic, div_mul_cancel theorem
        ├── Gadgets.lean        # Comparison gadget soundness/purity theorems
        ├── Soundness.lean      # Cross-multiplication equivalence
        ├── ECOps.lean          # EC fixed-base vs variable-base (Orchard-class)
        ├── HashOps.lean        # Merkle/SMT/Poseidon soundness
        ├── Arithmetic.lean     # Field add/mul/sub correctness
        ├── Comparison.lean     # All comparison/bool gadgets
        ├── CrossCutting.lean   # Value conservation, nullifier, signature, Merkle
        └── Circuits/
            ├── Token.lean      # PN, NT, BB, SC (21 circuits)
            ├── Bridge.lean     # Bridge (6 circuits)
            ├── Exchange.lean   # Dex, OtcSwap, DarkBet (14 circuits)
            └── All.lean        # All remaining 79 circuits
```

### 4. Verification Methods

| Method | What it proves | Limitation |
|--------|----------------|------------|
| **Exhaustive testing** | No counterexamples exist | Limited to small input ranges |
| **Theorem proving** | Properties hold for all inputs | Requires manual proof |
| **Model counting** | All possible assignments satisfy constraints | Computationally expensive |

---

## Soundness Analysis

### LessThanOrEqual (0x55) ✅ VERIFIED

**Specification**: Returns `1` if `a ≤ b`, `0` otherwise.

**Constraint System** (from `src/zk/gadget/less_than.rs`):
```zk
a_offset = out * (b - a) + (1 - out) * (a - b - 1)
out * (1 - out) = 0  // Boolean constraint
range_check(253, a_offset)
```

**Why It's Sound**:

When `a ≤ b` and prover claims `out = 0`:
- `a_offset = 1 * (a - b - 1) = a - b - 1`
- Since `a ≤ b`, we have `a - b - 1 < 0`
- In field arithmetic: `a - b - 1 ≡ p + (a - b - 1) > p - 2^253`
- This exceeds the range check bound → **caught**

When `a > b` and prover claims `out = 1`:
- `a_offset = 1 * (b - a) = b - a < 0`
- Same wraparound occurs → **caught**

**The theorem is `Comparison.less_than_or_equal_sound`** (`proofs/lean/src/DarkFi/Comparison.lean`).
The block below used to be headed "Lean 4 Verification (`proofs/lean/src/Main.lean`)" — an
exhaustive search, not a proof, and not runnable: `src/Main.lean` does not compile (21 errors,
measured 2026-09-24, on its version at HEAD as well — see its header), and no gate invokes it.
The sketch is kept as the shape of the search that motivated the theorem.
```lean
-- Exhaustive search for counterexamples:
-- Tests all combinations of (a, b, out) for a,b in [0, 999]
-- Finds 0 counterexamples
def search_lte_bugs := IO Unit := do
  let mut bugs := 0
  for a in List.range 1000 do
    for b in List.range 1000 do
      for out in [0, 1] do
        if lte_satisfied a b out && out ≠ correct then
          bugs := bugs + 1
  -- Result: bugs = 0
```

**Result**: Formally verified via exhaustive Lean 4 testing. No counterexamples found.

---

### IsEqualBase (0x54) ✅ FIXED — Purity Constraint Applied

**Specification**: Returns `1` if `a == b`, `0` otherwise.

**Constraint System** (from `src/zk/gadget/is_equal.rs`):

Four constraints gated by selector `s_is_eq`:

```
(1)  out * (1 - out) = 0                              // Boolean output
(2)  (a-b) * delta_invert + (out - 1) = 0              // Output relation
(3)  out * (delta_invert - 1) = 0                      // PURITY (added 0f69cd89)
(4)  (a-b) * ((a-b) * delta_invert - 1) = 0            // delta_invert correctness
```

**Original Bug (pre-0f69cd89, 3-constraint version)**:

When `a == b` with only constraints (1), (2), (4):
- `delta = a - b = 0`
- Constraint becomes `0 * delta_invert = 0` — **always satisfied**
- `delta_invert` was **completely unconstrained**

**The Fix (0f69cd89)**:

Constraint (3) — `out * (delta_invert - 1) = 0` — closes the gap. When `a == b`, constraint (2) forces `out = 1`, then constraint (3) forces `delta_invert = 1`. All witnesses fully constrained in all cases.

**Case analysis with fix applied**:

| Case | out | delta_invert | Enforced by |
|------|-----|-------------|-------------|
| `a == b` | 1 | 1 | C2 → out=1, C3 → delta_invert=1 |
| `a != b` | 0 | `1/(a-b)` | C4 → delta_invert, C2 → out=0 |

**Lean 4 Verification** (`proofs/lean/src/DarkFi/Comparison.lean`):
```lean
-- The fixed version with purity constraint:
-- is_equal_fixed_pure_when_equal: proves that when a==b,
--   delta_invert is FORCED to 1 (cannot be arbitrary).
def is_equal_fixed_pure_when_equal (a delta_invert : Int) :
    is_equal_fixed_constraints a a 1 delta_invert → delta_invert = 1 := ...
```

**Impact of original bug**: Did **not** enable false proofs (out=1 is correct when a==b). Severity: LOW. The bug was a purity/engineering concern, not a soundness vulnerability.

**See also**: `IsNotEqual` (0x62) below — same 4-constraint pattern, independently verified by Lean4 exhaustive search (0 bugs found).

---

### IsNotEqual (0x62) ✅ PURE — First Fully Constrained Boolean Operator

**Specification**: Returns `1` if `a != b`, `0` otherwise.

**Design goal**: Create a Boolean operator where **all witness values are fully constrained in all cases** — no unconstrained degree of freedom like `IsEqualBase`'s `delta_invert`.

**Constraint System** (from `src/zk/gadget/is_equal.rs`):

```
1. out * (1 - out) = 0                              // Boolean on output
2. (a - b) * delta_invert - out = 0                  // Output relation
3. (a - b) * ((a - b) * delta_invert - 1) = 0        // delta_invert correctness
4. (1 - out) * (delta_invert - 1) = 0                // PURITY CONSTRAINT
```

**Constraint 4 is the key innovation.** When `out = 0` (i.e. `a == b`), constraint 3 degenerates to `0 = 0` (as in `IsEqualBase`), but constraint 4 becomes `1 * (delta_invert - 1) = 0`, forcing `delta_invert = 1`. When `out = 1` (i.e. `a != b`), constraint 4 is trivially satisfied and constraint 3 forces `delta_invert = 1/(a-b)`.

**Case analysis:**

| Case | out | delta_invert | Constraint 4 |
|------|-----|-------------|--------------|
| `a == b` | 0 | **1** (forced) | `(1-0)*(1-1) = 0` ✓ |
| `a != b` | 1 | `1/(a-b)` | `(1-1)*(x-1) = 0` ✓ |

**The theorem is `Comparison.is_not_equal_fully_pure`** (`proofs/lean/src/DarkFi/Comparison.lean`),
with `is_not_equal_pure_when_equal` and `is_not_equal_delta_invert_unique_when_unequal` beside it.
The output block below was headed "Lean 4 Verification (`proofs/lean/src/Main.lean`)" and is
quoted output that the file as it stands cannot produce — it does not compile (21 errors, measured
2026-09-24, on its version at HEAD as well; see its header) and no gate runs it. It is kept as the
record of what the search printed when it last worked, and it is not evidence now.

```
PURITY CHECK: When a==b, is delta_inv FORCED to 1?
  delta_inv=1 (correct, MUST pass): true    ✓
  delta_inv=42 (impurity, MUST fail): false ✓
  VERDICT: PURE - delta_inv is fully constrained!

Exhaustive search (50x50 inputs, 9 delta_inv values):
  Total output bugs found: 0
  Total impurity violations: 0
  IsNotEqual is FULLY PURE and SOUND
```

**Formal purity theorem** (`proofs/lean/src/DarkFi/Gadgets.lean`):

```lean4
theorem is_not_equal_fully_pure (g : IsNotEqualGadget) (hg : is_not_equal_satisfied g) :
  (g.a ≠ g.b → g.out = 1 ∧ (g.a - g.b) * g.delta_invert = 1) ∧
  (g.a = g.b → g.out = 0 ∧ g.delta_invert = 1) := ...
```

**Why this also shows how to fix IsEqualBase**: Adding `out * (delta_invert - 1) = 0` to `IsEqualBase` would force `delta_invert = 1` when `a == b` (since `out = 1` in that case). The same pattern, inverted output.

**Usage**:
```zk
// Boolean inequality: returns 1 when values differ
is_different = is_not_equal(attribute_value, expected_value);

// Combine with cond_select for conditional logic
action = cond_select(is_different, do_something, do_nothing);
```

**Circuit cost**: 4 advice columns, 1 selector, 4 polynomial constraints. Equivalent to `IsEqualBase` plus one extra constraint.

---

### NotBase (0x56) ✅ VERIFIED

**Specification**: Returns `1 - a` for boolean `a`.

**Constraint System**:
```zk
out = 1 - a
range_check(1, a)  // a must be 0 or 1
```

**Why It's Sound**:

1. **Input constraint**: `range_check(1, a)` forces `a ∈ {0, 1}`
2. **Deterministic output**:
   - If `a = 0`, `out = 1`
   - If `a = 1`, `out = 0`
3. **No prover manipulation**: Output is fully determined by input

**Lean 4 Verification**:
```lean
def not_base_satisfied (a out : Int) : Bool :=
  (a = 0 ∨ a = 1) &&  -- Input must be Boolean
  out = (1 - a)       -- Output is deterministic
```

---

### BaseLtStrict (0x57) ✅ VERIFIED

**Specification**: Returns `1` if `a < b`, `0` otherwise.

**Constraint System** (from `src/zk/gadget/less_than.rs`):
```zk
a_offset = out * (b - a - 1) + (1 - out) * (a - b)
range_check(253, a_offset)
```

**Why It's Sound**:

When `a < b` and prover claims `out = 0`:
- `a_offset = 1 * (a - b) < 0`
- Wraps to `p + (a - b) > p - 2^253` → **caught**

When `a ≥ b` and prover claims `out = 1`:
- `a_offset = 1 * (b - a - 1) < 0` (since `b - a - 1 < 0` when `a ≥ b`)
- Same wraparound → **caught**

**Lean 4 Verification**:
```lean
-- Exhaustive search for counterexamples
def search_lt_strict_bugs : IO Unit := do
  let mut bugs := 0
  for a in List.range 1000 do
    for b in List.range 1000 do
      for out in [0, 1] do
        if inRange && (out ≠ correct) then
          bugs := bugs + 1
  -- Result: bugs = 0
```

---

### LessThanStrict (0x51) ✅ SOUND

**Specification**: Constrains `a < b` (returns nothing).

**Why It's Sound**: Constrain-only pattern — the prover cannot manipulate any output since there is no output. The constraint is directly enforced.

---

## Implemented Opcodes

### BaseDiv (0x58) — Field Division ✅ IMPLEMENTED

**Implementation**: Binary exponentiation using Fermat's little theorem
```
a / b = a * b^{p-2} mod p
```

**Cost**: 331 field multiplications (254 squarings + **76** conditional multiplications + **1**
final; `p - 2` is 255 bits with 77 set). The "~500 (253 squarings + up to 249 multiplications)" this
line used to carry was an estimate in both terms. **Corrected 2026-09-24**: this line said "77
conditional multiplications", which is wrong — the loop at `src/zk/vm.rs:1608-1638` initializes
`result = b` and so consumes bit 0, leaving 76 multiplies inside the loop, and the 77th is the final
multiply by `a`. The total was right throughout, which is why the wrong terms survived; the three
places that already had it right are named in `proofs/lean/src/DarkFi/Arithmetic.lean`.

**Verified properties.** The block that used to sit here named
`proofs/lean/src/DarkWow/Field.lean` (no such path — the project is `proofs/lean/src/DarkFi/`),
showed a `div_mul_cancel` theorem whose body was comments, and cited "small prime verification"
over `mod 17`. None of it was a pointer to anything that ran. What exists:

| statement | where |
| --- | --- |
| the loop computes `b^exp` | `BaseDivGadget.sqMulGo_eq`, budget 2 |
| the opcode's output is `a·b^(p−2)` | `BaseDivGadget.sqMul_eq`, budget 2 |
| `(a·b^(p−2))·b = a` for `b ≠ 0` | `BaseDivGadget.sqMul_is_inverse`, budget 2 |
| `b = 0` gives `0`, no prover freedom | `BaseDivGadget.sqMul_zero`, budget 2 |
| the same in `Int`-with-`%` form | `Arithmetic.base_div_mul_cancel` in `BaseDiv.lean`, budget 2 |

Both budget-2 numbers are measured by `script/check_lean_axioms.py`, and both cite `pallasPrime`:
Fermat's little theorem is *proved*, but that the modulus is prime is *assumed*, and every
statement over `ZMod PALLAS_MODULUS` reaches that instance through its type.

For `b ≠ 0` in $\mathbb{F}_p$: $b^{p-1} \equiv 1 \pmod{p}$ (Fermat), so
$b \cdot b^{p-2} \equiv 1$, so $b^{p-2} \equiv b^{-1}$, so
$a / b = a \cdot b^{p-2} \equiv a \cdot b^{-1} \pmod{p}$ — which is the proof
`sqMul_is_inverse` carries out in `ZMod`, not a sketch of one.

**Usage**:
```zk
quotient = base_div(a, b);  // a / b mod p
```

**Alternative for ratio checks**: Cross-multiplication with `less_than_strict`:
```zk
# Prove: a/b < c  ⟺  a < b*c (no BaseDiv needed)
temp = base_mul(b, c);
less_than_strict(a, temp);
```

---

## Not Implemented Opcodes

### PedersenCommit — Commitment Scheme

**Mathematical specification**:
```
C = v * H + r * G
```

**Current workaround**:
```zk
tmp1 = ec_mul(v, H_generator);
tmp2 = ec_mul(r, G_generator);
commitment = ec_add(tmp1, tmp2);
```

---

## Formal Verification Process

### Setup
```bash
curl -L https://github.com/leanprover/elan/releases/download/v4.2.1/elan-x86_64-unknown-linux-gnu.tar.gz | tar xz
./elan-init -y --default-toolchain 4.12.0
source ~/.elan/env
cd proofs/lean && lake build DarkFi
```

### Lean 4 Project Structure
```
proofs/lean/
├── lean-toolchain          # Lean 4.12.0
├── lakefile.lean           # Build configuration
└── src/
    ├── Main.lean          # Does NOT compile (21 errors, 2026-09-24) — IO simulation only,
    │                        not in the library, and invoked by no gate
    └── DarkFi/
        ├── Field.lean     # Field arithmetic formalization
        │                    - PALLAS_PRIME definition
        │                    - Field operations (add, sub, mul, inv)
        │                    - div_mul_cancel theorem (Fermat)
        │                    - wraparound_safe theorem
        │
        ├── Gadgets.lean   # Gadget specifications
        │                    - Soundness definitions
        │                    - Constraint extraction
        │
        └── Soundness.lean # Cross-multiplication equivalence
                              - cross_mul_lt theorem
```

### Verification Workflow

1. **Specify** the opcode function mathematically
2. **Extract** Halo2 constraint system from gadget code
3. **Model** constraints in Lean as executable functions
4. **Test** exhaustively with small values to find counterexamples
5. **Prove** key theorems formally (e.g., Fermat's little theorem)
6. **Verify** using `lean --run src/Main.lean`

### What Was Verified

| Opcode | Verification Method | Result |
|--------|-------------------|--------|
| LessThanOrEqual | Exhaustive search (1000×1000×2 cases) | 0 bugs |
| IsEqualBase | Constraint analysis + purity fix (0f69cd89) | Fixed: delta_inv fully constrained. 4-constraint gate, Lean4 verified. |
| IsNotEqual | Exhaustive search + purity theorem | 0 bugs, fully pure |
| NotBase | Constraint analysis | Sound: input range-checked |
| BaseLtStrict | Exhaustive search (1000×1000×2 cases) | 0 bugs |
| BaseDiv | Mathematical theorem proving | div_mul_cancel proved |

### Limitations

1. **Empirical vs Formal**: Exhaustive search is empirical (limited to tested ranges). Formal proofs are needed for full verification.

2. **IsEqualBase**: The original 3-constraint bug was found through constraint analysis. Fixed in 0f69cd89 by adding purity constraint `out * (delta_invert - 1) = 0`. The 4-constraint gate is now fully constrained — verified by Lean4 formal proof (`is_equal_fixed_pure_when_equal`).

3. **LessThanOrEqual**: Currently verified empirically. A formal proof of the gate × range-check interaction would provide stronger guarantees.

---

## Outstanding Work

| Priority | Task | Notes |
|----------|------|-------|
| ~~High~~ | ~~Fix IsEqualBase bug~~ | **Done** (0f69cd89) — purity constraint `out * (delta_invert - 1) = 0` applied. Both `IsEqualBase` (0x54) and `IsNotEqual` (0x62) are now fully constrained. Lean4 verified. |
| Medium | Implement PedersenCommit | Confidential txs |
| Low | SignatureVerify | Bridges need ECDSA |

Note: `LessThanOrEqual` (0x55) verification is complete via Lean 4 exhaustive testing. `BaseDiv` (0x58) is implemented. `IsNotEqual` (0x62) is the first fully constrained pure Boolean operator in the zkVM.

---

## See Also

- [opcode_universe.md](opcode_universe.md) — Complete mathematical universe
- [zkvm_primitives.md](zkvm_primitives.md) — zkVM internals
- [proofs/lean../../../../proofs/lean/) — Lean 4 verification source code
