/-
# The ρ-calculus, as a definition rather than an analogy

`type-system.md` §0 states the ρ-calculus primitives in a table and maps each to a blockchain
operation — channel is a contract instance, name is a capability, output is posting a commitment,
input is AEAD discovery, restriction is per-instance key derivation, replication is the nullifier
marker set. Nothing in Lean corresponded to that table, and this module is the beginning of the
correspondence: the syntax and the reflection, with the two equations that make `quote`/`eval` a
*definition* rather than a metaphor.

## The design decision, and why it is this one

The reflective (Meredith–Radestock) ρ-calculus has one property that the π-calculus lacks: **names
are processes and processes are names**. The naive encoding gives `Proc` and `Name` as mutually
recursive inductives, so that `Name.bang : Proc → Name` and `Proc.quote : Name → Proc` both exist —
and that is over-encoded. It makes `quote` a *constructor* and leaves `eval` to be defined as its
inverse, at which point `eval (quote x) = x` holds by `rfl` but `quote (eval p) = p` is false for
every `p` that is not already of the form `quote x`, and the repair is a quotient by the equation —
`Quotient` machinery, for a fact the syntax did not need to lose.

Stating the property directly removes the quotient. If names *are* processes, then there is no
`Name` type to be mutually recursive with: `Proc` alone carries everything, a name is a `Proc` of the
form `⌈P⌉`, and:

* `quote` is the constructor `bang` (⌈P⌉ — P as a name, which is also a process);
* `eval` (＊x — x as a process) is a **function**, not a constructor, because in ρ every name is a
  quote. `＊⌈P⌉ = P` is then definitional.
* `1`, the nil name, is `⌈0⌉` = `bang nil`, so it needs no constructor of its own — which is the
  standard presentation and the reason `eval`'s fallback case is `nil` rather than undefined.

Both equations therefore hold, and they hold for the honest reason:

    eval_quote          ＊⌈P⌉ = P        -- by rfl, for every P
    quote_eval_of_name  ⌈＊x⌉ = x        -- for every x that IS a name, and this guard is not a
                                        -- convenience: it is false for `out x y`, which is a
                                        -- process and not a name

That second row is the point of the module. `⌈＊x⌉ = x` is true exactly on `isName`, and stating it
unguarded would be stating something false — which is the failure mode this corpus has repeatedly
found (a "theorem" whose statement is true of nothing).

## Scope

Core Lean only — no Mathlib, no `Finset`, following `Combinatorial/NullifierStorage.lean`. The
concurrency and bisimulation layers build on this and are deliberately not in this file: this module
fixes the *encoding*, and the encoding is what everything else quantifies over. The structural
congruence is `Semantics/Congruence.lean` and the labelled transition system is `Semantics/LTS.lean`,
which is why nothing here mentions reduction.
-/

import DarkFi.AxiomBudget

namespace DarkFi.Semantics

/-! ==========================================================================
   Part 1 — Processes, with names folded in
   ========================================================================== -/

/-- Processes of the ρ-calculus. There is no separate `Name` type: a name is a process of the form
    `bang P`, which is what "names are processes and processes are names" means when it is written
    down instead of described.

    The constructors are `type-system.md` §0's table, one for one:

    | this file | §0 notation | §0's blockchain reading |
    |---|---|---|
    | `nil`   | `0`        | the stopped process |
    | `bang`  | `quote(x)` | code as data — the deployed contract bytes |
    | `out`   | `x!(y)`    | posting a commitment |
    | `inp`   | `x?(y).P`  | discovering a commitment by AEAD decryption |
    | `nu`    | `νx.P`     | deriving a per-instance key |
    | `rep`   | `!P`       | the nullifier marker set |
    | `par`   | `P \| Q`   | two calls in one block |

    `DecidableEq` is derived, and it is here rather than in `Substitution.lean` because it is part of
    how the type presents itself rather than a tool one module happens to need: substitution has to
    recognise the term it replaces, and in this calculus names are arbitrary terms, so that test is
    term equality. The instance is structural — it decides by the constructors and uses nothing beyond
    them — so it adds no assumption to anything. -/
inductive Proc : Type where
  /-- The stopped process, `0`. -/
  | nil : Proc
  /-- Reflection, `⌈P⌉`: the process `P` taken as a name. `bang nil` is the nil name `1`. -/
  | bang : Proc → Proc
  /-- Output, `x!(y)`: send the name `y` on the channel `x`. -/
  | out : Proc → Proc → Proc
  /-- Input, `x?(y).P`: receive a name on `x`, bind it as `y`, then behave as `P`. -/
  | inp : Proc → Proc → Proc → Proc
  /-- Restriction, `νx.P`: create a fresh name `x` scoped over `P`. -/
  | nu : Proc → Proc → Proc
  /-- Replication, `!P`. -/
  | rep : Proc → Proc
  /-- Parallel composition, `P | Q`. -/
  | par : Proc → Proc → Proc
deriving DecidableEq

/-! ==========================================================================
   Part 2 — Dereference, and the two equations
   ========================================================================== -/

/-- Dereference, `＊x`: the process named by `x`.

    Total, and total by the standard reading rather than by a fallback: `＊⌈P⌉ = P`, and for anything
    that is not a quote the value is `0`. In ρ the only non-quote case is the nil name `1 = ⌈0⌉`,
    where `＊1 = 0` — so the `nil` branch is the definition of `＊1`, not a safety net. Writing it as
    a `match` rather than as a constructor is what lets `eval_quote` be `rfl`. -/
def eval : Proc → Proc
  | .bang P => P
  | _ => .nil

/-- `x` is a name: a process of the form `⌈P⌉`. This is the guard `quote_eval_of_name` needs, and it
    is the honest place to put the side condition — in the hypothesis, where a reader sees it, rather
    than in a silently narrowed domain. -/
def IsName (x : Proc) : Prop := ∃ P : Proc, x = .bang P

/-- `＊⌈P⌉ = P`, for every process `P`. Definitional: the equation is the `bang` branch of `eval`. -/
@[axiom_budget 0]
theorem eval_quote (P : Proc) : eval (Proc.bang P) = P := rfl

/-- `⌈＊x⌉ = x`, for every `x` that is a name.

    The guard is load-bearing and not a technicality: without it the statement is false, and
    `quote_eval_not_name` below exhibits the witness. A `x` that is not a quote is a process that
    happens to be usable in name position — `out x y` is the smallest example — and `eval` discards
    it, so round-tripping cannot recover it. -/
@[axiom_budget 0]
theorem quote_eval_of_name (x : Proc) (h : IsName x) : Proc.bang (eval x) = x := by
  obtain ⟨P, rfl⟩ := h
  rfl

/-- The guard cannot be dropped, and this is the witness: `⌈＊x⌉ = x` fails at `x = out nil nil`.

    Stated as a theorem rather than left to the reader because the false version is the one that
    *looks* obviously true, and this corpus has already deleted eleven theorems whose statements
    were true of nothing. A guard with no witness that it is needed is a guard nobody can assess. -/
@[axiom_budget 0]
theorem quote_eval_not_name : ¬ (∀ x : Proc, Proc.bang (eval x) = x) := by
  intro h
  have h' := h (.out .nil .nil)
  -- `eval (.out .nil .nil) = .nil`, so the left side is `bang nil`, and `bang nil ≠ out nil nil`
  -- because the constructors differ.
  exact absurd h' (by intro hc; cases hc)

/-- `bang` is injective: two names are equal only if the processes they quote are equal. This is the
    property that makes the reflection *faithful* — without it `⌈P⌉ = ⌈Q⌉` would be possible for
    `P ≠ Q`, and a name would not determine what it names. It is a consequence of `Proc` being an
    inductive, and it is recorded here because `quote_eval_of_name` alone would not notice its
    absence.

    Stated with an explicit quantifier rather than as `Function.Injective`: that identifier is
    Mathlib's, and this module is core-Lean-only so that the calculus has no dependency the
    capability layer's `Action`/Mathlib name collision could reach. (Found by type-checking this
    file standalone — the experiment the plan asked for, and the only error it produced.) -/
@[axiom_budget 0]
theorem bang_injective {P Q : Proc} (h : Proc.bang P = Proc.bang Q) : P = Q :=
  Proc.bang.inj h

/-- `eval` fixes nothing but quotes: a process that is not a name is dereferenced to `0`. Recorded
    because it is the direction that *discards* information, and a reader tracing a round-trip needs
    to know which direction is lossy. -/
@[axiom_budget 0]
theorem eval_of_not_name (x : Proc) (h : ¬ IsName x) : eval x = .nil := by
  cases x with
  | bang P => exact absurd ⟨P, rfl⟩ h
  | nil => rfl
  | out a b => rfl
  | inp a b P => rfl
  | nu a P => rfl
  | rep P => rfl
  | par P Q => rfl

/-! ==========================================================================
   Part 3 — Occurrence, and what "fresh" means here
   ========================================================================== -/

/-- `Occurs x P`: the term `x` occurs anywhere in `P`, at any depth, bound or not.

    Deliberately *syntactic*: it does not distinguish free from bound occurrence, because that
    distinction requires committing to a binding convention for `bang` — whether `⌈P⌉` seals the
    names inside `P` or exposes them — and the ρ-calculus literature is where that convention
    belongs, not in a definition invented here.

    What that costs is recorded here rather than left implicit. `Occurs` is **not invariant under the
    structural congruence**, because the congruence can change what a term mentions: `nu_nil` gives
    `SCong (νx.0) 0`, and `cong_bang` identifies `⌈P⌉` with `⌈Q⌉` for `P ≡ Q`. Those are the two
    mechanisms, and `Congruence.lean` carries each as a theorem — `occurs_not_invariant_nu_nil` and
    `occurs_not_invariant_cong_bang`, with `occurs_not_scong_invariant` the claim they refute. A side
    condition stated
    with it is therefore *weaker* than it reads, and the rules do not use it: the restriction rule and
    scope extrusion carry `Congruence.lean`'s `FreshUpToScong`, which quantifies over every term the
    congruence can reach. `Semantics/LTS.lean`'s record carries the witnesses that made the
    difference visible. This module keeps `Occurs` because it is the honest description of the term
    as written, and because `FreshUpToScong` is defined *from* it.

    `Occurs` is decidable in principle but is left as a `Prop`: nothing here computes with it, and
    a `Bool` version named for occurrence is the shape of the `has_deadlock` placeholder this corpus
    already deleted once. -/
def Occurs (x : Proc) : Proc → Prop
  | .nil => False
  | .bang P => x = .bang P ∨ Occurs x P
  | .out a b => x = a ∨ x = b
  | .inp a b P => x = a ∨ x = b ∨ Occurs x P
  | .nu a P => x = a ∨ Occurs x P
  | .rep P => Occurs x P
  | .par P Q => Occurs x P ∨ Occurs x Q

/-- `Fresh x P`: `x` does not occur in `P` at all.

    The *syntactic* freshness, and **not** a side condition of any rule. This one does not survive the
    congruence — `cong_bang` with `nu_nil` identifies `⌈νx.0⌉` with `⌈0⌉`, so a term whose channel is
    written `⌈0⌉` mentions `⌈0⌉` however its own syntax reads (see `Occurs` above) — and whether an
    *occurrence* condition of any kind could serve as one is answered in the negative by
    `Congruence.lean`'s `FreshUpToScong` and `not_freshUpToScong`: quantifying over the congruence
    makes such a condition unsatisfiable rather than invariant, because a binder of any name can always
    be reached. What a rule can use instead depends on which side of the rule it is on. -/
def Fresh (x P : Proc) : Prop := ¬ Occurs x P

/-- A process is `fresh`-determined: `Fresh x P` is decidable in the sense that matters for the
    rules — `Occurs` is a finite disjunction of equalities, so a rule carrying it carries a checkable
    condition rather than a semantic one. Recorded as the two directions a reader needs. -/
@[axiom_budget 0]
theorem not_occurs_par {x P Q : Proc} (h : Fresh x (Proc.par P Q)) :
    Fresh x P ∧ Fresh x Q := by
  constructor
  · intro hc; exact h (Or.inl hc)
  · intro hc; exact h (Or.inr hc)

/-- Occurrence is stable under `bang`: a name occurs in `⌈P⌉` iff it is `⌈P⌉` itself or occurs in
    `P`. This is the one place the module's convention is visible, so it is stated rather than left
    to the definition's unfolding. -/
@[axiom_budget 0]
theorem occurs_bang {x P : Proc} : Occurs x (Proc.bang P) ↔ x = Proc.bang P ∨ Occurs x P :=
  Iff.rfl

end DarkFi.Semantics
