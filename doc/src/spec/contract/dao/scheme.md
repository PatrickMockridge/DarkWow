# Scheme

<!-- toc -->

Let $\t{PoseidonHash}$ be defined as in the section [PoseidonHash Function](../../crypto-schemes.md#poseidonhash-function).

Let $𝔽ₚ, ℙₚ, \t{DerivePubKey}, \t{Lift}_q, G_N, \mathcal{X}, \mathcal{Y}$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

Let $\t{PedersenCommit}$ be defined as in the section [Homomorphic Pedersen Commitments](../../crypto-schemes.md#homomorphic-pedersen-commitments).

Let $\t{MerklePos}, \t{MerklePath}, \t{MerkleRoot}$ be defined as in the section [Incremental Merkle Tree](../../crypto-schemes.md#incremental-merkle-tree).

Let $\t{Params}_\t{DAO}, \t{Bulla}_\t{DAO}, \t{Params}_\t{Proposal}, \t{Bulla}_\t{Proposal}$ be defined as in [DAO Model](model.md).

Let $\t{AeadEncNote}$ be defined as in [In-band Secret Distribution](../../crypto-schemes.md#in-band-secret-distribution).

Let $\t{ElGamal.Encrypt}, \t{ElGamalEncNote}ₖ$ be defined as in the section [Verifiable In-Band Secret Distribution](../../crypto-schemes.md#verifiable-in-band-secret-distribution).

## Mint

This function creates a DAO bulla $𝒟 $. It's comparatively simple- we commit to
the DAO params and then add the bulla to the set.

* Wallet builder: `src/contract/dao/src/client/mint.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/mint.rs`
* ZK proof: `src/contract/dao/proof/mint.zk`

### Function Params

Define the DAO mint function params
$$ \begin{aligned}
  𝒟 &∈ \t{im}(\t{Bulla}_\t{DAO}) \\
  \t{PK} &∈ ℙₚ
\end{aligned} $$

```rust
pub struct DaoMintParams {
    pub dao_bulla: pallas::Base,
    pub public_key: pallas::Point,
}
```

### Contract Statement

**DAO bulla uniqueness** &emsp; whether $ℬ $ already exists. If yes then fail.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  L &∈ ℕ₆₄ \\
  Q &∈ ℕ₆₄ \\
  EEQ &∈ ℕ₆₄ \\
  A^\% &∈ ℕ₆₄ × ℕ₆₄ \\
  τ &∈ 𝔽ₚ \\
  Nx &∈ 𝔽ₚ \\
  px &∈ 𝔽ₚ \\
  Px &∈ 𝔽ₚ \\
  Vx &∈ 𝔽ₚ \\
  Ex &∈ 𝔽ₚ \\
  EEx &∈ 𝔽ₚ \\
  b_\t{DAO} &∈ 𝔽ₚ
\end{aligned} $$

Attach a proof $π$ such that the following relations hold:

**Proof of notes public key ownership** &emsp; $\t{NPK} = \t{DerivePubKey}(Nx)$.

**Proof of proposer public key ownership** &emsp; $\t{pPK} = \t{DerivePubKey}(px)$.

**Proof of proposals public key ownership** &emsp; $\t{PPK} = \t{DerivePubKey}(Px)$.

**Proof of votes public key ownership** &emsp; $\t{VPK} = \t{DerivePubKey}(Vx)$.

**Proof of executor public key ownership** &emsp; $\t{EPK} = \t{DerivePubKey}(Ex)$.

**Proof of early executor public key ownership** &emsp; $\t{EEPK} = \t{DerivePubKey}(EEx)$.

**Proof that early execution quorum is greater than normal quorum** &emsp; $Q <= EEQ1$.

**DAO bulla integrity** &emsp; $ℬ  = \t{Bulla}_\t{DAO}((L, Q, EEQ, A^\%, τ,
\t{NPK}, \t{pPK}, \t{PPK}, \t{VPK}, \t{EPK}, \t{EEPK}), b_\t{DAO})$

### Signatures

There should be a single signature attached, which uses
$\t{NPK}$ as the signature public key.

## Propose

This contract function creates a DAO proposal. It takes a merkle root
$R_\t{DAO}$ which contains the DAO bulla created in the Mint phase.

Several inputs are attached containing proof of ownership for the governance
token. This is to satisfy the proposer limit value set in the DAO.
We construct the nullifier $\cN$ which can leak anonymity when those same
coins are spent. To workaround this, wallet implementers can attach an
additional `Money::transfer()` call to the transaction.

The nullifier $\cN$ proves the coin isn't already spent in the set determined
by $R_\t{coin}$. Each value commit $V$ exported by the input is summed and
used in the main proof to determine the total value attached in the inputs
crosses the proposer limit threshold.

This is merely a proof of ownership of holding a certain amount of value.
Coins are not locked and continue to be spendable.

Additionally the encrypted note $\t{note}$ is used to send the proposal
values to the DAO members using the public key set inside the DAO.

A proposal contains a list of auth calls as specified in [Auth Calls](model.md#auth-calls). This specifies the contract call executed by the DAO on passing.

* Wallet builder: `src/contract/dao/src/client/propose.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/propose.rs`
* ZK proofs:
  * `src/contract/dao/proof/propose-main.zk`
  * `src/contract/dao/proof/propose-input.zk`

### Function Params

Define the DAO propose function params
$$ \begin{aligned}
  R_\t{DAO} &∈ 𝔽ₚ \\
  T &∈ 𝔽ₚ \\
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  \t{note} &∈ \t{AeadEncNote} \\
  𝐢 &∈ \t{ProposeInput}^*
\end{aligned} $$

Define the DAO propose-input function params
$$ \begin{aligned}
  \t{ProposeInput}.\cN &∈ 𝔽ₚ \\
  \t{ProposeInput}.V &∈ ℙₚ \\
  \t{ProposeInput}.R_\t{coin} &∈ 𝔽ₚ \\
  \t{ProposeInput}.\t{PK}_σ &∈ ℙₚ
\end{aligned} $$

```rust
pub struct DaoProposeParams {
    pub dao_merkle_root: pallas::Base,
    pub token_commit: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub encrypted_note: AeadEncryptedNote,
    pub inputs: Vec<ProposeInput>,
}

pub struct ProposeInput {
    pub nullifier: pallas::Base,
    pub value_commit: pallas::Point,
    pub coin_merkle_root: pallas::Base,
    pub signature_public_key: pallas::Point,
}
```

### Contract Statement

Let $t₀ = \t{BlockWindow} ∈ 𝔽ₚ$ be the current blockwindow as defined in [Blockwindow](model.md#blockwindow).

Let $\t{Attrs}_\t{Coin}$ be defined as in [Coin](../money/model.md#coin).

**Valid DAO bulla merkle root** &emsp; check that $R_\t{DAO}$ is a previously
seen merkle root in the DAO contract merkle roots DB.

**Proposal bulla uniqueness** &emsp; whether $𝒫 $ already exists. If yes then fail.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  v &∈ 𝔽ₚ \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  (ψ, Π) &∈ \t{MerklePos} × \t{MerklePath} \\
\end{aligned} $$
Attach a proof $π_𝒫 $ such that the following relations hold:

**Governance token commit** &emsp; export the DAO token ID as an encrypted pedersen
commit $T = \t{PedersenCommit}(d.τ, b_τ)$ where $T = ∑_{i ∈ 𝐢} Tᵢ$.

**Proof of proposer public key ownership** &emsp; $\t{pPK} = \t{DerivePubKey}(px)$.

**DAO bulla integrity** &emsp; $𝒟  = \t{Bulla}_\t{DAO}(d, b_d)$

**DAO existence** &emsp; $R_\t{DAO} = \t{MerkleRoot}(ψ, Π, 𝒟 )$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $p.t₀ = t₀$.

**Proposer limit threshold met** &emsp; check the proposer has supplied enough
inputs that the required funds for the proposer limit set in the DAO is met.
Let the total funds $v = ∑_{i ∈ 𝐢} i.v$, then check $d.L ≤ v$.

**Total funds value commit** &emsp; $V = \t{PedersenCommit}(v, bᵥ)$ where
$V = ∑_{i ∈ 𝐢} i.V$. We use this to check that $v = ∑_{i ∈ 𝐢} i.v$ as
claimed in the *proposer limit threshold met* check.

For each input $i ∈ 𝐢$, perform the following checks:

&emsp; **Unused nullifier** &emsp; check that $\cN$ does not exist in the
money contract nullifiers DB.

&emsp; **Valid input coins merkle root** &emsp; check that $i.R_\t{coin}$ is a
previously seen merkle root in the money contract merkle roots DB.

&emsp; Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  x_c &∈ 𝔽ₚ \\
  c &∈ \t{Attrs}_\t{Coin} \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  (ψᵢ, Πᵢ) &∈ \t{MerklePos} × \t{MerklePath} \\
  x_σ &∈ 𝔽ₚ \\
\end{aligned} $$
&emsp; Attach a proof $π_i$ such that the following relations hold:

&emsp; **Nullifier integrity** &emsp; $\cN = \t{PoseidonHash}(x_c, C)$

&emsp; **Coin value commit** &emsp; $i.V = \t{PedersenCommit}(c.v, bᵥ)$.

&emsp; **Token commit** &emsp; $T = \t{PoseidonHash}(c.τ, b_τ)$.

&emsp; **Valid coin** &emsp; Check $c.P = \t{DerivePubKey}(x_c)$. Let $C = \t{Coin}(c)$. Check $i.R_\t{coin} = \t{MerkleRoot}(ψᵢ, Πᵢ, C)$.

&emsp; **Proof of signature public key ownership** &emsp; $i.\t{PK}_σ = \t{DerivePubKey}(x_σ)$.

### Signatures

For each $i ∈ 𝐢$, attach a signature corresponding to the
public key $i.\t{PK}_σ$.

## Vote

After `DAO::propose()` is called, DAO members can then call this contract
function. Using a similar method as before, they attach inputs proving ownership
of a certain value of governance tokens. This is how we achieve token weighted
voting. The result of the vote is communicated to DAO members that can view votes
through the encrypted note $\t{note}$.

Each nullifier $𝒩 $ is stored uniquely per proposal. Additionally as before,
there is a leakage here connecting the coins when spent. However prodigious
usage of `Money::transfer()` to wash the coins after calling `DAO::vote()`
should mitigate against this attack. In the future this can be fixed using
set nonmembership primitives.

Another leakage is that the proposal bulla $𝒫 $ is public. To ensure every vote
is discoverable by verifiers (who cannot decrypt values) and protect against
'nothing up my sleeve', we link them all together. This is so the final tally
used for executing proposals is accurate.

The total sum of votes is represented by the commit $V_\t{all} = ∑_{i ∈ 𝐢} i.V$
and the yes votes by $V_\t{yes}$.

* Wallet builder: `src/contract/dao/src/client/vote.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/vote.rs`
* ZK proofs:
  * `src/contract/dao/proof/vote-main.zk`
  * `src/contract/dao/proof/vote-input.zk`

### Function Params

Define the DAO vote function params
$$ \begin{aligned}
  τ &∈ 𝔽ₚ \\
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  V_\t{yes} &∈ ℙₚ \\
  \t{enc\_vote} &∈ \t{ElGamalEncNote}₄ \\
  𝐢 &∈ \t{VoteInput}^*
\end{aligned} $$

Define the DAO vote-input function params
$$ \begin{aligned}
  \t{VoteInput}.𝒩 &∈ 𝔽ₚ \\
  \t{VoteInput}.V &∈ ℙₚ \\
  \t{VoteInput}.R_\t{coin} &∈ 𝔽ₚ \\
  \t{VoteInput}.\t{PK}_σ &∈ ℙₚ
\end{aligned} $$

**Note**: $\t{VoteInput}.V$ is a pedersen commitment, where the blinds are
selected such that their sum is a valid field element in $𝔽ₚ$ so the blind
for $∑ V$ can be verifiably encrypted. Likewise we do the same for the blind
used to calculate $V_\t{yes}$.

This allows DAO members that hold the votes key to securely receive all secrets
for votes on a proposal. This is then used in the Exec phase when we work on the
sum of DAO votes.

```rust
pub struct DaoVoteParams {
    pub token_id: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub yes_vote_commit: pallas::Point,
    pub encrypted_vote: ElGamalEncryptedNote,
    pub inputs: Vec<VoteInput>,
}

pub struct VoteInput {
    pub vote_nullifier: pallas::Base,
    pub value_commit: pallas::Point,
    pub coin_merkle_root: pallas::Base,
    pub signature_public_key: pallas::Point,
}
```

### Contract Statement

Let $t₀ = \t{BlockWindow} ∈ 𝔽ₚ$ be the current blockwindow as defined in [Blockwindow](model.md#blockwindow).

**Proposal bulla exists** &emsp; check $𝒫 $ exists in the DAO contract proposal
bullas DB.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  o &∈ 𝔽ₚ \\
  b_y &∈ 𝔽ₚ \\
  v &∈ 𝔽ₚ \\
  bᵥ &∈ 𝔽ₚ \\
  b_τ &∈ 𝔽ₚ \\
  t_\t{now} &∈ 𝔽ₚ \\
  \t{esk} &∈ 𝔽ₚ \\
\end{aligned} $$
Attach a proof $π_\mathcal{V}$ such that the following relations hold:

**Governance token commit** &emsp; export the DAO token ID as an encrypted pedersen
commit $T = \t{PedersenCommit}(d.τ, b_τ)$ where $T = ∑_{i ∈ 𝐢} Tᵢ$.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$

**Yes vote commit** &emsp; $V_\t{yes} = \t{PedersenCommit}(ov, \t{Lift}_q(b_y))$

**Total vote value commit** &emsp; $V_\t{all} = \t{PedersenCommit}(v, \t{Lift}_q(bᵥ))$ where
$V_\t{all} = ∑_{i ∈ 𝐢} i.V$ should also hold.

**Vote option boolean** &emsp; enforce $o ∈ \{ 0, 1 \}$.

**Proposal not expired** &emsp; let $t_\t{end} = ℕ₆₄2𝔽ₚ(p.t₀) + ℕ₆₄2𝔽ₚ(p.D)$,
and then check $t_\t{now} < t_\t{end}$.

**Verifiable encryption of vote commit secrets** &emsp;
let $𝐧 = (o, b_y, v, bᵥ)$, and verify
$\t{enc\_vote} = \t{ElGamal}.\t{Encrypt}(𝐧, \t{esk}, d.\t{VPK})$.

For each input $i ∈ 𝐢$, perform the following checks:

&emsp; **Valid input merkle root** &emsp; check that $i.R_\t{coin}$ is the
previously seen merkle root in the proposal snapshot merkle root.

&emsp; **Unused nullifier (money)** &emsp; check that $\cN$ does not exist in the
money contract nullifiers DB.

&emsp; **Unused nullifier (proposal)** &emsp; check that $\cN$ does not exist in the
DAO contract nullifiers DB for this specific proposal.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  x_c &∈ 𝔽ₚ \\
  c &∈ \t{Attrs}_\t{Coin} \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  (ψᵢ, Πᵢ) &∈ \t{MerklePos} × \t{MerklePath} \\
  x_σ &∈ 𝔽ₚ \\
\end{aligned} $$
Attach a proof $πᵢ$ such that the following relations hold:

&emsp; **Nullifier integrity** &emsp; $\cN = \t{PoseidonHash}(x_c, C)$

&emsp; **Coin value commit** &emsp; $i.V = \t{PedersenCommit}(c.v, bᵥ)$.

&emsp; **Token commit** &emsp; $T = \t{PoseidonHash}(c.τ, b_τ)$.

&emsp; **Valid coin** &emsp; Check $c.P = \t{DerivePubKey}(x_c)$. Let $C = \t{Coin}(c)$. Check $i.R_\t{coin} = \t{MerkleRoot}(ψᵢ, Πᵢ, C)$.

&emsp; **Proof of signature public key ownership** &emsp; $i.\t{PK}_σ = \t{DerivePubKey}(x_σ)$.

### Signatures

For each $i ∈ 𝐢$, attach a signature corresponding to the
public key $i.\t{PK}_σ$.

## Exec

Exec is the final stage after voting is [Accepted](concepts.md#proposal-states).

It checks that voting has passed, and correct conditions have been met, in accordance
with the [DAO params](model.md#dao) such as quorum and approval ratio.
$V_\t{yes}$ and $V_\t{all}$ are pedersen commits to $v_\t{yes}$ and $v_\t{all}$ respectively.

It also checks that child calls have been attached in accordance with the auth
calls set inside the proposal. One of these will usually be an auth module
function. Currently the DAO provides a single preset for executing
`Money::transfer()` calls so DAOs can manage anonymous treasuries.

* Wallet builder: `src/contract/dao/src/client/exec.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/exec.rs`
* ZK proofs:
  * `src/contract/dao/proof/exec.zk`
  * `src/contract/dao/proof/early-exec.zk`

### Function Params

Let $\t{AuthCall}, \t{Commit}_{\t{Auth}^*}$ be defined as in the section [Auth Calls](model.md#auth-calls).

Define the DAO exec function params
$$ \begin{aligned}
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  𝒜  &∈ \t{AuthCall}^* \\
  V_\t{yes} &∈ ℙₚ \\
  V_\t{all} &∈ ℙₚ \\
\end{aligned} $$

```rust
pub struct DaoExecParams {
    pub proposal_bulla: pallas::Base,
    pub auth_calls: Vec<DaoAuthCall>,
    pub yes_vote_commit: pallas::Point,
    pub all_vote_commit: pallas::Point,
}

pub struct DaoBlindAggregateVote {
    pub yes_votes: u64,
    pub all_votes: u64,
    pub yes_blind: pallas::Scalar,
    pub all_blind: pallas::Scalar,
}
```

### Contract Statement

There are two phases to Exec. In the first we check the calling format of this
transaction matches what is specified in the proposal. Then in the second phase,
we verify the correct voting rules.

**Auth call spec match** &emsp; denote the child calls of Exec by $C$.
If $\#C ≠ \#𝒜 $ then exit.
Otherwise, for each $c ∈ C$ and $a ∈ 𝒜 $, check the function ID of $c$ is $a$.

**Aggregate votes lookup** &emsp; using the proposal bulla, fetch the
aggregated votes from the DB and verify $V_\t{yes}$ and $V_\t{all}$ are set correctly.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  v_y &∈ 𝔽ₚ \\
  v_a &∈ 𝔽ₚ \\
  b_y &∈ 𝔽ᵥ \\
  b_a &∈ 𝔽ᵥ \\
\end{aligned} $$
Attach a proof $π$ such that the following relations hold:

**Proof of executor public key ownership** &emsp; $\t{EPK} = \t{DerivePubKey}(Ex)$.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $p.𝒜  = 𝒜 $.

**Proposal has expired** &emsp; let $t_\t{end} = ℕ₆₄2𝔽ₚ(p.t₀) + ℕ₆₄2𝔽ₚ(p.D)$,
and then check $t_\t{end} <= t_\t{now}$.

**Yes vote commit** &emsp; $V_\t{yes} = \t{PedersenCommit}(v_y, b_y)$

**All vote commit** &emsp; $V_\t{all} = \t{PedersenCommit}(v_a, b_a)$

**All votes pass quorum** &emsp; $Q ≤ v_a$

**Approval ratio satisfied** &emsp; we wish to check that
$\frac{A^\%_q}{A^\%_b} ≤ \frac{v_y}{v_a}$. Instead we perform the
equivalent check that $v_a A^\%_q ≤ v_y A^\%_b$.

### EarlyExec

This is a special case of Exec for when we want to execute a strongly accepted proposal
before voting period has passed. A different proof statement is used in this case.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  v_y &∈ 𝔽ₚ \\
  v_a &∈ 𝔽ₚ \\
  b_y &∈ 𝔽ᵥ \\
  b_a &∈ 𝔽ᵥ \\
\end{aligned} $$
Attach a proof $π$ such that the following relations hold:

**Proof of executor public key ownership** &emsp; $\t{EPK} = \t{DerivePubKey}(Ex)$.

**Proof of early executor public key ownership** &emsp; $\t{EEPK} = \t{DerivePubKey}(EEx)$.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $p.𝒜  = 𝒜 $.

**Proposal has not expired** &emsp; let $t_\t{end} = ℕ₆₄2𝔽ₚ(p.t₀) + ℕ₆₄2𝔽ₚ(p.D)$,
and then check $t_\t{now} < t_\t{end}$.

**Yes vote commit** &emsp; $V_\t{yes} = \t{PedersenCommit}(v_y, b_y)$

**All vote commit** &emsp; $V_\t{all} = \t{PedersenCommit}(v_a, b_a)$

**All votes pass early execution quorum** &emsp; $EEQ ≤ v_a$

**Approval ratio satisfied** &emsp; we wish to check that
$\frac{A^\%_q}{A^\%_b} ≤ \frac{v_y}{v_a}$. Instead we perform the
equivalent check that $v_a A^\%_q ≤ v_y A^\%_b$.

### Signatures

No signatures are attached.

## AuthMoneyTransfer

This is a child call for Exec which can be used for DAO treasuries.
It checks the next sibling call is `Money::transfer()` and accordingly
verifies the first $n - 1$ output coins match the data set in this
call's [auth data](model.md#auth-calls).

Additionally we provide a note with the coin params that are verifiably
encrypted to mitigate the attack where Exec is called, but the supplied
`Money::transfer()` call contains an invalid note which cannot be
decrypted by the receiver. In this case, the money would still leave the
DAO treasury but be unspendable.

* Wallet builder: `src/contract/dao/src/client/auth_xfer.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/auth_xfer.rs`
* ZK proofs:
  * `src/contract/dao/proof/auth-money-transfer.zk`
  * `src/contract/dao/proof/auth-money-transfer-enc-coin.zk`

### Function Params

Define the DAO AuthMoneyTransfer function params
$$ \begin{aligned}
  𝒞_\t{enc} &∈ \t{ElGamalEncNote}₅^* \\
  𝒟_\t{enc} &∈ \t{ElGamalEncNote}₃
\end{aligned} $$

This provides verifiable note encryption for all output coins in the sibling `Money::transfer()` call as well as the DAO change coin.

```rust
pub struct DaoAuthXferParams {
    pub encrypted_output_coins: Vec<ElGamalEncryptedNote>,
    pub encrypted_change_coin: ElGamalEncryptedNote,
}
```

### Contract Statement

Denote the DAO contract ID by $\t{CID}_\t{DAO} ∈ 𝔽ₚ$.

**Sibling call is `Money::transfer()`** &emsp; load the sibling call and check
the contract ID and function code match `Money::transfer()`.

**Money originates from the same DAO** &emsp; check all the input's `user_data`
for the sibling `Money::transfer()` encode the same DAO. We do this by using the
same blind for all `user_data`. Denote this value by $\t{UD}_\t{enc}$.

**Output coins match proposal** &emsp; check there are $n + 1$ output coins,
with the first $n$ coins exactly matching those set in the auth data in
the parent `DAO::exec()` call. Denote these proposal auth calls by $𝒜 $.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  b_\t{UD} &∈ 𝔽ₚ \\
  v_\t{DAO} &∈ 𝔽ₚ \\
  τ_\t{DAO} &∈ 𝔽ₚ \\
  b_\t{DAO} &∈ 𝔽ₚ \\
  \t{esk} &∈ 𝔽ₚ \\
\end{aligned} $$

Attach a proof $π_\t{auth}$ such that the
following relations hold:

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $𝒫 $ matches the value in `DAO::exec()`, and $p.𝒜  = 𝒜 $.

**Input user data commits to DAO bulla** &emsp; $\t{UD}_\t{enc} =
\t{PoseidonHash}(𝒟 , b_\t{UD})$

**DAO change coin integrity** &emsp; denote the last coin in the
`Money::transfer()` outputs by $C_\t{DAO}$. Then check
$$ C_\t{DAO} = \t{Coin}(d.\t{PK}, v_\t{DAO}, τ_\t{DAO},
                        \t{CID}_\t{DAO}, 𝒟 , b_\t{DAO}) $$

**Verifiable DAO change coin note encryption** &emsp;
let $𝐧 = (v_\t{DAO}, τ_\t{DAO}, b_\t{DAO})$, and verify
$𝒟_\t{enc} = \t{ElGamal}.\t{Encrypt}(𝐧, \t{esk}, d.\t{PK})$.

Then we do the same for each output coin of `Money::transfer()`.
For $k ∈ [n]$, let $a = (𝒞_\t{enc})ₖ$ and $C$ be the $k$th output coin from
`Money::transfer()`.
Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  c &∈ \t{Attrs}_\t{Coin} \\
  e &∈ 𝔽ₚ
\end{aligned} $$
Attach a proof $πₖ$ such that the following relations hold:

&emsp; **Coin integrity** &emsp; $C = \t{Coin}(c)$

&emsp; **Verifiable output coin note encryption** &emsp;
let $𝐧 = (c.v, c.τ, c.\t{SH}, c.\t{UD}, c.n)$, and verify
$a = \t{ElGamal}.\t{Encrypt}(𝐧, \t{esk}, d.\t{PK})$.

### Signatures

No signatures are attached.

