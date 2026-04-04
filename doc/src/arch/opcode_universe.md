# The Complete Mathematical Universe of DarkFi Opcodes

> **Abstract**: DarkFi's zkVM provides a Turing-complete zero-knowledge computation framework built on elliptic curve cryptography and finite field arithmetic. This document analyzes the complete opcode space required to express all possible DeFi applications, identifies critical gaps in the current implementation, and provides mathematical reasoning for field arithmetic challenges that determine what smart contracts can and cannot securely express.

---

## 1. The Mathematical Foundation

DarkFi operates in the **Pallas field** $\mathbb{F}_p$ where:

$$p = 2^{254} - 2^{32} - 2^7 - 2^4 - 2 - 1$$

This is a 254-bit prime, approximately $1.19 \times 10^{76}$. Every value in DarkFi circuits is an element of this field.

### 1.1 Field Elements vs Mathematical Integers

A critical distinction for smart contract security:

```
As integers:     0 < 1 < 2 < ... < p-2 < p-1
As field elements:  0 ≡ p < 1 < 2 < ... < p-2 < p-1 ≡ -1 (mod p)
```

**The wraparound problem**: Values in the range $[p - 2^{32}, p)$ exhibit field ordering that differs from integer ordering. A comparison gadget that works correctly for values $\ll p$ may fail catastrophically for values near $p$.

This is not merely theoretical. Consider:
- Integer comparison: `max_value = 2^64 - 1` checks correctly
- But if the same field element represents `p - 1`, field ordering treats it as `-1`

### 1.2 The Object Capability Model

DarkFi supports **object capability (OCap) security** patterns through its permission system. In OCap:

- **Capability**: An unforgeable token that grants specific access rights
- **Object**: An entity that can perform actions or hold state
- **Invocation**: A capability proves the right to call a method

Mathematically, a capability is a **digest** derived from:
```
capability = PoseidonHash(secret_key, permissions, scope, nonce)
```

The holder of the capability can only exercise permissions within the specified scope. The scope itself can be:
- A specific contract address
- A finite set of function signatures
- A time-bounded interval
- An amount ceiling

**Why this matters for opcode design**: Every opcode that handles capabilities must respect the scope boundary. An opcode that allows "unchecked addition" to a capability's scope breaks OCap invariants.

---

## 2. Current Opcode Inventory

### 2.1 Elliptic Curve Operations (Complete)

| Opcode | Signature | Mathematical Operation |
|--------|-----------|----------------------|
| `EcAdd` | `(EcPoint, EcPoint) → EcPoint` | $P + Q$ on Jubjub |
| `EcMul` | `(Scalar, EcFixedPoint) → EcPoint` | $a \cdot G$ |
| `EcMulBase` | `(Base, EcFixedPointBase) → EcPoint` | $x \cdot G$ with 64-bit $x$ |
| `EcMulShort` | `(Base, EcFixedPointShort) → EcPoint` | Short scalar multiplication |
| `EcMulVarBase` | `(Base, EcNiPoint) → EcPoint` | $x \cdot P$ variable base |
| `EcGetX` | `EcPoint → Base` | Extract x-coordinate |
| `EcGetY` | `EcPoint → Base` | Extract y-coordinate |

**Assessment**: ✅ **Complete for Jubjub curve operations.** All point arithmetic expressible.

### 2.2 Hashing Operations (Partial)

| Opcode | Signature | Status |
|--------|-----------|--------|
| `PoseidonHash` | `BaseArray → Base` | ✅ Production |
| `MerkleRoot` | `(Uint32, MerklePath, Base) → Base` | ⚠️ Fixed depth 32 only |
| `SparseMerkleRoot` | `(Base, SparseMerklePath, Base) → Base` | ✅ Depth 3 only |

**The Merkle Depth Problem**: `MerkleRoot` uses:
```rust
MerklePath::construct(
    [config.merkle_chip_1(), config.merkle_chip_2()],
    OrchardHashDomains::MerkleCrh,  // Sinsemilla hash
    leaf_pos,
    merkle_path,  // [Fp; MERKLE_DEPTH_ORCHARD] where DEPTH = 32
)
```

This is **type-level fixed** — the depth cannot vary at runtime. Different blockchain ecosystems use different Merkle depths:

| Chain | Depth | Hash Function |
|------|-------|---------------|
| Zcash Orchard | 32 | Sinsemilla |
| Ethereum (Keccak) | Variable | Keccak-256 |
| Bitcoin | Variable | SHA256 |
| Solana | 32 | SHA-256 (some) |

**Implication**: A bridge contract verifying Ethereum state proofs cannot use `MerkleRoot`. Must implement custom Keccak-based verification.

### 2.3 Field Arithmetic (Partial)

| Opcode | Signature | Status |
|--------|-----------|--------|
| `BaseAdd` | `(Base, Base) → Base` | ✅ Sound |
| `BaseMul` | `(Base, Base) → Base` | ✅ Sound |
| `BaseSub` | `(Base, Base) → Base` | ✅ Sound |
| `BaseDiv` | — | ❌ **Missing** |

**The Division Gap**: Field division $a / b$ requires computing $b^{-1}$ via extended Euclidean algorithm:

$$b^{-1} \equiv b^{p-2} \pmod{p} \text{ (Fermat's little theorem) }$$

This requires a **variable-base exponentiation** of ~254 doublings, which in a circuit costs approximately 254 constraint rows per division.

**Workaround via cross-multiplication**: To check $\frac{a}{b} < c$:

```zk
# Instead of: less_than_or_equal(base_div(a, b), c)
# Use:
temp = base_mul(b, c);
less_than_strict(a, temp);  # Proves a < b*c, equivalent to a/b < c when b > 0
```

This is sound but ** bloats circuits** — a 2-opcode check becomes 3-4x more expensive.

### 2.4 Comparison Operations (Critical Gaps)

| Opcode | Returns | Soundness | Status |
|--------|---------|-----------|--------|
| `LessThanStrict` | No | ✅ | ✅ Production |
| `LessThanLoose` | No | ✅ | ✅ Production |
| `IsEqualBase` | Yes | ❌ | ⚠️ Experimental |
| `LessThanOrEqual` | Yes | ❌ | ⚠️ Experimental |
| `BaseLtStrict` | Yes | Unknown | ⚠️ Experimental |
| `NotBase` | Yes | Unknown | ⚠️ Experimental |

**The Comparison Soundness Crisis**: Only `less_than_strict` and `less_than_loose` are **constrain-only** (they don't return values). All comparison opcodes that return values have soundness bugs:

**IsEqualBase bug**:
```zk
delta = base_sub(a, b)
delta_invert = field_inverse(delta)
# When a == b: delta = 0, and the constraint 0 * delta_invert == 1 is SKIPPED
# Prover can assign ANY value to delta_invert
```

**LessThanOrEqual bug**:
```zk
a_offset = out * (b - a) + (1 - out) * (a - b - 1)
out * (1 - out) = 0
# Prover choosing out=0 bypasses intended logic for a > b
```

### 2.5 Constraint Operations (Complete)

| Opcode | Signature | Status |
|--------|-----------|--------|
| `ConstrainEqualBase` | `(Base, Base) → ()` | ✅ |
| `ConstrainEqualPoint` | `(EcPoint, EcPoint) → ()` | ✅ |
| `ConstrainInstance` | `Base → ()` | ✅ |

**Assessment**: ✅ **Complete for equality constraints.**

### 2.6 Selection Operations (Complete)

| Opcode | Signature | Meaning |
|--------|-----------|---------|
| `CondSelect` | `(Base, Base, Base, Base) → Base` | `c ? a : b` |
| `ZeroCondSelect` | `(Base, Base, Base) → Base` | `a == 0 ? b : a` |

**Assessment**: ✅ **Complete for conditional selection.**

---

## 3. Critical Missing Opcodes for Full DeFi

### 3.1 BaseDiv — Field Division

**Why it's critical**: Every ratio check in DeFi requires division.

**Examples**:
- Liquidation threshold: `collateral / debt < 1.5`
- Interest rate: `balance * rate / 12`
- Exchange rate: `output_amount = input_amount * exchange_rate`
- Slippage: `output >= input * (1 - slippage)`

**Mathematical formulation**:
Given $a, b \in \mathbb{F}_p$, compute $c = a \cdot b^{-1} \pmod{p}$.

**Circuit cost estimate**:
- Extended Euclidean algorithm: ~500 gates
- Fermat exponentiation: 254 field multiplications
- Total: ~760 constraint rows per division

**Current workaround**: Cross-multiplication bloats ratio checks by 3-4x but remains sound.

### 3.2 SignatureVerify — External Chain Signatures

**Why it's critical**: Bridges cannot verify Ethereum/Bitcoin transactions without it.

**Required signatures**:
| Chain | Signature Scheme | Curve |
|-------|-----------------|-------|
| Ethereum | ECDSA (secp256k1) | ECDSA |
| Bitcoin | ECDSA (secp256k1) | ECDSA |
| Solana | Ed25519 | EdDSA |
| Zcash | RedDSA | Jubjub |

**Current bridge approach**: Trusted relayer attestation — not cryptographic verification.

**Mathematical formulation**:
For ECDSA verification of message hash $m$, public key $Q$, signature $(r, s)$:
1. Verify $r = x_0(H(m) \cdot s^{-1} \cdot G + s^{-1} \cdot Q)$
2. Where $H$ is Keccak-256 (Ethereum) or SHA-256 (Bitcoin)

This requires Keccak/SHA-256 hashing + secp256k1 arithmetic — both currently missing.

### 3.3 Sha256 / Keccak — Standard Hash Functions

**Why it's critical**: Cross-chain Merkle proofs use chain-specific hash functions.

**Ethereum Merkle Patricia Trie**:
- Hash: Keccak-256
- Encoding: RLP (Recursive Length Prefix)
- Node types: branch, leaf, extension, extension

**Circuit cost estimate**:
- Keccak-256: ~20,000 constraints per 256-bit block
- Full Merkle proof verification: ~200,000 constraints

This is **expensive but necessary** for production bridges.

### 3.4 PedersenCommit — Commitment Opening

**Why it's critical**: Confidential transactions require hiding values.

**Mathematical formulation**:
$$C = v \cdot H + r \cdot G$$

Where:
- $v$ is the value (hidden)
- $r$ is the blinding factor (randomness)
- $H, G$ are generators
- $C$ is the commitment (public)

Opening requires proving knowledge of $(v, r)$ such that $C = vH + rG$.

**Why missing**: The current `ec_mul` and `ec_add` can express this, but a **compound opcode** would be more efficient:

```zk
# Efficient (1 opcode):
commitment = pedersen_commit(value, randomness);

# Verbose (current workaround):
tmp1 = ec_mul(value, H_generator);
tmp2 = ec_mul(randomness, G_generator);
commitment = ec_add(tmp1, tmp2);
```

### 3.5 SetMembership — Prove x ∈ S Without Revealing x

**Why it's critical**: Allowlists, circuitbreakers, regulatory compliance.

**Mathematical formulations**:

**Option A: Merkle tree membership**
- $S$ is the set of allowed addresses
- Prove leaf index $i$ such that $\text{MerkleRoot}(path_i, S[i]) = \text{root}$
- Cost: One `MerkleRoot` call

**Option B: Polynomial commitment (Kate/Zaveracha)**
- $S = \{s_1, s_2, ..., s_n\}$
- Polynomial $f(x) = \prod_{i=1}^n (x - s_i)$
- Prove $f(x) = 0$ for claimed $x \in S$
- Cost: 1 pairings check

**Option C: Accumulators (RSA or bilinear)**
- Constant-size proof for any set membership
- Requires pairing-friendly curve operations

**Current workaround**: Poseidon-based Merkle tree with fixed depth — works for small sets but is not constant-size.

### 3.6 Power — Variable Exponentiation

**Why it's critical**: Exponential decay in vesting schedules, Dutch auctions, bonded curves.

**Mathematical formulation**:
$$r = a^b \pmod{p}$$

**Circuit cost**: $O(\log b)$ multiplications (repeated squaring).

**Example**: Bonding curve price $P = P_0 \cdot 2^{V / M}$:
```zk
# Instead of:
price = power(2, base_div(current_supply, M));
# Must use loop unrolling (not possible in zkas):
# price = 2^(V/M) via 30 multiplications
```

### 3.7 RangeProof — Aggregate Range Checks

**Why it's critical**: Multi-asset transactions require many range checks.

**Current**: Each `range_check(value, bit_width)` costs ~100 constraints.

**Batch approach** (Bulletproofs-style):
$$\text{AggregatedRangeProof}(v_1, ..., v_n, r_1, ..., r_n) \ll n \cdot \text{IndividualRangeProof}(v_i)$$

**Current workaround**: Sequential range checks — $n$ assets means $100n$ constraints.

---

## 4. Object Capability Security Patterns

### 4.1 Capability as Computational Object

In DarkFi's OCap model, a **capability** is a first-class cryptographic object:

```rust
struct Capability {
    issuer: EcPoint,           // Who granted this
    scope: EcPoint,            // What it applies to
    permissions: u8,           // Bitmask of allowed ops
    expiry: Option<Uint64>,    // Optional time bound
    nonce: EcPoint,            // Unpredictable element
}
```

**Derivation**:
$$\text{cap\_digest} = \text{PoseidonHash}(\text{issuer}, \text{scope}, \text{permissions}, \text{nonce})$$

**Why Poseidon?** It's:
- ZK-friendly (low constraint count)
- Collision-resistant
- Suitable for circuit computation

### 4.2 Opcode Requirements for OCap

**Minimum opcodes for OCap**:

| Opcode | Purpose |
|--------|---------|
| `poseidon_hash` | Derive capability digest |
| `ec_mul_var_base` | Verify issuer's public key |
| `constrain_equal_point` | Verify scope matches |
| `range_check(8, permissions)` | Ensure valid permission bitmask |
| `less_than_strict(expiry, now)` | Check not expired |

**Missing for advanced OCap**:

| Opcode | Enables |
|--------|---------|
| `set_membership` | Permission hierarchies |
| `pedersen_commit` | Delegation with hidden values |
| `ec_add` | Capability combination |
| `timestamp_range` | Time-delegation |

### 4.3 Revocation and Delegation

**Revocation**: Require the issuer to maintain a **revocation list** Merkle tree. The capability holder proves their nonce is NOT in the revocation tree.

```zk
# In the revoke.zk circuit:
not_revoked = merkle_check_revocation_list(cap_nonce, issuer_revroot);
constrain_equal_base(not_revoked, 1);
```

**Delegation**: Delegatee receives a new capability derived from delegator's:

$$\text{delegate\_cap} = \text{PoseidonHash}(\text{delegator\_cap}, \text{reduced\_permissions}, \text{delegatee\_id})$$

This creates a **delegation chain** that can be verified by traversing back to the issuer.

### 4.4 The Atomicity Problem

**Issue**: OCap revocation checks interact with complex DeFi logic atomically.

**Example**: A liquidation capability that:
1. Transfers collateral
2. Mints debt tokens
3. Updates price oracle
4. Revokes the liquidation capability

All 4 must succeed or fail together. Currently, DarkFi's transaction model doesn't support multi-contract atomicity — this requires a **transactional execution layer** on top of individual circuits.

---

## 5. Field Arithmetic Challenges

### 5.1 The Wraparound Invariant

**Critical theorem**: For any comparison opcode returning a boolean:

> **Wraparound Safety**: If input values are guaranteed to be in range $[0, 2^{k})$ where $k < 254 - 32$, then field arithmetic and integer arithmetic are identical.

**Proof**: Since $p \approx 2^{254}$ and $2^k < 2^{222}$, we have:
$$\forall a, b \in [0, 2^k): a < b \implies a < b \pmod{p}$$

**Implication**: The 32-bit safety margin in range checks protects against wraparound.

### 5.2 Division by Zero Prevention

Field division $a / b$ requires $b \neq 0$. When $b = 0$ in a circuit:

1. The prover could assign any value to $b^{-1}$
2. The constraint $b \cdot b^{-1} = 1$ would be unsatisfiable
3. BUT: If there's a selector that disables this constraint when $b = 0$...

**Current unsafe pattern** (in LessThanOrEqual):
```zk
# When out = 0, b - a could be 0, and its inverse is undefined
# The gate constraint is skipped, allowing exploitation
```

**Safe pattern**: Explicitly constrain $b \neq 0$ before division:
```zk
# Proves b != 0:
delta = base_sub(b, 0);  # delta = b
delta_invert = field_inverse(delta);
# Now use delta_invert only when delta != 0
# Selector ensures delta * delta_invert = 1 when delta != 0
```

### 5.3 The Comparison Soundness Matrix

| Opcode | Input Constraint | Returns Value | Soundness |
|--------|-----------------|---------------|-----------|
| `less_than_strict` | None | No | ✅ |
| `less_than_loose` | None | No | ✅ |
| `is_equal_base` | None | Yes | ❌ |
| `less_than_or_equal` | None | Yes | ❌ |
| `is_equal_base` | `range_check(253, a)` | Yes | ⚠️ |
| `less_than_or_equal` | `range_check(253, a)` | Yes | ⚠️ |

**Key insight**: Restricting inputs to $[0, 2^{253})$ eliminates the wraparound region, but the **delta-invert bug** remains.

### 5.4 The Extended Euclidean Circuit

Computing $a^{-1} \pmod{p}$ via extended Euclidean algorithm:

```rust
// Input: a ∈ ℤ/pℤ, a ≠ 0
// Output: a^{-1} ∈ ℤ/pℤ

let mut t = 0;
let mut new_t = 1;
let mut r = p;
let mut new_r = a;

while new_r != 0 {
    let quotient = r / new_r;
    let tmp_t = new_t;
    new_t = t - quotient * new_t;
    t = tmp_t;
    let tmp_r = new_r;
    new_r = r - quotient * new_r;
    r = tmp_r;
}

if r > 1 { return Error("Not invertible"); }
if t < 0 { t = t + p; }
return t;
```

**Circuit representation**: Each division step requires comparison and subtraction. The loop runs at most 254 iterations. This is **not practical** as a plain-circuit implementation — needs specialized gadget with precomputed tables.

---

## 6. The Complete Opcode Roadmap

### Tier 1: Critical (Required for Production DeFi)

| Opcode | Rationale | Complexity | Status |
|--------|-----------|------------|--------|
| `base_div` | Every ratio check | High | Not implemented |
| `signature_verify(secp256k1)` | Ethereum bridge | Very High | Not implemented |
| `keccak256` | Ethereum Merkle proofs | Very High | Not implemented |

### Tier 2: High Value (Major Feature Enablers)

| Opcode | Rationale | Complexity | Status |
|--------|-----------|------------|--------|
| `sha256` | Bitcoin bridge | Very High | Not implemented |
| `pedersen_commit` | Confidential DeFi | Medium | Not implemented |
| `set_membership` | Allowlists, revocations | Medium | Not implemented |
| `power` | Vesting, bonding curves | Medium | Not implemented |

### Tier 3: Nice to Have (Efficiency Gains)

| Opcode | Rationale | Complexity | Status |
|--------|-----------|------------|--------|
| `range_proof_batch` | Multi-asset privacy | High | Not implemented |
| `ed25519_verify` | Solana compatibility | Very High | Not implemented |
| `ecdsa_verify` | Bitcoin legacy | Very High | Not implemented |

### Tier 4: Theoretical (Future Expansion)

| Opcode | Rationale | Complexity | Status |
|--------|-----------|------------|--------|
| `pairing_check` | BBS+ signatures, ZK-Rollups | Extreme | Not implemented |
| `fft` | Polynomial operations | High | Not implemented |
| `sort_verify` | Order book circuits | High | Not implemented |

### Note on Comparison Opcodes: Safemath vs Native Opcode

`LessThanOrEqual` and `IsEqualBase` are implemented (experimental, grey-market).

**Formal Verification Results** (see [Opcodes and Formal Verification](opcodes.md)):
- `LessThanOrEqual` (0x55): ✅ **Verified Sound** via Lean 4
- `IsEqualBase` (0x54): ❌ **Bug Confirmed** - delta_invert unconstrained when `a == b`
- `NotBase` (0x56): ✅ **Verified Sound**
- `BaseLtStrict` (0x57): ✅ **Verified Sound**

**Safemath workaround**: For assertion-only use cases (no Boolean return value), the [darkfi-safemath](https://codeberg.org/rusticml/darkfi-safemath) library provides production-ready templates using sound `less_than_strict` + `base_add` + `range_check`.

**Native opcode still needed**: When a circuit requires a Boolean return value (e.g., for public output or `CondSelect`), the native opcode is still required. Safemath cannot replace this.

**Current status**: stablecoin and identity use safemath for assertion-only checks. `LessThanOrEqual` is verified sound for bounded inputs.

**See**: [Safemath](../safemath.md) for the workaround, [zkVM Primitive Layer](zkvm_primitives.md) for native opcode status.

---

## 7. Mathematical Universe Summary

**What DarkFi CAN Express** (with current opcodes):

- ✅ Public key derivation (ECDSA on Jubjub)
- ✅ Poseidon-based Merkle trees (fixed depth 32)
- ✅ Arbitrary field arithmetic circuits
- ✅ Boolean comparison logic (`LessThanOrEqual`, `BaseLtStrict` — formally verified sound)
- ✅ Field division (`BaseDiv` opcode 0x58 — implemented via binary exponentiation)
- ✅ Object capability permissions (basic)
- ✅ Single-asset confidential transfers (via ec_mul/ec_add workaround)
- ✅ DAO governance with voting

**What DarkFi CANNOT Express** (missing opcodes):

- ❌ Cross-chain signature verification (ETH, BTC, Solana)
- ❌ Standard hash functions (SHA-256, Keccak)
- ❌ Constant-size set membership proofs
- ❌ Variable-time exponentiation
- ❌ Confidential multi-asset transactions

**Theoretical Maximum**: With all Tier 1-3 opcodes implemented, DarkFi could express:

1. **Uniswap-style AMM** with constant product formula
2. **Liquity-style lending** with排骨 redemption
3. **MakerDAO-style CDP** with oracle price feeds
4. **Lightning Network**-style HTLC
5. **Aztec-style private DeFi**

The mathematical universe is bounded only by constraint density and proving time. Each opcode added expands the expressible problem space exponentially.

---

## See Also

- [Opcodes and Formal Verification](opcodes.md) — Soundness verification status, Lean 4 proofs, and outstanding work
- [Merkle Depth Limitation](merkle_depth.md) — Fixed-depth constraints and workarounds
- [Bridge Contract Architecture](../contract/bridge/README.md) — How Merkle proofs are used in production
- [dao/exec.zk](../../src/contract/dao/proof/exec.zk) — Cross-multiplication pattern example
- [halo2_gadgets::sinsemilla](https://docs.rs/halo2_gadgets) — Underlying Halo2 implementation
