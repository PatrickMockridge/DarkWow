# Smart Contract Inherent Safety

## Fundamentals

Smart contract safety begins with a counterintuitive principle: **the safest code is the code you never write**. Every feature added to a contract is a potential vulnerability. Every code path is an attack surface. Every authorization check is a point of failure.

This is not a statement about code quality — it's about combinatorial complexity. A contract with 3 functions has a manageable set of state transitions to audit. A contract with 12 functions, ACL-gated minting, governance-controlled parameters, and cross-contract child calls has an exponentially larger space of possible interactions to verify.

### The Principle of Minimum Functionality

```
Security ∝ 1 / (features × code_paths × authorization_gates)
```

Three corollaries follow:

1. **Isolate blast radius**: Put the minimum viable logic in the most frequently called contracts. Sophisticated business logic goes in less critical contracts where failures are contained.

2. **Remove, don't gate**: If you think a feature needs an ACL gate to be safe, ask whether the feature should exist at all. Authorization is itself an attack surface — every permission check is a place an attacker can try to bypass.

3. **Separate concerns by failure cost**: A bug in a DEX loses user funds for that trade. A bug in the consensus token loses block rewards for every miner. These are not the same severity.

---

## Design Exemplar: NativeToken vs MoneyV3

DarkWow's token architecture is the concrete expression of these principles. It splits token functionality across two contracts with deliberately asymmetric safety requirements.

### NativeToken: Consensus Safety by Minimum Functionality

NativeToken handles exactly what consensus requires — block rewards, fee payment, and value transfer. It is **deliberately minimal**:

| What it does | What it deliberately omits |
|---|---|
| PoW block rewards (PoWRewardV1) | No token freezing |
| Network fee payment (FeeV1) | No governance coupling |
| Private transfers (Mint/Burn/Transfer) | No multi-token support |
| | No authorization gates |
| | No token registry |
| | No business logic |

Every omission is a security property. No freeze means no freeze-key attack. No governance coupling means no plutocratic takeover of consensus. No multi-token support means no token-ID confusion attacks. No authorization gates means no auth bypass.

The principle: **in consensus-critical code, the feature you don't add is the vulnerability you don't create.** NativeToken is the most frequently called contract in the system. A bug here cascades to every transaction, every block, every miner reward.

### MoneyV3: Minimum Viable Business Logic for DeFi

MoneyV3 carries the business logic that DeFi contracts need to compose — multi-token support, authorization, and cross-contract value verification. It is still minimal by DeFi standards (no AMM, no lending pools, no governance), but it carries more logic than NativeToken because composition demands it:

| What it adds | Why it's needed |
|---|---|
| TokenMintV1 / AuthTokenMintV1 | Permissionless token creation for stablecoins, wrapped assets, LP tokens |
| Multi-token support (token_id) | DEX, lending, yield — all need multiple token types |
| Token registry | Prevents unauthorized minting of unregistered token types |
| public_value on Output | Parent contracts can verify child transfer amounts |
| TransferOutput_V1 ZK circuit | Proves public values match encrypted coin attributes |
| BlindOutput_V1 ZK circuit | Proves fully private output coins are correctly formed |
| validate_child_transfer_value | Helper for parent contracts to verify child call amounts |

### Why Not One Contract?

A monolithic token contract that handles both consensus and DeFi creates a single point of failure — a bug in DeFi token logic can break consensus. By separating them:

1. **Failure isolation**: A bug in MoneyV3 cannot break NativeToken. Mining rewards and fees keep flowing regardless.
2. **Different audit postures**: Consensus tokens need maximum security review; DeFi tokens need flexibility. One codebase can't optimize for both.
3. **Independent evolution**: The consensus token can remain frozen while DeFi tokens evolve.
4. **Process safety**: Developers working on DeFi features don't touch consensus-critical code.

```
┌──────────────────────────────────────┐
│           NativeToken                 │
│  Consensus only — block rewards, fees │
│  MINIMAL by design                    │
│  No freezing, no auth, no registry   │
│  Blast radius: ENTIRE NETWORK        │
└──────────────┬───────────────────────┘
               │
┌──────────────┴───────────────────────┐
│           MoneyV3                     │
│  DeFi composition — multi-token, auth │
│  MINIMAL VIABLE for DeFi              │
│  Business logic, cross-contract calls │
│  Blast radius: individual tokens     │
└──────────────┬───────────────────────┘
               │
┌──────────────┴───────────────────────┐
│     DeFi Contracts (DEX, Bridge...)   │
│  Application logic lives here        │
│  Blast radius: individual operations │
└──────────────────────────────────────┘
```

---

## Hardening Lessons: What Can Go Wrong

The following sections describe real vulnerabilities that were identified through security review and their mitigations. Each represents a class of bug that can occur in any contract.

### Lesson 1: Authorization Gaps — The Token Registry

**The vulnerability**: MoneyV3's `MintV1` accepted an `auth_proof` struct containing a nullifier, mint authority public key, and token registry Merkle root. The ZK circuit constrained these values against private witnesses, so the proof verified correctly. But the on-chain contract **never checked that the nullifier was actually spent** — it never verified that `AuthTokenMintV1` had been called first.

Anyone could call `MintV1` with arbitrary `auth_proof` data. As long as the ZK proof verified (which only required knowing a valid auth secret), the mint would succeed. The two-phase authorization model (AuthTokenMintV1 → MintV1) existed on paper but wasn't enforced on-chain.

**The fix**: Three changes close this gap:

1. **Token registry Merkle tree** — `TokenMintV1` stores the `token_id` in an on-chain registry. `MintV1` and `AuthTokenMintV1` both check that the `token_id` exists before proceeding. A token must be registered before it can be minted.

2. **Auth nullifier verification** — `MintV1` performs an SMT lookup on the nullifiers tree to verify that `auth_proof.nullifier` was marked spent by a prior `AuthTokenMintV1` call. The ZK proof alone is not sufficient — on-chain state must corroborate it.

3. **Token registry root tracking** — The registry has its own Merkle tree with historical roots, so `AuthTokenMintV1` can prove token existence against a specific root. This enables light client verification of token authorization.

**The principle**: **ZK proofs constrain witness relationships, not on-chain state.** You must verify that the public inputs to a ZK proof correspond to actual on-chain data. A valid proof of a valid witness does not mean the witness was produced by a valid prior state transition.

### Lesson 2: Cross-Contract Routing — The Opcode Collision

**The vulnerability**: Every parent contract validates child calls by checking `child_call.data[0]` — the function opcode byte. But `0x04` is used by both `MoneyV3::TransferV1` and `Attestation::VerifyClaimV1`. A contract like `labor_market::create_job_v1` checks `data[0] == 0x04` expecting a money transfer, while `labor_market::submit_deliverable_v1` checks `data[0] == 0x04` expecting attestation verification. The contracts never validate `child_call.contract_id`.

If a malicious transaction builder swapped the `contract_id` for a child call, the parent would accept the wrong child function — the opcode matches, but the contract being called is wrong. The WASM runtime dispatches by `contract_id`, so the call goes to the intended contract, but the parent's validation is blind to which contract that is.

**The fix**: Two complementary defenses:

1. **Contract ID validation helper** — `validate_child_contract_id(child_contract_id, expected_contract_id)` provides a standard way for parent contracts to verify the target contract, not just the function code. This should be called after the opcode check.

2. **Value amount validation** — Even with contract_id validation, parent contracts should verify the transfer amount via `validate_child_transfer_value`. This requires the child transfer to use `public_value` on its outputs, backed by a `TransferOutput_V1` ZK proof. A child call that passes both contract_id AND amount validation cannot be a misrouted call to the wrong contract.

**The principle**: **Validate the target, not just the action.** Checking `data[0]` tells you what function will run, but not what contract will run it. Always validate `contract_id` alongside function code, and validate amount/value fields when the child call moves assets.

### Lesson 3: Unproven Outputs — The Blind Output Gap

**The vulnerability**: TransferV1 and OtcSwapV1 outputs without `public_value` had **no ZK proof of correct coin formation**. The TransferOutput_V1 circuit only covered outputs with public values (for cross-contract composition). Fully private outputs were created client-side and inserted into the transaction without any ZK constraint proving:

- The coin commitment is correctly computed from the attributes
- The value commitment matches the value and blind
- The value is within 64-bit range

The only on-chain check was coin uniqueness — preventing duplicate coin commitments but not proving correct formation. A buggy client could produce malformed coins that would be accepted on-chain.

**The fix**: A new `BlindOutput_V1` ZK circuit (Poseidon-only, no EC) proves correct coin formation for private outputs. The circuit constrains `coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)` and `value_commit = poseidon_hash(value, value_blind)` as public inputs, with a 64-bit range check on value. Every TransferV1 and OtcSwapV1 output now has a ZK proof — either TransferOutput_V1 (revealing values for cross-contract composition) or BlindOutput_V1 (keeping values private while proving correctness).

**The principle**: **Every output must have a ZK proof of correct formation.** Client-side construction is not sufficient — the network must be able to verify that every coin commitment and value commitment is correctly computed. Without this, buggy or malicious clients can inject arbitrary coins.

### Lesson 4: Composition Amount Blindness

**The vulnerability**: Before the cross-contract composition refactor, parent contracts called `money_v3::transfer_v1` as a child call but could not verify the transfer amount. The amount was encrypted inside `AeadEncryptedNote` (which the parent can't decrypt), and the `value_commit` was a Poseidon hash (the parent doesn't know the blind). A parent like a bridge or DEX that expects a transfer of 1000 tokens had no way to verify that the child call actually transferred 1000 tokens — only that a TransferV1 call existed.

**The fix**: `Output` structs gained optional `public_value: Option<u64>` and `public_token_id: Option<pallas::Base>` fields, backed by a `TransferOutput_V1` ZK circuit that constrains these equal to the encrypted coin attributes. Parent contracts call `validate_child_transfer_value` to verify the amount. The bridge contract now validates child transfer amounts in WithdrawV1, CancelWithdrawV1, and ExecuteGuaranteedWithdrawV1. The stablecoin contract validates them in OpenPositionV1, AddCollateralV1, and RepayStableV1.

**The principle**: **A child call's existence is not proof of its correctness.** When a child call moves value, the parent must verify the amount. Relying on the transaction builder to set the right amount is trusting off-chain infrastructure with on-chain correctness.

---

## Security Checklist for Contract Developers

### Consensus-Critical Contracts

If your contract handles block rewards, fee payment, or any function that the network cannot function without:

- [ ] No governance coupling — no one can vote to change its behavior
- [ ] No authorization gates — no freeze, no ACL, no permissioned minting
- [ ] No multi-token support — single asset, no token-ID confusion possible
- [ ] Minimum functions — if a feature can live in a separate contract, it should
- [ ] Every output has a ZK proof — no client-side-only coin construction
- [ ] Poseidon-only circuits — no EC operations in internal ZK circuits

### DeFi / Application Contracts

If your contract composes with other contracts and handles user funds:

- [ ] Validate `contract_id` on child calls, not just `data[0]`
- [ ] Validate child transfer amounts via `validate_child_transfer_value`
- [ ] Every authorization model has on-chain state backing — ZK proofs alone are not enough
- [ ] Registries exist for any resource that must be "registered before use" (tokens, members, etc.)
- [ ] Nullifier-based replay prevention for all authorization operations
- [ ] Merkle proofs for all existence checks against growing datasets
- [ ] Child call validation happens in the `instruction` phase, before state mutation
- [ ] All database trees are initialized in `init_contract`

### ZK Circuit Development

- [ ] Is EC required? If this is an internal DarkWow circuit, use Poseidon-only
- [ ] Every output coin has a circuit proving correct formation (either BlindOutput or TransferOutput)
- [ ] Public inputs to the circuit are verified against on-chain state in the entrypoint
- [ ] Range checks on all value fields (64-bit for coin values)
- [ ] Nullifier uniqueness is checked both in the circuit AND in the on-chain nullifiers tree

---

## References

- [NativeToken](./native_token.md) — Consensus token with zero business logic
- [MoneyV3](./money_v3.md) — DeFi token with minimum viable composition logic
- [Standards](./standards.md) — ZK circuit, token, and testing standards
- [Composability](../../contract/composability.md) — Cross-contract child call patterns
- [MoneyV3 Migration](../../contract/money_v3_migration.md) — Architecture rationale for the hard fork that separated NativeToken and MoneyV3
