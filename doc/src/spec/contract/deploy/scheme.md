# Scheme

Let $ℙₚ$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

## Deploy

This function initializes a smart contract deployment.

* Wallet builder: `src/contract/deployooor/src/client/deploy_v1.rs`
* WASM VM code: `src/contract/deployooor/src/entrypoint/deploy.rs`

### Function Params

Define the deploy params
$$ \begin{aligned}
  \t{Params}_\t{Deploy}.WASM &∈ \t𝔹^* \\
  \t{Params}_\t{Deploy}.PK &∈ ℙₚ \\
  \t{Params}_\t{Deploy}.IX &∈ 𝔹^* \\
  \t{Params}_\t{Deploy}.SINGLETON &∈ \{0, 1\} \\
  \t{Params}_\t{Deploy}.NAME &∈ \t{String} \\
\end{aligned} $$

`SINGLETON` and `NAME` carry the genesis singleton rule: when `SINGLETON = 1`, a
deployment whose `NAME` is already claimed by another contract is rejected. It exists so
critical ecosystem infrastructure — promissory note replicas — cannot be name-squatted.

```rust
{{#include ../../../../../src/sdk/src/deploy.rs:deploy-deploy-params}}
```

### Contract Statement

**Contract deployment status** &emsp; whether the contract is locked. If yes then fail.

**WASM bincode validity** &emsp; whether the provided `WASM` bincode is valid. If no then fail.

### Signatures

There should be a single signature attached, which uses $\t{PK}$ as the
signature public key.

## Lock

This function finalizes the smart contract state definition.

* Wallet builder: `src/contract/deployooor/src/client/lock_v1.rs`
* WASM VM code: `src/contract/deployooor/src/entrypoint/lock.rs`

### Function Params

Define the lock params
$$ \begin{aligned}
  \t{Params}_\t{Lock}.PK &∈ \tℙₚ \\
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/deployooor/src/model.rs:deploy-lock-params}}
```

### Contract Statement

**Contract deployment status** &emsp; whether the contract is already locked. If yes then fail.

### Signatures

There should be a single signature attached, which uses $\t{PK}$ as the
signature public key.
