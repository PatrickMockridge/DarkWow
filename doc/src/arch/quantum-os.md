# Quantum-OS and the Promissory Note Bridge

*Quantum-OS (by Jim Scarver, DarkWow collaborator) implements the same O-Cap
authorization model as DarkWow — but over ZFA algebra instead of ZK circuits, and
without a blockchain. The shared promissory note primitive is the natural bridge
between the two projects: same bearer-instrument lifecycle, radically different
conservation mechanisms.*

---

## What is Quantum-OS

[Quantum-OS](https://github.com/jimscarver/quantum-os) is a browser-based
peer-to-peer collaboration platform. Two or more peers connect via WebRTC, share a
room identified by a ZFA (Zero Free Action) capability token, and run operations
whose results broadcast to all peers. There is no blockchain, no ZK proofs, no
global consensus, and no accounts — possessing a capability token IS authorization.

The ZFA kernel is implemented in Rust (compiled to WASM, ~650 lines) with a
TypeScript browser frontend (~9000 lines). All security invariants are
machine-verified in Lean 4 via the companion [Quantum Logical
Framework](https://github.com/jimscarver/quantum-logical-framework) project.

**Live deployment:** <https://jimscarver.github.io/quantum-os/>

---

## The Shared Primitive: Promissory Notes

Both Quantum-OS and DarkWow implement promissory notes as **bearer instruments** —
transferable tokens where holding the token IS the capability to redeem. The
lifecycle is identical: declare a currency → mint notes → transfer → redeem. The
conservation mechanism is where the projects diverge.

### Lifecycle Side-by-Side

| Operation | Quantum-OS | DarkWow |
|-----------|-----------|---------|
| **Declare currency** | `cap:token-USD:hex` — ZFA-balanced issuer authority token, stored in `currencyTokens` | `TokenMintV1` (0x00) on [Promissory Note](../contract/promissory_note.md) contract — registers token ID, issuer commitment, Pedersen generator |
| **Mint** | `/note grant USD 5` → `cap:note-USD:hex` where denomination = `hex.length / 2` (each twist pair = 1 unit) | `MintV1` (0x02) → `Coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)` committed to Merkle tree |
| **Transfer** | `/note pass <token> <peer>` — direct WebRTC data channel, token moves from sender's `noteStore` to recipient's | `TransferV1` (0x04) — ZK proof that input coin exists in Merkle tree, nullifier prevents double-spend, output coin commitment in new tree |
| **Redeem** | `/note redeem <token>` — issuer-side accounting, receipt generated, token removed from `noteStore` | `RedeemV1` (0x01) — ZK circuit constrains `value = 0` for redeemed coin, nullifier marks as consumed |
| **Split** | `/note split <token> <n> <m>` — ZFA-balanced partition (hex split preserves count balance) | Create multiple output coins in one TransferV1, circuit constrains `sum(inputs) == sum(outputs)` |
| **Merge** | `/note merge <token1> <token2>` — ZFA-balanced concatenation (hex concatenation preserves count balance) | Multiple input coins in one TransferV1, circuit constrains conservation |
| **Discovery** | `note-grant` broadcast: currency + denomination only (bearer token stays private), `sync-currencies` on join | AEAD-encrypted note (`AeadEncryptedNote`) in contract call `ix` field, wallet decrypts with shared secret via Sapling DH + ChaCha20Poly1305 |
| **Terms** | Terms-stamped series: `cap:note-USD~<hash>:hex` via FNV-1a hash, dyncap-signed `note-series` envelope, `/note accept` gate before redeem | Manifest-described capability types and function schemas, trust-tiered (Genesis > SelfDeployed > Attested > Unverified) |

### Conservation: ZFA Algebra vs ZK Circuits

This is the core architectural divergence. Both systems enforce value conservation
at the primitive level — but through fundamentally different mechanisms chosen for
fundamentally different trust models.

**Quantum-OS (ZFA algebra):** Every note token is a ZFA-balanced twist sequence.
`count_pos == count_neg` is the conservation invariant. Splitting a note partitions
the hex string — the child sequences remain ZFA-balanced because any partition of a
balanced multiset is itself balanced. Merging concatenates balanced sequences —
`count_balanced(A) ∧ count_balanced(B) → count_balanced(A ++ B)`. Conservation is
an **algebraic identity**, machine-verified in Lean 4 (`rho_process_always_zfa`).

**DarkWow (ZK circuits):** Coin values are Pedersen commitments — additively
homomorphic. The ZK circuit constrains `sum(input_values) == sum(output_values)` per
token type. Conservation is a **cryptographic constraint**, verified by the Halo2
proving system on the Pallas curve. The `BaseDiv` opcode (Lean4-verified) enables
proportional splits.

| Property | ZFA Algebra (Quantum-OS) | ZK Circuits (DarkWow) |
|----------|--------------------------|----------------------|
| **Conservation check** | O(1) string length check | O(n) circuit constraint verification |
| **Hidden values** | No — token string is the value | Yes — Pedersen commitments hide amounts |
| **Hidden participants** | No — data channel is point-to-point | Yes — ZK proofs reveal nothing about holder |
| **Double-spend prevention** | Sender deletes from `noteStore` (honest peer) | Nullifier + Merkle proof (cryptographic) |
| **Trust model** | Semi-trusted room (peers agree on invariant) | Adversarial (anyone can submit proofs) |
| **Formal verification** | Lean 4: `rho_process_always_zfa` | Lean 4: `LessThanOrEqual`, `IsNotEqual`, `BaseDiv` opcodes |

### Why Two Mechanisms for the Same Primitive

The choice of conservation mechanism follows directly from the trust model:

- **Quantum-OS rooms are collaborative.** Peers join by invitation (room cap in URL
  hash). The ZFA invariant is sufficient because all participants are assumed to
  agree on it — a peer who violates it is simply ignored. No cryptographic
  enforcement is needed.

- **DarkWow is adversarial.** Anyone can submit transactions to the mempool. Miners
  are economically incentivized but not trusted. The ZK circuit must enforce
  conservation without trusting any participant — the proof IS the enforcement.

Both approaches are correct for their context. The shared promissory note primitive
survives the translation because the bearer-instrument lifecycle (declare → mint →
transfer → redeem) is independent of the conservation mechanism. **The note is the
abstraction; ZFA and ZK are alternative implementations of the same interface.**

---

## Capability Model Parallels

Both projects implement Object Capabilities — authorization via token possession
rather than identity revelation. The structural parallels run deep:

| Concept | Quantum-OS | DarkWow |
|---------|-----------|---------|
| **Capability token** | `cap:label:hex` — ZFA-balanced twist sequence | `poseidon_hash(pub, params)` — on-chain commitment |
| **Authorization check** | `validateCapability(token)` — ZFA balance check | ZK proof of secret knowledge + predicate satisfaction |
| **Identity** | `cap:peer:hex` + dyncap hash chain (TOFU) | Identity Contract with competency DAGs, ZK credential proofs |
| **Revocation** | Not supported (bearer model) | `RevokeCapabilityV1` — issuer invalidates before use |
| **Delegation** | `/pass name peer` — direct transfer | `IssueCapabilityV1` — issuer grants to holder |
| **Predicates** | No predicates (possession = authorization) | ZK circuit constraints (LTE, IsNotEqual, range checks, etc.) |
| **Formal model** | Curry-Howard: token IS proof of authorization | Authorization Inversion Theorem: ACL → O-Cap via ZK |

Quantum-OS demonstrates that the O-Cap model works at its algebraic extreme: the
token itself is the only authorization primitive. DarkWow extends this with ZK
predicates, enabling capabilities with conditions ("can spend up to N", "must be
over 18") without revealing who satisfies them.

---

## Governance

Both projects implement governance, but at different layers:

| Aspect | Quantum-OS | DarkWow |
|--------|-----------|---------|
| **Model** | Liquid democracy + liquid trust, built into protocol | DAO Escrow contract, three modes (Escrow/Treasury/Endowment) |
| **Voting** | Approval + ranked-choice (IRV), joiner-local tally | Token-weighted, on-chain |
| **Delegation** | Transitive, revocable, per-issue override | Not implemented |
| **Accountability** | Trust hierarchy + ⅔-quorum censure with slashing | Not implemented |
| **Treasury** | Promissory note currencies per group | Native token (DARK) + any PN token |

Quantum-OS's governance is more expressive (liquid democracy, trust weighting,
censure) because it doesn't need on-chain execution — all computation is
joiner-local. DarkWow's governance is simpler but has the advantage of global
enforceability.

---

## Formal Verification

Both projects use **Lean 4** for formal verification of security-critical
properties, but verify different things:

- **Quantum-OS**: Verifies algebraic invariants of the ZFA system —
  `rho_process_always_zfa` (parallel composition stays balanced),
  `decoherence_impossibility` (no operation breaks ZFA),
  `bra_ket_always_balanced` (bra-ket well-typedness IS ZFA balance).

- **DarkWow**: Verifies ZK opcode correctness — `LessThanOrEqual` (0x55),
  `IsNotEqual` (0x62), `BaseDiv` (0x58). These are return-value gates whose
  constraint soundness is critical to the ZK circuit's security.

**Shared methodology:** Both projects treat formal verification as a development
tool, not an afterthought. The Lean proofs are integrated into the development
workflow and anchor the security claims each project makes.

---

## Synergy Opportunities

The shared promissory note primitive creates concrete opportunities for
cross-pollination:

### DarkWow → Quantum-OS

- **ZK privacy for notes.** Quantum-OS notes are bearer tokens — anyone who sees the
  hex string owns the note. DarkWow's ZK commitment + nullifier model could add
  cryptographic privacy to Quantum-OS notes without changing the lifecycle.
- **AEAD discovery.** DarkWow's AEAD-encrypted note discovery (wallet scans all
  contract call data byte-by-byte for decryptable notes) could replace
  Quantum-OS's broadcast-based discovery, enabling private note delivery.
- **Global consensus for notes that need it.** A note that needs globally-verifiable
  redemption (e.g., a stablecoin) could bridge from a Quantum-OS room to DarkWow's
  blockchain.

### Quantum-OS → DarkWow

- **Simpler promissory note model.** Quantum-OS's ZFA-algebraic conservation is a
  minimal reference implementation of the bearer-instrument lifecycle. New DarkWow
  contracts that issue bearer instruments could use this as a design template
  before adding ZK complexity.
- **Liquid democracy governance.** Quantum-OS's `/gov` system (transitive
  delegation, trust weighting, censure accountability) is more expressive than
  DarkWow's DAO Escrow. These patterns could inform a richer on-chain governance
  contract.
- **Lemma/credential system.** Quantum-OS's `@lemma` system — deterministic
  name-to-twist allocation, composable deduction via `/qucalc`, transfer via
  `/request` → `/pass` — is a lightweight alternative to DarkWow's Identity
  Contract for use cases that don't need ZK credential proofs.
- **Multi-room Markov blanket isolation.** Quantum-OS's per-room state isolation
  (independent lemma stores, currency registries, dyncap chains) is a pattern for
  DarkWow applications that need compartmentalized state with explicit bridging.

### Shared

- **Capability token encoding.** Both projects encode capabilities as
  `cap:label:payload` strings. A shared encoding standard would enable
  interoperability — a Quantum-OS note could be referenced in a DarkWow contract,
  or a DarkWow capability could be validated in a Quantum-OS room.
- **Formal verification cross-validation.** The Lean 4 proofs in each project verify
  different properties of the same underlying primitives. Cross-validating the
  proof strategies could strengthen both.

---

## Links

- [Quantum-OS Repository](https://github.com/jimscarver/quantum-os)
- [Quantum Logical Framework](https://github.com/jimscarver/quantum-logical-framework) — ZFA formal verification in Lean 4
- [DarkWow O-Cap Model](ocap.md) — Authorization Inversion Theorem and composable privacy
- [DarkWow Promissory Note](../contract/promissory_note.md) — Private bearer instruments on DarkWow
- [DarkWow Identity Contract](identity.md) — O-Cap implementation with competency DAGs
- [DarkWow Formal Verification](../../proofs/lean/) — ZK opcode proofs in Lean 4
