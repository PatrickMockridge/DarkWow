# Base Field Arithmetic: The Invisible Wall

This document is about the fundamental constraint that shapes everything DarkFi's smart contracts can and cannot do: **the base field arithmetic wall**.

Read this before writing any ZK circuit. If you have spent any time writing ZK circuits, this will feel obvious. If you have not, it will feel like being told the floor is made of lava.

---

## The Core Problem: Normal Math Does Not Apply

In standard programming, integers behave like integers:

```python
# Every programmer expects this:
x = 5
y = 10
assert x < y          # True
assert x + y == 15    # True
assert x * y == 50    # True
```

In a ZK circuit, you are not working with integers. You are working with **field elements** — members of a finite cyclic group defined by a large prime `p`. For DarkFi's Pallas field:

```
p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1
```

All arithmetic wraps at `p`. This breaks every intuition:

```python
# Field arithmetic:
x = p - 1     # This equals -1 (mod p)
y = 1
x < y         # False! In the field, p-1 < 1 is FALSE
              # because p-1 ≡ -1, and -1 is "large" as an integer
```

This is not a bug. It is the nature of finite fields. But it means that **every operation you want to express must be re-expressed as field arithmetic** — and that re-expression is where the difficulty lives.

---

## What You Take For Granted in Normal Code

Here is a list of operations that are trivial in any programming language but become **research problems** in ZK circuits:

| Normal operation | ZK circuit reality |
|-----------------|-------------------|
| `a < b` (comparison) | Must encode integer ordering using range checks; field wraparound near `p` inverts the result |
| `a / b` (division) | No native division. Prove `c = a / b` by asserting `a = b * c`; requires multiplicative inverse |
| `a > 0` (sign check) | No sign bit in the field. Prove `a > 0` by proving `a < p/2` — requires bounding the range |
| `min(a, b)` | Must be expressed as a constrained conditional using comparison results |
| `a % m` (modulus) | Requires division circuit plus remainder check |
| `a ^ b` (exponentiation) | Requires repeated multiplication or a dedicated `BaseModExp` gadget |
| `sqrt(a)` | Requires field square root algorithm in circuit form |

None of these are impossible. All of them require careful circuit design, and getting them wrong in ways that are subtle and hard to test.

---

## The Comparison Example: Why It Matters

This is the single most important example because it appears everywhere in DarkFi contracts.

**What you want to enforce**: "the liquidator reward must not exceed the collateral"

**What the circuit must prove**: `reward <= collateral` as integers

**The naive attempt**:
```zk
result = less_than_strict(reward, collateral);  # ERROR: returns ()
constrain_equal_base(result, 1);                  # Can't use the result
```

`LessThanStrict` in DarkFi's zkVM returns nothing — it only **constrains** the circuit to fail if `reward >= collateral`. It is unusable as a value in subsequent logic.

**Why this limitation exists**: Making comparison return a value (`0` or `1`) in a ZK circuit requires:

1. Computing `a - b` in the field
2. Determining whether `a - b` is in `{0, 1, ..., (p-1)/2}` or `{(p+1)/2, ..., p-1}`
3. Encoding that decision as a field element `out` that can be used by other gates
4. Constraining `out` to be exactly `0` or `1`

Step 3 is where it breaks down. The prover can assign any `out` value they like and then set the gate's other inputs to satisfy the constraint. The range check limits this, but does not eliminate it. This is why `LessThanOrEqual` is an **experimental opcode** — its soundness properties are not fully proven.

The fix (when someone does it properly) requires an explicit `is_zero` gadget — a separate piece of circuit logic that correctly constrains the case where `a == b`. That gadget itself requires careful design.

---

## The Division Example: Cross-Multiplication

Division is instructive because there is a pattern that often eliminates the need for it entirely.

**What you want**: `collateral / debt < liquidation_threshold`

**What you do in a ZK circuit**: You almost never compute the division. Instead you cross-multiply:

```zk
# Instead of: collateral / debt < threshold
# Prove:     collateral < threshold * debt
lhs = base_mul(collateral, 1);
rhs = base_mul(threshold, debt);
less_than_strict(lhs, rhs);  # Uses only base_mul + less_than_strict
```

This is exactly what `dao/exec.zk` does for its approval ratio check (lines 118-126). The moral: **express your invariant as a constraint, not as an algorithm**.

When you catch yourself thinking "I need to compute X" in a ZK circuit, ask instead: "what constraint on X do I need to enforce?" The answer is usually cheaper and more sound than computing the value.

---

## The Theoretical Universe vs. The Practical Wall

There is a sense in which ZK circuits can express **arbitrary computation** — given enough gates and enough prover time, any function can be approximated. This is the "complete mathematical universe" of what DarkFi's contracts could theoretically do.

But that universe is separated from practice by the **field arithmetic wall**:

1. **Expressing** a computation as a ZK circuit requires re-expressing it in field arithmetic
2. **Implementing** that circuit requires gadgets that may not exist yet (`LessThanOrEqual`, for example)
3. **Verifying** those gadgets requires understanding their soundness properties (can a malicious prover bypass them?)
4. **Deploying** those gadgets is irreversible — there is no upgrade path for buggy arithmetic in deployed circuits

This is why even humans and AI together are still exploring the boundary. The mathematical universe is vast. The subset that is **practically expressible** in a ZK circuit with current gadget libraries, audit status, and upgrade constraints — that subset is much smaller, and the frontier moves slowly.

---

## Why You Need to Understand This

If you are contributing to DarkFi's contract layer, this matters to you directly:

**If you are writing circuit code**: Every time you reach for a comparison, a division, a modulus, or a conditional, you are crossing the field arithmetic wall. You need to know whether the opcode you need exists, whether it is experimental, and whether a cross-multiplication workaround exists instead.

**If you are designing a contract**: Your beautiful DeFi primitive might have a clean mathematical specification that becomes a 500-gate circuit nightmare because of field arithmetic constraints. The design space is not just "what do we want to compute" — it is also "what can be computed in a ZK circuit with acceptable prover cost and soundness properties."

**If you are reviewing a contract**: Soundness bugs in field arithmetic are subtle. A circuit that looks correct might be bypassable by a malicious prover assigning field elements that satisfy the constraints while violating the intended invariant. Understanding the math is how you catch these.

---

## What This Means for DarkFi's Roadmap

DarkFi's theoretical contract capability is enormous. In practice, the roadmap is constrained by:

- **Which field arithmetic gadgets are implemented** — and correctly implemented, with proven soundness
- **Which gadgets are audited** — an implementation is not production-ready just because it compiles
- **Which gadgets have integration tests** — an opcode working in isolation is not the same as it working inside a real circuit
- **Which gadgets have a clear upgrade path** — once a buggy arithmetic gadget is deployed, it cannot be fixed without a hard fork

The `LessThanOrEqual` opcode took an experimental fork to prototype, multiple iterations to integrate, and is still grey-market goods because the soundness analysis is incomplete. A `BaseDiv` or `BaseModExp` opcode would take similar effort.

This is why the opcode primitives documentation is on the roadmap. It is not academic — it determines what contracts can actually exist on DarkFi.

---

## The Practical Mindset

When approaching ZK circuit design, internalize this:

> **"Every value is a field element. Every operation is modular. Every comparison is a range check. Every division is a multiplication with a proof."**

If you find yourself thinking in integers, stop. Ask: what is the field element equivalent? What constraint do I actually need to enforce? Can I express this as a cross-multiplication or a range check instead of a computed value?

The contracts that ship are the ones whose authors learned to think in field arithmetic.

---

## Further Reading

- [zkVM Primitive Layer](zkvm_primitives.md) — Technical analysis of each implemented and planned opcode
- [Contract MVP Status](mvp_status.md) — What contracts are blocked by which arithmetic gaps
- [The Complete Mathematical Universe](https://technologytruth.substack.com/p/the-complete-mathematical-universe) — The broader context for why ZK arithmetic is hard
- [Pallas Field Notes](https://hackmd.io/@poroo/H0_LdDzqy) — Field arithmetic edge cases for bn254
- [ZK Maths Discord](https://discord.gg/zk) — Active community working through these problems

---

## Appendix: The Field Boundary Problem

For reference, here is exactly where integer ordering breaks down in the Pallas field.

Values in the range `[0, 2^253)` behave like integers — the field representation and the integer value are in the same order. Beyond `2^253`, wraparound begins:

```
Range [0, 2^253):           Safe — field order == integer order
Range [2^253, p):            Dangerous — field order ≠ integer order
Range [p - 2^32, p):         Very dangerous — common arithmetic gives
                             inverted results for comparisons and signs
```

The current comparison gadgets handle this by **constraining all inputs to `[0, 2^253)`** before comparison. This works, but it means:

1. You must add explicit `range_check(253, x)` before using `x` in a comparison
2. Values near the boundary (e.g., a token amount of 10^18 when expressed in field elements) need validation
3. Any gadget that does not enforce this range is unsound for large inputs

This is why input validation is not just good practice in DarkFi circuits — it is a security requirement.
