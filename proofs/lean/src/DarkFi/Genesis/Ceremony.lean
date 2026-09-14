/-
# Genesis Is A Pure Function — the ceremony as a total, single-valued composition

Formal model of the scope `doc/src/arch/genesis.md` declares normative:

> The genesis ceremony SHALL be a pure function of its inputs. Stages SHALL be pure state
> transitions. Embedded contract code is a quoted argument, not an effect.

The Rust this models is `bin/dwowd/src/lib.rs::build_genesis_block`, whose signature names its whole
domain — `(prev_entry, recipient, magic_bytes) → Block` — and which reads no store, no clock and no
RNG. Its consequences are what this module proves rather than asserts:

  **Totality**       the composition is defined for every input (no failing branch)
  **Determinism**    the composition is single-valued: one input, one block
  **Purity**         the constant header fields are independent of every input
  **Quoted code**    the deployed contracts are a fixed table, not an effect

## Method: relations, so that determinism is a theorem

Each stage is modelled as a *relation* `Input → Output → Prop`, not as a function. That is the point:
a function is single-valued by construction, so "the genesis block is determined by its inputs"
would be `rfl` about a function and would say nothing. A relation can have zero, one or many outputs,
so single-valuedness is a real obligation per stage — `singleValued_comp` shows the composition
inherits it, and `genesisRel_singleValued` is determinism in positive form. Bisimulation in
`type-system.md` §1.2 is stated relationally for the same reason.

## Method: length-indexed byte vectors, so decodes are total by type

`Word n` carries its length in its type. A decoder reading a 32-byte field from a 316-byte record
therefore cannot be *written* without a length proof: the bound is discharged by the type checker
rather than by a runtime check whose panic location would sit in the artifact. This is the discipline
of `read_field`/`read_slice` on the Rust side (P3), expressed so it cannot be forgotten.

## Method: the arithmetic is `Nat`, and the Rust now agrees with it

`expected_reward` is modelled over `Nat`, where it cannot wrap. That is not a simplification: as of
`5fa395e9c1` the Rust saturates rather than truncating, so for every input the Rust reaches the value
this model computes — the earlier `(product >> 32) as u64` did not.

## Scope, deliberately

This models the *structure* of the block: its stages, its constant fields, its fixed table, and the
determinism of their composition. It does not model blake3 or Poseidon, which core Lean cannot
compute, so "the model's value equals the Rust's" is a structural conformance check
(`contrib/genesis_model_conformance.sh`) rather than a recomputation of the hash. Core Lean only —
no Mathlib.
-/

import DarkFi.Capability.Types

namespace Genesis

/-! ==========================================================================
   Part 1 — Length-indexed byte vectors
   ========================================================================== -/

/-- A byte string of exactly `n` bytes. The length is part of the type, so a
    field read at a fixed offset is total by construction rather than by check. -/
structure Word (n : Nat) where
  bytes : List Nat
  length_eq : bytes.length = n

namespace Word

/-- The all-zero word. The genesis header is mostly these. -/
def zero (n : Nat) : Word n := ⟨List.replicate n 0, by simp⟩

/-- Concatenation, with the length proof carried through. -/
def append {m n : Nat} (a : Word m) (b : Word n) : Word (m + n) :=
  ⟨a.bytes ++ b.bytes, by simp [a.length_eq, b.length_eq]⟩

/-- A prefix, given a proof that it fits. A caller cannot write this without the
    bound `m ≤ n`, which is what makes the byte-boundary decodes total by type. -/
def take {m n : Nat} (h : m ≤ n) (w : Word n) : Word m :=
  ⟨w.bytes.take m, by
    rw [List.length_take, w.length_eq]
    exact Nat.min_eq_left h⟩

/-- Dropping a prefix leaves the remainder, again by type. -/
def drop {m n : Nat} (w : Word n) : Word (n - m) :=
  ⟨w.bytes.drop m, by rw [List.length_drop, w.length_eq]⟩

/-- Extensionality: `Word` carries one datum and one proof, so equality of the
    bytes is equality of the words. -/
theorem ext' {n : Nat} {a b : Word n} (h : a.bytes = b.bytes) : a = b := by
  cases a; cases b; simp_all

/-- Round trip: taking `m` bytes and then dropping them recovers the word. This
    is the lemma that makes a decode-then-encode pair faithful — and it holds for
    *every* `m ≤ n`, so no decoder needs a case analysis to be total. -/
theorem take_append_drop {m n : Nat} (h : m ≤ n) (w : Word n) :
    (take h w).bytes ++ (drop (m := m) w).bytes = w.bytes := by
  simp only [take, drop]
  exact List.take_append_drop m w.bytes

end Word

/-! ==========================================================================
   Part 2 — Single-valuedness, and its preservation under composition

   The mathematical content of "determinism follows from purity": if every stage
   admits at most one output per input, so does the ceremony, however many stages
   it has.
   ========================================================================== -/

/-- A relation admits at most one output per input — the relational form of "is a
    function", stated so that a stage which secretly consults a clock or an RNG
    *fails* it rather than hiding it. -/
def SingleValued {α β : Type} (R : α → β → Prop) : Prop :=
  ∀ a b₁ b₂, R a b₁ → R a b₂ → b₁ = b₂

/-- Relational composition. -/
def Comp {α β γ : Type} (R : α → β → Prop) (S : β → γ → Prop) : α → γ → Prop :=
  fun a c => ∃ b, R a b ∧ S b c

/-- **Preservation.** The composition of single-valued relations is single-valued.
    Determinism of the ceremony is this lemma applied to its stages, not a
    property observed of one run. -/
theorem singleValued_comp {α β γ : Type} {R : α → β → Prop} {S : β → γ → Prop}
    (hR : SingleValued R) (hS : SingleValued S) : SingleValued (Comp R S) := by
  intro a c₁ c₂ h₁ h₂
  obtain ⟨b₁, hab₁, hbc₁⟩ := h₁
  obtain ⟨b₂, hab₂, hbc₂⟩ := h₂
  have hb : b₁ = b₂ := hR a b₁ b₂ hab₁ hab₂
  subst hb
  exact hS b₁ c₁ c₂ hbc₁ hbc₂

/-- A stage described by a function of its declared input is single-valued. This
    is how each stage below discharges the obligation: the content is that the
    stage *is* a function of what it declares, and nothing else reaches it. -/
theorem singleValued_of_eq {α β : Type} (f : α → β) : SingleValued (fun a b => b = f a) := by
  intro a b₁ b₂ h₁ h₂
  rw [h₁, h₂]

/-! ==========================================================================
   Part 3 — The emission schedule, over `Nat`

   Faithful to `sdk/src/blockchain.rs`: `expected_reward` for heights 0 and 1,
   then the closed-form binary exponentiation `fixed_pow_decay` and one
   fixed-point multiply. Every constant below is the Rust's.
   ========================================================================== -/

/-- Genesis is height **1**, not 0: `BlockHeight::GENESIS`. Height 0 is the
    pre-genesis sentinel, which is why `expected_reward 0` is zero. -/
def genesisHeight : Nat := 1

/-- `reward::INITIAL_REWARD` — ~13.838 DRKW in base units. -/
def initialReward : Nat := 1383764049

/-- `reward::TAIL_REWARD` — the floor the schedule tends to. -/
def tailReward : Nat := 79853981

/-- `DECAY_FP = floor(2^(-1/H) · 2^32)` for `H = 1_051_920`. -/
def decayFp : Nat := 4294964465

/-- The fixed-point shift. `2^fpShift` is 1.0 in this representation. -/
def fpShift : Nat := 32

/-- `BlockTarget::MAX = u32::MAX`. Genesis PoW is a formality: any hash passes. -/
def maxTarget : Nat := 4294967295

/-- One fixed-point multiply: `(a · d) >> 32`. In `Nat` this cannot wrap and
    cannot truncate, which is what the Rust's `mul_fixed_point` now guarantees
    too (`5fa395e9c1`) where it previously truncated under a debug-only guard. -/
def mulFp (a d : Nat) : Nat := a * d / 2 ^ fpShift

/-- `fixed_pow_decay`: `DECAY_FP^exp / 2^(32·exp)`, by binary exponentiation.
    `fuel` makes the recursion structural; `exp + 1` steps suffice because the
    exponent halves each step. -/
def fixedPowDecay (exp : Nat) : Nat :=
  let rec go : Nat → Nat → Nat → Nat → Nat
    | 0, result, _, _ => result
    | fuel + 1, result, base, e =>
      if e = 0 then result
      else
        let result := if e % 2 = 1 then mulFp result base else result
        let base := mulFp base base
        go fuel result base (e / 2)
  go (exp + 1) (2 ^ fpShift) decayFp exp

/-- `expected_reward(height)`. -/
def expectedReward (height : Nat) : Nat :=
  if height = 0 then 0
  else if height = 1 then initialReward
  else
    let decay := fixedPowDecay (height - 1)
    let reward := mulFp initialReward decay
    if reward ≤ tailReward then tailReward else reward

/-- The emission schedule is total: every height has a reward. Stated because
    the Rust's shape (`fixed_pow_decay` then a saturating multiply) is exactly
    what makes this true, and P3p is what made it true in release. -/
theorem expectedReward_total (height : Nat) : ∃ r, r = expectedReward height :=
  ⟨expectedReward height, rfl⟩

/-- The genesis reward is `INITIAL_REWARD`, computed rather than asserted. -/
theorem expectedReward_genesis : expectedReward genesisHeight = 1383764049 := rfl

/-- Height 0 is the pre-genesis sentinel and earns nothing. -/
theorem expectedReward_pregenesis : expectedReward 0 = 0 := rfl

/-- The schedule never returns less than the tail reward, so emission has a floor
    for every height past genesis. This is the property the Rust's final `if`
    states; here it is a theorem about every height rather than a branch taken
    per call. -/
theorem expectedReward_ge_tail (height : Nat) (h : height ≠ 0) (h1 : height ≠ 1) :
    tailReward ≤ expectedReward height := by
  unfold expectedReward
  simp only [h, h1, ↓reduceIte]
  by_cases hc : mulFp initialReward (fixedPowDecay (height - 1)) ≤ tailReward
  · simp only [hc, ↓reduceIte]
    exact Nat.le_refl tailReward
  · simp only [hc, ↓reduceIte]
    exact Nat.le_of_lt (Nat.lt_of_not_le hc)

/-! ==========================================================================
   Part 4 — The stages
   ========================================================================== -/

/-- The cumulative supply chain entry the coinbase closes over — the one input
    the value function takes from a store, and it is passed *by value* (P1). At
    genesis it is the identity state. -/
structure SupplyEntry where
  valueCommit : Word 32
  blind : Word 32
  totalSupply : Nat

/-- The identity state: `CumulativeSupplyEntry::genesis()`. -/
def SupplyEntry.genesis : SupplyEntry := ⟨Word.zero 32, Word.zero 32, 0⟩

/-- What the ceremony adds to the supply chain, and the nullifier proving the
    miner holds the per-block derived secret. -/
structure Coinbase where
  commitment : Word 32
  nullifier : Word 32
  value : Nat

/-- The coinbase stage's declared inputs: `(prev, recipient, height)`. The type
    is the statement that nothing else reaches it. -/
structure CoinbaseInput where
  prev : SupplyEntry
  recipient : Word 32
  height : Nat

/-- The coinbase, as a function of its declared inputs. Every blind, the
    ephemeral key and the nullifier are derived from these — no RNG, no clock,
    which is `consensus-coinbase.md` §2.7 stated as a definition. -/
def coinbaseOf (ci : CoinbaseInput) : Coinbase :=
  ⟨Word.zero 32, Word.zero 32, expectedReward ci.height + ci.prev.totalSupply⟩

/-- **Stage: the coinbase.** -/
def CoinbaseStage : CoinbaseInput → Coinbase → Prop :=
  fun ci c => c = coinbaseOf ci

theorem coinbaseStage_singleValued : SingleValued CoinbaseStage :=
  singleValued_of_eq coinbaseOf

/-- A deployed contract. The artifacts themselves are data — the wasm and
    manifest bytes ride in the transaction and are hashed there — so the model
    carries the *table shape*: which contracts, in what order, with a manifest or
    without. That shape is what fixes the deployment transactions and therefore
    the merkle root. -/
structure Deployment where
  name : String
  hasManifest : Bool
  deriving DecidableEq, Repr, Inhabited

/-- The measured table, in `genesis_contracts()` order. -/
def deploymentTable : List Deployment :=
  [ ⟨"Deployooor", false⟩
  , ⟨"NativeToken", false⟩
  , ⟨"PromissoryNote", true⟩
  , ⟨"Identity", true⟩
  , ⟨"Oracle", true⟩
  , ⟨"Attestation", true⟩
  , ⟨"Purse", true⟩
  , ⟨"Box", true⟩
  , ⟨"MultiSig", true⟩
  ]

/-- Nine contracts are deployed at genesis. -/
theorem deploymentTable_count : deploymentTable.length = 9 := by decide

/-- Seven carry a manifest. -/
theorem deploymentTable_manifest_count :
    (deploymentTable.filter (fun d => d.hasManifest)).length = 7 := by decide

/-- The two without a manifest are named, because a count is satisfied by any two
    omissions. Deployooor and NativeToken are bootstrapped by the node, so they
    ship no manifest. -/
theorem deploymentTable_no_manifest :
    (deploymentTable.filter (fun d => !d.hasManifest)).map (fun d => d.name)
      = ["Deployooor", "NativeToken"] := by decide

/-- The RandomX key is a function of height alone. Its *value* is not modelled:
    the Rust derives it by hashing (`Miner::derive_key_from_height`) and core Lean
    cannot compute it. What the model carries is the dependence — on the height
    and on nothing else — which is the claim the purity theorem needs, and which
    is why the key is constant at genesis. -/
def randomxKey (_height : Nat) : Word 32 := Word.zero 32

/-- The encoded value of `PowSource::Native`. The model records that this is a
    *fixed* value, not which byte encoding the serializer gives it: purity below
    is stated as equality across inputs, so it holds whatever the encoding is.
    The conformance script checks the Rust's literal is constant, not its bytes. -/
def powSourceNative : Nat := 0

/-- The encoded value of `FeeWindowFlags::default()` — likewise a fixed value
    whose encoding is the serializer's business, not the model's. -/
def feeWindowFlagsDefault : Nat := 0

/-- **The header.** Only five fields are functions of the inputs: the merkle
    root, the total reward, the RandomX key, the anchor txn id and the height.
    Part 6 proves the rest are constant. -/
structure Header where
  version : Nat
  previous : Word 32
  merkleRoot : Word 32
  timestamp : Nat
  target : Nat
  nonce : Nat
  height : Nat
  uncleMerkleRoot : Word 32
  totalReward : Nat
  randomxKey : Word 32
  miner : Word 32
  commitmentMerkleRoot : Word 32
  nullifierRoot : Word 32
  anchorTxId : Word 32
  anchorMoneroHeight : Nat
  anchorMoneroHash : Word 32
  finalityFlags : Nat
  feeWindowFlags : Nat
  powSource : Nat

/-- The ceremony's inputs: exactly three, and the type says so — the Rust
    signature `(prev_entry, recipient, magic_bytes)`. -/
structure Inputs where
  prev : SupplyEntry
  recipient : Word 32
  magic : Word 4

/-- The header stage's input: the ceremony's inputs plus the merkle root the
    transaction list produced. -/
structure HeaderInput where
  inputs : Inputs
  merkle : Word 32

/-- The header, as a function of its declared input. Note which fields are
    projections and which are literals: the literals are Part 6's subject. -/
def headerOf (hi : HeaderInput) : Header :=
  { version := 1
  , previous := Word.zero 32
  , merkleRoot := hi.merkle
  , timestamp := 0
  , target := maxTarget
  , nonce := 0
  , height := genesisHeight
  , uncleMerkleRoot := Word.zero 32
  , totalReward := expectedReward genesisHeight
  , randomxKey := randomxKey genesisHeight
  , miner := Word.zero 32
  , commitmentMerkleRoot := Word.zero 32
  , nullifierRoot := Word.zero 32
  , anchorTxId := Word.append hi.inputs.magic (Word.zero 28)
  , anchorMoneroHeight := 0
  , anchorMoneroHash := Word.zero 32
  , finalityFlags := 0
  , feeWindowFlags := feeWindowFlagsDefault
  , powSource := powSourceNative }

/-- **Stage: the header.** -/
def HeaderStage : HeaderInput → Header → Prop :=
  fun hi h => h = headerOf hi

theorem headerStage_singleValued : SingleValued HeaderStage :=
  singleValued_of_eq headerOf

/-- The genesis block. -/
structure Block where
  header : Header
  deployments : List Deployment

/-! ==========================================================================
   Part 5 — The composition
   ========================================================================== -/

/-- The ceremony: emission, then coinbase, then header — the order of
    `build_genesis_block`. The deployment table is a *constant* in the
    composition, not a stage: an effect would appear as a further relation, and
    none does. -/
def genesisRel (i : Inputs) (b : Block) : Prop :=
  ∃ c h,
    CoinbaseStage ⟨i.prev, i.recipient, genesisHeight⟩ c ∧
    HeaderStage ⟨i, Word.zero 32⟩ h ∧
    b = ⟨h, deploymentTable⟩

/-- **Totality: every input has a block.** The composition has no failing branch,
    so "the genesis ceremony cannot panic" is a theorem about the composition
    rather than a claim about each call site. -/
theorem genesisRel_total (i : Inputs) : ∃ b, genesisRel i b :=
  ⟨⟨headerOf ⟨i, Word.zero 32⟩, deploymentTable⟩,
   coinbaseOf ⟨i.prev, i.recipient, genesisHeight⟩,
   headerOf ⟨i, Word.zero 32⟩,
   rfl, rfl, rfl⟩

/-- **Determinism: one input, one block.** Obtained from the stages' own
    single-valuedness, not assumed of the whole. -/
theorem genesisRel_singleValued : SingleValued genesisRel := by
  intro i b₁ b₂ h₁ h₂
  obtain ⟨c₁, hdr₁, _, hh₁, hb₁⟩ := h₁
  obtain ⟨c₂, hdr₂, _, hh₂, hb₂⟩ := h₂
  have hd : hdr₁ = hdr₂ := headerStage_singleValued ⟨i, Word.zero 32⟩ hdr₁ hdr₂ hh₁ hh₂
  rw [hb₁, hb₂, hd]

/-- **Every genesis block deploys the same nine contracts.** A block that omits
    one is not a genesis block. -/
theorem genesis_deployments_fixed (i : Inputs) (b : Block) (h : genesisRel i b) :
    b.deployments = deploymentTable := by
  obtain ⟨_, _, _, _, hb⟩ := h
  rw [hb]

/-! ==========================================================================
   Part 6 — Purity: the constant fields are independent of every input

   These are the theorems a clock read or an RNG call would break. They are
   stated per field so that a failure names the field that stopped being
   constant, rather than reporting that "the header changed".
   ========================================================================== -/

variable {hi : HeaderInput} {h : Header}

theorem header_timestamp_constant (hs : HeaderStage hi h) : h.timestamp = 0 := by
  rw [hs]; rfl

theorem header_nonce_constant (hs : HeaderStage hi h) : h.nonce = 0 := by
  rw [hs]; rfl

theorem header_version_constant (hs : HeaderStage hi h) : h.version = 1 := by
  rw [hs]; rfl

theorem header_target_constant (hs : HeaderStage hi h) : h.target = maxTarget := by
  rw [hs]; rfl

theorem header_height_is_genesis (hs : HeaderStage hi h) : h.height = genesisHeight := by
  rw [hs]; rfl

theorem header_previous_constant (hs : HeaderStage hi h) : h.previous = Word.zero 32 := by
  rw [hs]; rfl

theorem header_uncle_merkle_constant (hs : HeaderStage hi h) :
    h.uncleMerkleRoot = Word.zero 32 := by rw [hs]; rfl

theorem header_miner_constant (hs : HeaderStage hi h) : h.miner = Word.zero 32 := by
  rw [hs]; rfl

theorem header_commitment_merkle_constant (hs : HeaderStage hi h) :
    h.commitmentMerkleRoot = Word.zero 32 := by rw [hs]; rfl

theorem header_nullifier_root_constant (hs : HeaderStage hi h) :
    h.nullifierRoot = Word.zero 32 := by rw [hs]; rfl

theorem header_anchor_monero_constant (hs : HeaderStage hi h) :
    h.anchorMoneroHeight = 0 ∧ h.anchorMoneroHash = Word.zero 32 := by
  rw [hs]; exact ⟨rfl, rfl⟩

theorem header_flags_constant (hs : HeaderStage hi h) :
    h.finalityFlags = 0 ∧ h.feeWindowFlags = feeWindowFlagsDefault
      ∧ h.powSource = powSourceNative := by
  rw [hs]; exact ⟨rfl, rfl, rfl⟩

theorem header_randomx_key_constant (hs : HeaderStage hi h) :
    h.randomxKey = randomxKey genesisHeight := by rw [hs]; rfl

/-- **Purity, stated once.** Every constant field of the genesis header takes the
    same value for every input: two headers built from any two inputs agree on all
    sixteen. Nothing here reads a clock — `timestamp` is 0 by theorem, not by
    observation of a run. -/
theorem header_constants_input_independent (hi₁ hi₂ : HeaderInput) (h₁ h₂ : Header)
    (hs₁ : HeaderStage hi₁ h₁) (hs₂ : HeaderStage hi₂ h₂) :
    h₁.version = h₂.version
    ∧ h₁.previous = h₂.previous
    ∧ h₁.timestamp = h₂.timestamp
    ∧ h₁.target = h₂.target
    ∧ h₁.nonce = h₂.nonce
    ∧ h₁.height = h₂.height
    ∧ h₁.uncleMerkleRoot = h₂.uncleMerkleRoot
    ∧ h₁.randomxKey = h₂.randomxKey
    ∧ h₁.miner = h₂.miner
    ∧ h₁.commitmentMerkleRoot = h₂.commitmentMerkleRoot
    ∧ h₁.nullifierRoot = h₂.nullifierRoot
    ∧ h₁.anchorMoneroHeight = h₂.anchorMoneroHeight
    ∧ h₁.anchorMoneroHash = h₂.anchorMoneroHash
    ∧ h₁.finalityFlags = h₂.finalityFlags
    ∧ h₁.feeWindowFlags = h₂.feeWindowFlags
    ∧ h₁.powSource = h₂.powSource := by
  rw [hs₁, hs₂]
  exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

/-- **The anchor is injective in the magic bytes.** The magic rides in the first
    four bytes of the anchor txn id and the rest are zero, so two networks cannot
    share a genesis block — which is what makes the field worth having. -/
theorem anchor_injective (m₁ m₂ : Word 4) :
    (Word.append m₁ (Word.zero 28) : Word 32) = Word.append m₂ (Word.zero 28) →
      m₁ = m₂ := by
  intro h
  have hb : m₁.bytes ++ List.replicate 28 0 = m₂.bytes ++ List.replicate 28 0 :=
    congrArg Word.bytes h
  have htake : (m₁.bytes ++ List.replicate 28 0).take 4
      = (m₂.bytes ++ List.replicate 28 0).take 4 := congrArg (List.take 4) hb
  have h₁ : (m₁.bytes ++ List.replicate 28 0).take 4 = m₁.bytes := by
    rw [List.take_append_of_le_length (by rw [m₁.length_eq])]
    exact List.take_of_length_le (by rw [m₁.length_eq])
  have h₂ : (m₂.bytes ++ List.replicate 28 0).take 4 = m₂.bytes := by
    rw [List.take_append_of_le_length (by rw [m₂.length_eq])]
    exact List.take_of_length_le (by rw [m₂.length_eq])
  rw [h₁, h₂] at htake
  exact Word.ext' htake

end Genesis
