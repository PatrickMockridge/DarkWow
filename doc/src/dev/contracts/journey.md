# Contract Developer Journey

A step-by-step walkthrough from zero to a deployed contract on DarkWow.

## Prerequisites

- Rust toolchain (1.87.0+)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- Built DarkWow: `make` from repo root

## 1. Understand the architecture

Before writing code, understand what makes DarkWow contracts different:

1. **Every state transition is proven in ZK.** Contracts are WASM modules that
   execute inside the zkVM. You write a ZKAS circuit for each function, a Rust
   entrypoint for state transitions, and a client builder for constructing
   transactions.

2. **O-Cap, not ACL.** Contracts hold only the capabilities explicitly passed
   to them. No `msg.sender`, no global state, no ambient authority.

3. **The money split.** Consensus-critical operations (coinbase, fees, supply
   audit) live in `native_token`. DeFi token operations (transfer, mint, burn)
   live in `promissory_note`. Your contract calls the one it needs — or both.

4. **Manifest-first.** Every contract declares its interface in a `manifest.toml`.
   The wallet reads this to auto-configure. No hardcoded ABIs.

Read the [Contract Developer Guide](../../for-contract-developers.md) for the
full architecture overview, and [Smart Contract Inherent Safety](../contracts/safety.md)
for the 20 vulnerability lessons — **before you write a line of code.**

## 2. Pick a template contract

The fastest way to start is to copy an existing contract that matches your
use case:

| If you're building... | Use as template... | Directory |
|---|---|---|
| Token operations | PromissoryNote | `src/contract/promissory_note/` |
| Governance / treasury | DAO Escrow | `src/contract/dao_escrow/` |
| Gaming / randomness | DarkToshi Dice | `src/contract/darktoshi_dice/` |
| Financial instrument | Stablecoin | `src/contract/stablecoin/` |
| Identity / credentials | Identity | `src/contract/identity/` |

Every contract in `src/contract/<name>/` follows the same structure:
```
src/contract/<name>/
  Cargo.toml          # Workspace member, wasm32 target
  Makefile             # Build + test commands
  manifest.toml        # Capability declarations
  src/
    lib.rs             # Contract entrypoints + function enum
    model.rs           # Parameter structs, state types
    client/            # Transaction builders
  proof/               # ZKAS circuits (.zk files)
  tests/               # Integration tests
```

## 3. Write the manifest

Create `manifest.toml` in your contract directory. This declares what your
contract does and how the wallet discovers its capabilities:

```toml
[contract]
name = "my_contract"
category = "DeFi"
description = "What this contract does"
version = "0.1.0"

[[functions]]
name = "transfer"
code = 0
description = "Transfer tokens"
requires_proof = true
proof_circuit = "Transfer_V1"

[[trees]]
name = "coins"
description = "Coin commitment Merkle tree"

[[trees]]
name = "nullifiers"
description = "Nullifier SMT for double-spend prevention"
```

See [Manifest System](../../arch/manifest.md) for the full specification.

## 4. Write the ZK circuit

Circuits are written in ZKAS (zero-knowledge assembly). Create your circuit
in `proof/<name>.zk`. See [Writing ZK Proofs](../../zkas/writing-zk-proofs.md)
and the [zkas compiler](../../zkas/zkas.md) documentation.

Key constraints every circuit must enforce:
- Value conservation (when moving coins)
- Nullifier derivation from a secret the prover knows
- Merkle proof verification (for coin inclusion)
- Spend hook validation (if composable)

Study the existing circuits in `src/contract/promissory_note/proof/` for patterns.

## 5. Write the Rust entrypoint

Your contract's `src/lib.rs` needs:

1. **A function enum**: Each function gets a unique code and a WASM export.
2. **Model structs**: Typed parameter structs using DarkWow's newtype wrappers
   (`PublicKey`, `TokenId`, `Nullifier`, `CoinCommitment` — never raw `[u8; 32]`).
3. **Entrypoint handlers**: `get_metadata`, `process_instruction`, `process_update`
   for each function.

See the existing contracts for patterns. Specifically:
- `src/contract/promissory_note/src/entrypoint/` for token operations
- `src/contract/dao_escrow/src/entrypoint/` for governance operations

## 6. Write the client builder

Client code in `src/client/mod.rs` builds transactions that users can submit.
The wallet uses these builders to construct and prove transactions.

Use the typed wrapper constructors:
```rust
let recipient = PublicKey::from_bytes(bytes)?;  // not (x, y) pairs
let token = TokenId::from_bytes(bytes)?;          // not [u8; 32]
let nullifier = Nullifier::from(secret, coin);    // derived, not zero
```

## 7. Test

Run through the [testing pipeline](../testing/overview.md):

| Level | Command | Catches |
|---|---|---|
| 1 — Lightweight | `cargo test -p dwowd test_all_contracts_deploy` | Compilation, deploy, serialization |
| 2 — Heavyweight | `RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd test_heavyweight` | ZK proof failures, state machine bugs |
| 3 — Localnet | `contrib/docker/darkwow-testnet/test_pipeline.sh --mode native` | P2P, mining, block propagation, wallet sync |

Run the [safety checklist](checklist.md) before each level. Never skip a level.

## 8. Deploy

Contracts are deployed via the [Deployooor](../../contract/deployooor.md) contract.
On devnet:
```bash
cargo run -p dwowd -- --network darkwow-devnet
```

Use `contract.deploy` via JSON-RPC (devnet only) or submit a deployment
transaction through the Deployooor contract.

## Reference

- [Contract Standards](standards.md) — Naming conventions, structure, code patterns
- [Rust-WASM Interaction](../rust-wasm-interaction.md) — Host function reference
- [ZK Circuit Troubleshooting](../zk-circuit-troubleshooting.md)
- [Contract Invoke API](../../arch/contract_invoke_api.md)
- [Formal Specification](../../arch/formal-specification.md)
