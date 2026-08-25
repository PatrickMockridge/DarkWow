# Contract Safety Checklist

Pre-commit gates extracted from the 20 vulnerability lessons documented in
[Smart Contract Inherent Safety](safety.md). Each item below maps to a real
bug found and fixed in DarkWow's contracts.

> **Prerequisite**: Read [safety.md](safety.md) before using this checklist.
> This is a quick reference, not a substitute.

## Consensus-Critical Contracts

For any contract whose failure could halt the chain (NativeToken, Deployooor):

- [ ] **Nullifier zero-rejection**: Every nullifier path rejects `Nullifier::zero()`. A zero nullifier matches every commitment. See safety.md lesson #1.
- [ ] **Handle allocation vs query**: `db_lookup` allocates a handle without querying sled — it always returns `Ok`. Use `db_contains_key` for idempotency guards. See safety.md lesson #2.
- [ ] **Per-block key derivation**: Miner and wallet compute `sk_H = derive_instance(sk, cid, height)` independently. No shared state. Must produce identical output.
- [ ] **Cumulative supply verification**: `Σ outputs + Σ burns + Σ fees == Σ inputs` enforced at every block acceptance path with no bypass.
- [ ] **No `unwrap_or(zero)` on typed identifiers**: `from_repr().unwrap_or(zero)` silently substitutes zero for invalid data. Every typed identifier uses fallible `from_bytes` → `Result`.
- [ ] **Ephemeral signatures**: Every signature uses a fresh per-transaction secret. The wallet secret is never used as a signing key.

## DeFi / Application Contracts

For contracts that handle user funds (PromissoryNote, Stablecoin, Bridge, DEX, etc.):

- [ ] **Two-step auth audit**: Every authorization that spans multiple function calls is replaced with a single-step ZK proof. The proof IS the authorization. No "step 1 creates an artifact, step 2 checks it" patterns. See safety.md lesson #3.
- [ ] **Child call verification**: When a child contract moves value, the parent verifies the amount — no trusting off-chain infrastructure. See safety.md lesson #5.
- [ ] **Input nullifier binding**: Every input nullifier is bound to the operation it authorizes. A nullifier valid for "transfer 5 DRKW" cannot authorize "transfer 500 DRKW." See safety.md lesson #17.
- [ ] **Parent call validation**: Every parent call check verifies both `contract_id` AND `func_code`. Checking only `contract_id` allows any function in the parent contract to authorize. See safety.md lesson #18.
- [ ] **Value conservation in ZK**: Every value transformation (fee subtraction, interest accrual, exchange rate) that happens off-circuit is constrained in-circuit. The Rust client is a convenience, not a security boundary. See safety.md lesson #20.
- [ ] **Structural conservation is not enough**: 1-in-1-out commitment structure does not imply value conservation. The circuit must explicitly constrain input.value == output.value (or the intended transformation).

## ZK Circuit Development

For any new or modified ZK circuit:

- [ ] **Witness derivation constrained**: Every witness used for authorization (mint_public, auth_parent, spend_hook) has its derivation constrained in the circuit. An aspirational comment in the Rust code is not a constraint. See safety.md lesson #19.
- [ ] **No free variables in auth checks**: If a witness is compared against on-chain state, the circuit proves the prover knows the secret that derived it.
- [ ] **Merkle tree off-circuit parity**: Every hash function used in a circuit opcode has a matching off-circuit implementation with identical output. See safety.md lesson #16.
- [ ] **Opaque field audit**: Fields committed into commitment hashes or passed as ZK public inputs do not carry identity-derived data. Authorization goes in nullifiers, not auxiliary data. See safety.md lesson #8.
- [ ] **Token ID unlinkability**: Token IDs are derived with randomized inputs. A token's existence reveals nothing about who created it. See safety.md lesson #9.
- [ ] **Bincode format sync**: Every ZKAS `constant` block has a matching Rust struct with identical field order, types, and encoding. When the format changes, both sides change together. See safety.md lesson #15.

## Pre-Deployment Gates

Run before deploying any contract, even in devnet:

- [ ] `cargo test -p dwowd test_all_contracts_deploy` — Level 1 deployment
- [ ] `./bin/dwowd/src/tests/heavyweight.sh --all` — Level 2 ZK proofs
- [ ] Python model tests pass for the relevant contract (if a model exists)
- [ ] Capability descriptor updated: every new entrypoint function has a matching descriptor action. See safety.md lesson #10.
- [ ] `cargo check --tests` clean — zero warnings in the contract crate

> **USE AT YOUR OWN RISK.** These checklists are derived from internal review.
> No third-party audit has been performed.
