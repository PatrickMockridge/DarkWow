# Scheme

> [!WARNING] DEPRECATED — No corresponding `src/contract/vesting/` exists. This spec contains 13 bare TODOs and is retained for reference only.

<!-- toc -->

Let $\t{Params}_\t{VC}, \t{Bulla}_\t{VC}$ be defined as in
[Vesting Configuration Model](model.md).

Let $\t{Commitment}$ be defined as in the section [Commitment][1].

Let $ℙₚ, 𝔽ₚ, \mathcal{X}, \mathcal{Y}, \t{𝔹⁶⁴2𝔽ₚ}$ be defined as in the
section [Pallas and Vesta][2].

Let $t₀ = \t{BlockWindow} ∈ 𝔽ₚ$ be the current blockwindow as defined
in [Blockwindow](model.md#blockwindow).

Let $\t{PoseidonHash}$ be defined as in the section
[PoseidonHash Function](../../crypto-schemes.md#poseidonhash-function).

Let $\t{ElGamal.Encrypt}, \t{ElGamalEncNote}ₖ$ be defined as in the
section [Verifiable In-Band Secret Distribution][3].

Denote the Vesting contract ID by $\t{CID}_\t{V} ∈ 𝔽ₚ$ and its `Exec`
function spend hook by $\t{SH}_\t{V} ∈ 𝔽ₚ$.

## Vest

This function creates a vesting configuration bulla $ℬ_\t{VC}$. We
commit to the vesting configuration params and then add the bulla to
the set, along with the vested commitment $\t{Commitment}$ minted by the child
`Money::transfer()` call. Each vesting configuration keeps track of its
minted commitments, to ensure that only those can be burned in next actions,
creating a sequence of commitments, enabling the contract to keep track of
remaining balances anonymously. Additionally, we verify the minted
vesting commitment is encrypted for the configuration shared secret key,
ensuring both parties have access to it.

* Wallet builder: `TODO: add client path`
* WASM VM code: `TODO: add entrypoint path`
* ZK proof: `TODO: add proof path`

### Function Params

Define the vest function params
$$ \begin{aligned}
  ℬ_\t{VC} &∈ \t{im}(\t{Bulla}_\t{VC}) \\
  \t{SPK} &∈ ℙₚ
\end{aligned} $$

```rust
TODO: Add call params path
```

### Contract Statement

**Vesting configuration bulla uniqueness** &emsp; whether $ℬ_\t{VC}$
already exists. If yes then fail.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  VAx &∈ 𝔽ₚ \\
  VPK &∈ 𝔽ₚ \\
  Sx &∈ 𝔽ₚ \\
  τ &∈ 𝔽ₚ \\
  T &∈ ℕ₆₄ \\
  C &∈ ℕ₆₄ \\
  S &∈ ℕ₆₄ \\
  E &∈ ℕ₆₄ \\
  V &∈ ℕ₆₄ \\
  b_\t{VC} &∈ 𝔽ₚ \\
  b_\t{Commitment} &∈ 𝔽ₚ
\end{aligned} $$

Attach a proof $π$ such that the following relations hold:

**Proof that start blockwindow is greater than current blockwindow**
&emsp; $S > t₀$.

**Proof that end blockwindow is greater than start blockwindow** &emsp;
$E > S$.

**Proof that total is greater than cliff** &emsp; $T >= C$.

**Proof that blockwindow value is valid** &emsp; $T == (E - S) * V +
C$.

**Proof of vesting authority public key ownership** &emsp; $\t{VAPK} =
\t{DerivePubKey}(VAx)$.

**Proof of shared secret public key ownership** &emsp; $\t{SPK} =
\t{DerivePubKey}(Sx)$.

**Vesting configuration bulla integrity** &emsp; $ℬ =
\t{Bulla}_\t{VC}(\mathcal{X}(p.\t{VAPK}), \mathcal{Y}(p.\t{VAPK}),
\mathcal{X}(p.\t{VPK}), \mathcal{Y}(p.\t{VPK}),
\mathcal{X}(p.\t{SPK}), \mathcal{Y}(p.\t{SPK}), t, T, C, S, E, V,
b_\t{VC})$

**Minted vested commitment integrity** &emsp; $Commitment =
\t{PoseidonHash}(\mathcal{X}(p.\t{SPK}), \mathcal{Y}(p.\t{SPK}), T, t,
\t{CID}_\t{V}, \t{SH}_\t{V}, ℬ, b_\t{Commitment})$

**Verifiable vested commitment note encryption** &emsp;
let $𝐧 = (c.v, c.τ, c.\t{SH}, c.\t{UD}, c.n)$, and verify
$a = \t{ElGamal}.\t{Encrypt}(𝐧, \t{esk}, d.\t{SPK})$.

### Signatures

There should be a single signature attached, which uses
$\t{SPK}$ as the signature public key.

## Withdraw

This function enables the vestee to withdraw the corresponding unlocked
value up to that blockwindow. The child `Money::transfer()` call must
contain a single input, the vested commitment we burn, and two outputs. The
first one being the withdrawed one while the second one is the
remaining vested balance commitment. Both commitments values are verified by the
vesting configuration rules, and we store the second one as the current
vested commitment, to burn in next actions. Additionally, we verify the
second/vested commitment is encrypted for the configuration shared secret
key, ensuring both parties have access to it.

* Wallet builder: `TODO: add client path`
* WASM VM code: `TODO: add entrypoint path`
* ZK proof: `TODO: add proof path`

### Function Params

Define the withdraw function params
$$ \begin{aligned}
  ℬ_\t{VC} &∈ \t{im}(\t{Bulla}_\t{VC}) \\
  \t{SPK} &∈ ℙₚ
\end{aligned} $$

```rust
TODO: Add call params path
```

### Contract Statement

**Vesting configuration bulla existance** &emsp; whether $ℬ_\t{VC}$
exists. If no then fail.

**Burned vested commitment existance** &emsp; whether the burned commitment
$\t{BCommitment}$ matches the vesting configuration record one. If no then
fail.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  VAPK &∈ 𝔽ₚ \\
  Vx &∈ 𝔽ₚ \\
  Sx &∈ 𝔽ₚ \\
  τ &∈ 𝔽ₚ \\
  T &∈ ℕ₆₄ \\
  C &∈ ℕ₆₄ \\
  S &∈ ℕ₆₄ \\
  E &∈ ℕ₆₄ \\
  V &∈ ℕ₆₄ \\
  b_\t{VC} &∈ 𝔽ₚ \\
  Bv &∈ ℕ₆₄ \\
  b_\t{BCommitment} &∈ 𝔽ₚ \\
  x_c &∈ 𝔽ₚ \\
  Cv &∈ ℕ₆₄ \\
  b_\t{Commitment} &∈ 𝔽ₚ
\end{aligned} $$

Attach a proof $π$ such that the following relations hold:

**Proof of vestee public key ownership** &emsp; $\t{VPK} =
\t{DerivePubKey}(Vx)$.

**Proof of shared secret public key ownership** &emsp; $\t{SPK} =
\t{DerivePubKey}(Sx)$.

**Vesting configuration bulla integrity** &emsp; $ℬ =
\t{Bulla}_\t{VC}(\mathcal{X}(p.\t{VAPK}), \mathcal{Y}(p.\t{VAPK}),
\mathcal{X}(p.\t{VPK}), \mathcal{Y}(p.\t{VPK}),
\mathcal{X}(p.\t{SPK}), \mathcal{Y}(p.\t{SPK}), t, T, C, Cb, S, E, V,
b_\t{VC})$

**Proof that current blockwindow is greater than start blockwindow**
&emsp; $t₀ >= S$.

TODO: cond_select statement to pick current or end blockwindow

**Proof of withdraw amount correctness** &emsp;
$$ \begin{aligned}
CurrentBlockwindow = CondSelect(BlockwindowCond, t₀, E); \\
BlockwindowsPassed = CurrentBlockwindow - S; \\
Available = (BlockwindowsPassed * V) + C; \\
Withdrawn = T - Bv; \\
WithdrawlCommitmentValue = Available - Withdrawn; \\
VestingChangeValue = T - (Withdrawn + WithdrawlCommitmentValue);
\end{aligned} $$

Verify the child `Money::transfer()` call correctnes:

**Burned vested commitment integrity** &emsp; $BCommitment =
\t{PoseidonHash}(\mathcal{X}(p.\t{SPK}), \mathcal{Y}(p.\t{SPK}),
Bv, t, \t{CID}_\t{V}, \t{SH}_\t{V}, ℬ, b_\t{Commitment})$

**Burned vested commitment nullifier integrity** &emsp; $\cN =
\t{PoseidonHash}(x_c, BCommitment)$

**Minted vested commitment integrity** &emsp; $Commitment =
\t{PoseidonHash}(\mathcal{X}(p.\t{SPK}), \mathcal{Y}(p.\t{SPK}),
VestingChangeValue, t, \t{CID}_\t{V}, \t{SH}_\t{V}, ℬ, b_\t{Commitment})$

**Verifiable vested commitment note encryption** &emsp;
let $𝐧 = (c.v, c.τ, c.\t{SH}, c.\t{UD}, c.n)$, and verify
$a = \t{ElGamal}.\t{Encrypt}(𝐧, \t{esk}, d.\t{SPK})$.

### Signatures

There should be a single signature attached, which uses
$\t{SPK}$ as the signature public key.

## Forfeit

This function enables the vesting authority to forfeit a vesting
configuration, withdrawing the rest of vested value. The child
`Money::transfer()` call must containg a single input, the vested commitment
we burn, and a single output, the newlly minted commitment. Both commitments values
are verified by the vesting configuration rules, and we remove the
vesting configuration bulla $ℬ_\t{VC}$ entry from the set.

* Wallet builder: `TODO: add client path`
* WASM VM code: `TODO: add entrypoint path`
* ZK proof: `TODO: add proof path`

### Function Params

Define the vest function params
$$ \begin{aligned}
  ℬ_\t{VC} &∈ \t{im}(\t{Bulla}_\t{VC}) \\
  \t{SPK} &∈ ℙₚ
\end{aligned} $$

```rust
TODO: Add call params path
```

### Contract Statement

**Vesting configuration bulla existance** &emsp; whether $ℬ_\t{VC}$
exists. If no then fail.

**Burned vested commitment existance** &emsp; whether the burned commitment
$\t{BCommitment}$ matches the vesting configuration record one. If no then
fail.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  VAx &∈ 𝔽ₚ \\
  VPK &∈ 𝔽ₚ \\
  Sx &∈ 𝔽ₚ \\
  τ &∈ 𝔽ₚ \\
  T &∈ ℕ₆₄ \\
  C &∈ ℕ₆₄ \\
  S &∈ ℕ₆₄ \\
  E &∈ ℕ₆₄ \\
  V &∈ ℕ₆₄ \\
  b_\t{VC} &∈ 𝔽ₚ
  Bv &∈ ℕ₆₄ \\
  b_\t{BCommitment} &∈ 𝔽ₚ \\
  x_c &∈ 𝔽ₚ
\end{aligned} $$

Attach a proof $π$ such that the following relations hold:

**Proof of vesting authority public key ownership** &emsp; $\t{VAPK} =
\t{DerivePubKey}(VAx)$.

**Proof of shared secret public key ownership** &emsp; $\t{SPK} =
\t{DerivePubKey}(Sx)$.

**Vesting configuration bulla integrity** &emsp; $ℬ =
\t{Bulla}_\t{VC}(\mathcal{X}(p.\t{VAPK}), \mathcal{Y}(p.\t{VAPK}),
\mathcal{X}(p.\t{VPK}), \mathcal{Y}(p.\t{VPK}),
\mathcal{X}(p.\t{SPK}), \mathcal{Y}(p.\t{SPK}), t, T, C, S, E, V,
b_\t{VC})$

**Proof of forfeit amount correctness** &emsp; $ForfeitValue = T - Bv$

Verify the child `Money::transfer()` call correctnes:

**Burned vested commitment integrity** &emsp; $BCommitment =
\t{PoseidonHash}(\mathcal{X}(p.\t{SPK}), \mathcal{Y}(p.\t{SPK}),
ForfeitValue, t, \t{CID}_\t{V}, \t{SH}_\t{V}, ℬ, b_\t{Commitment})$

**Burned vested commitment nullifier integrity** &emsp; $\cN =
\t{PoseidonHash}(x_c, BCommitment)$

**Minted commitment integrity** &emsp;
let $c.\t{CID}, c.\t{SH}, c.\t{UD}$ be the vesting authority
chosen Contract ID, spend hook and user data for the minted commitment,
and verify $Commitment = \t{PoseidonHash}(\mathcal{X}(p.\t{VAPK}),
\mathcal{Y}(p.\t{VAPK}), ForfeitValue, t, \t{CID}, \t{SH},
\t{UD}, b_\t{Commitment})$

### Signatures

There should be a single signature attached, which uses
$\t{SPK}$ as the signature public key.

[1]: Commitment = H(attributes) — see native_token/src/model/mod.rs
[2]: ../../crypto-schemes.md#pallas-and-vesta
[3]: ../../crypto-schemes.md#verifiable-in-band-secret-distribution
