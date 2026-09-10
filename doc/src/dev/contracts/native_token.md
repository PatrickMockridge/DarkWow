# NativeToken

> **Developer integration guide.** For the contract specification, see [NativeToken Contract](../../contract/native_token.md).

## Why NativeToken Exists

A blockchain needs a native token. Miners need to be paid for securing the
network. Users need to pay fees to have their transactions included. Value
needs to move between participants.

But DarkWow is a privacy blockchain. If every payment reveals who paid whom
and how much, there is no privacy. And if the total supply can't be verified
independently, there is no trust — as the Orchard exploit proved in May 2026,
when a single missing circuit constraint allowed potentially unbounded hidden
inflation with no way to audit whether it happened.

NativeToken exists to do three things simultaneously:

1. **Run consensus** — pay miners, collect fees
2. **Preserve privacy** — hide amounts, break the link between sender and recipient
3. **Prove supply integrity** — give every node the ability to independently
   verify total circulation against the emission schedule

The first is what every blockchain token does. The second is what privacy
tokens do. The third is what no token did before Orchard broke — and it's the
reason NativeToken carries a Pedersen cumulative commitment chain from genesis
to tip.

## The Burn-Mint Pattern

In a transparent blockchain, moving commitments is simple: subtract from sender, add
to recipient. Everyone can see both sides of the transaction, so they can verify
the arithmetic.

In a privacy blockchain, you can't do that. If you reveal which commitments were
spent and which were created, you reveal who paid whom. The sender and recipient
are linked by the transaction itself.

The solution is to destroy the old commitments and create entirely new ones, with no
visible connection between the two events. This is the burn-mint pattern:

- **Burn**: The sender's commitments are permanently destroyed. Each produces a
  *nullifier* — a unique fingerprint that proves the commitment existed and prevents
  it from being spent twice. The nullifier reveals nothing about which commitment was
  burned. Only the spender, who knows the spend secret, can compute it.

- **Mint**: New commitments are created for the recipient. Each is a cryptographic
  commitment — a hash of the recipient's public key, the value, and a random
  blinding factor. The commitment hides everything. Only the recipient, who
  knows the blinding factor, can open it.

The ZK proof ties these together. It proves: "I destroyed valid commitments of total
value V, and I created new commitments of total value V, and I know the secrets for
the destroyed commitments." The verifier learns that value was conserved. They learn
nothing about who sent what to whom.

This pattern — burn old commitments, prove validity in ZK, mint new commitments — is the
architectural foundation of NativeToken. Every operation follows it. The
exceptions are the three mint-without-burn paths: the coinbase reward
(PoWRewardV1, which mints new supply), FeeCollectV1 (which mints a fee
commitment from the block's plaintext fee pot — redistribution, not new
supply), and UncleMintV1 (which mints spendable uncle notes carved out of the
coinbase via the `total_pin` mechanism — also not new supply).

## How Value Moves

### Transfer

Alice wants to send 50 DRKW to Bob.

Alice's wallet selects commitments she owns whose total value is at least 50. It
burns them — producing nullifiers that go on-chain and prevent those commitments
from ever being used again. It mints two new commitments: one worth 50 DRKW for Bob,
and one worth the change for Alice.

```
burn:  [Coin_A (30), Coin_B (40)]  →  nullifiers [N_a, N_b]
mint:  [Coin_C (50, to Bob), Coin_D (20, to Alice)]
```

Two things must be proven. First, that Alice owned commitments A and B — the ZK
proof verifies each commitment exists in the Merkle tree and that Alice knows its
secret. Second, that value was conserved — the contract entrypoint checks:

```
pedersen_commit(30) + pedersen_commit(40) == pedersen_commit(50) + pedersen_commit(20)
```

Pedersen commitments are additively homomorphic — they can be added without
being opened. The equality proves that 30 + 40 = 50 + 20 without revealing
any of those numbers. The contract knows value was conserved. It doesn't know
the amounts, who owned the inputs, or who owns the outputs.

No new supply is created. The cumulative supply chain is not extended.

### Fee Payment

Charlie wants to send a transaction. He pays a fee to the miner who includes it.

```
burn:  [Coin_E (100)]  →  nullifier [N_e]
mint:  [Coin_F (98, to Charlie)]
       fee = 2            // accumulated in the block's plaintext fee pot (fees_db[height])
```

The ZK circuit enforces `change + fee == input`. Charlie gets his change back
to the same public key. The miner claims the 2 DRKW fee at block close via
FeeCollectV1 (0x06), which mints one fee commitment of exactly the pot total
and zeroes the pot. The fee is not burned — it transfers to the miner. Fees
never enter the coinbase or the cumulative supply chain.

### Commitment Destruction

Commitments can be permanently destroyed, reducing actual supply below the cumulative
ceiling.

```
burn:  [Coin_G (100)]  →  nullifier [N_g]
mint:  (nothing)
```

Each burn uses a per-burn unique signature:

```
signature_secret = poseidon_hash(spend_secret, nullifier)
```

This binds the transaction signer to the commitment owner — only the commitment's owner
can burn it — while keeping each burn unlinkable. Even if the same owner burns
multiple commitments, each burn uses a different signature secret because each
nullifier is unique.

## How Supply Is Created

New DRKW enters circulation exclusively through coinbase rewards. When a miner
finds a block, they earn:

- The block reward per the emission schedule (coinbase = reward ONLY)
- All fees from transactions in the block — claimed separately via
  FeeCollectV1 (0x06), never through the coinbase

The coinbase output is built with plaintext Pedersen/poseidon arithmetic (no
ZK circuit since b6bf44f79), with one critical addition: it extends a Pedersen
cumulative commitment chain.

```
S_0 = pedersen_commit(0, 0)              // genesis: zero supply
S_H = S_{H-1} + C_H                      // each coinbase extends the chain
```

Where `C_H = pedersen_commit(expected_reward(H), blind_H)` is the coinbase's
value commitment, and the blind is deterministically derived from chain state:

```
value_blind_H = poseidon_hash([sk_H, height, 1])   // DOMAIN_VALUE_BLIND
```

(`sk_H` is the deterministic per-block coinbase key; commitment and token
blinds use domains 2 and 3 — `client/pow_reward.rs`. The legacy blake3
`coinbase_blind`/`verify_cumulative_supply` SDK audit helpers were deleted
(2026-09): the blind is private-key-derived and not publicly recomputable,
so the RPC supply-audit endpoint returns the stored chain state.)

The entrypoint enforces `ec_add(S_{H-1}, C_H) == S_H` and carries the new
cumulative commitment in the plaintext `PoWRewardParamsV1.new_cumulative_commit`
field. This is not a separate mechanism
bolted onto the side — it is part of how minting works. Every coinbase call
carries the cumulative chain forward by exactly the expected reward.

DarkWow uses continuous exponential decay with a ~4-year half-life
(`HALF_LIFE_BLOCKS = 1_051_920`): `R(h) = max(R₀ × 2^(-h/H), R_tail)` with
`R₀ = 1_383_764_049` base units (~13.838 DRKW) at genesis. The decay
asymptotically approaches the 21M DRKW reference supply; when the per-block
reward drops below the tail floor `R_tail = 79_853_981` (~0.7985 DRKW, ~16.5
years after launch), permanent 1% per-annum tail emission takes over. There
are no step-function halvings and no supply cap.

## The Supply Audit

Because the cumulative chain is built from Pedersen commitments, it has a
property the ZK circuit alone does not: any node can verify it independently,
without trusting a single ZK proof.

Pedersen commitments are binding — once published, the value inside cannot be
changed without breaking the discrete log assumption between the commitment's
generators. Read the stored cumulative state via the RPC endpoint and check
the identity the entrypoint and block acceptor enforce:

```
blockchain.get_cumulative_supply → (S_H, blind, total_supply)
check: S_H == S_{H-1} + pedersen_commit(expected_reward(H), value_blind_H)
```

A single mismatch at any height is cryptographic proof of an anomaly. Either
a coinbase minted the wrong amount, the cumulative chain was not correctly
extended, or the stored commitment was tampered with. The audit doesn't say
which — it only says the chain does not match the emission schedule. That is
enough. An honest chain matches exactly.

### Why This Matters

In May 2026, a missing circuit constraint was discovered in the Orchard shielded
pool. The circuit had an under-constrained elliptic-curve check — it verified
that a multiplication was performed but did not constrain the validity of the
inputs. False inputs could produce valid ZK proofs. The bug existed undetected
for four years and survived multiple rounds of cryptographic review.

Orchard had **one witness** to supply integrity: the ZK circuit. When that
witness broke, there was nothing else. The network still cannot cryptographically
prove the bug wasn't exploited.

NativeToken has **two witnesses**: the ZK circuits and the Pedersen chain. A ZK
soundness bug alone cannot hide inflation from the audit — the forged `S_H`
won't match `pedersen_commit(expected_supply, expected_blind)`. A Pedersen
binding break alone cannot fool the entrypoint's plaintext `ec_add` check,
which rejects the invalid chain extension. Both must fail simultaneously to
hide inflation from all observers. Either failure alone raises the alarm.

### Active Enforcement: Proof of Token Balance

The supply audit is no longer passive. As of June 2026, it is an **active
consensus rule** enforced at every block acceptance path in `dwowd`. The check
— called **proof of token balance** — is performed before any block is applied
to the chain, across all block acceptance paths: built-in miner, stratum,
merge-mining RPC, miner RPC, and P2P broadcast (plus consensus sync).

The proof of token balance extends the cumulative chain audit with a per-block
**Pedersen mass balance equation**:

```
Σ output_commits + Σ burn_commitments + Σ fee_commits == Σ input_commits
```

This verifies that non-coinbase transactions are collectively net-neutral (or
net-negative) for darkw token supply. Every input and output value commitment
across every native token call in the block — `BurnV1`, `TransferV1`,
`SpendV1`, and `FeeV2` — is summed as a Pedersen point (FeeV2 contributes its
input/output commits plus its `fee_value_commit`). `MintV1`, `PoWRewardV1`,
`FeeCollectV1`, and `UncleMintV1` are skipped — coinbase, fee-pot
redistribution, and uncle notes carved out of the coinbase are handled
separately. The coinbase is
excluded from these sums and verified separately against the emission schedule.
Together, the mass balance and cumulative chain prove that the only new darkw
entering circulation is the coinbase reward.

A block that fails the mass balance check is **rejected** — it will not be
applied to the chain, will not be broadcast to peers, and will not be mined
upon. This is not advisory. It is consensus.

The implementation is in `src/linear/src/proof_of_token_balance.rs` (crate
`dwow_chain`, invoked from `bin/dwowd/src/block_acceptor.rs`). The Python
model is at `contrib/model/proof_of_token_balance.py`.

### Why the Audit Does Not Break Privacy — Proof

**Claim.** The cumulative supply audit reveals exactly one bit of information
per block height: whether the stored `S_H` matches `pedersen_commit(expected_cumulative_supply(H), total_blind(H))`.
It reveals zero information about individual transaction amounts, participants,
or the link between burned and minted commitments.

**Proof.** We examine the two cases.

**Case 1 — Coinbase block (supply is created).** At a block height H where a
coinbase reward is issued, the entrypoint extends the cumulative chain in
plaintext:

```
S_H = S_{H-1} + C_H
     = S_{H-1} + pedersen_commit(expected_reward(H), value_blind_H)
```

The value `expected_reward(H)` is a public constant from the emission schedule.
The blind `value_blind_H` is poseidon-derived from the miner's private
per-block key (`client/pow_reward.rs`, domain 1), so it is not publicly
recomputable — auditors read the stored `S_H` and blind from the
`blockchain.get_cumulative_supply` RPC endpoint.

Therefore `C_H` is a Pedersen commitment to a public value with a stored
blind, and the `S_H = S_{H-1} + C_H` identity is checkable by every node.
It contains zero private information. It is a deterministic function of public
chain state. The same holds for `S_H`, which is a sum of such commitments.

An auditor who recomputes `S_H` and compares it to the stored value learns
exactly one bit: match or mismatch. If match, the coinbase correctly extended
the chain. If mismatch, an anomaly occurred. The auditor already knew
`expected_reward(H)` from the emission schedule. The audit adds no new
information beyond what the schedule already declares.

**Case 2 — Transfer block (supply is unchanged).** Transfers conserve value:
the sum of burned values equals the sum of minted values. The
cumulative chain must not change. The prover sets the old cumulative to the
Pedersen identity (the commitment to zero):

```
old_cumulative = pedersen_commit(0, 0) = identity point
```

The circuit computes:

```
new_cumulative = identity + coin_value_commit
               = pedersen_commit(0, 0) + pedersen_commit(output_value, output_blind)
               = coin_value_commit
```

The coordinates of `new_cumulative` equal the coordinates of `coin_value_commit`.
But `coin_value_commit`'s coordinates `(vc_x, vc_y)` are *already* public inputs
— the circuit exposes them via `constrain_instance` at lines 55-56. The
cumulative public inputs are redundant. They duplicate information the verifier
already possesses.

The stored on-chain cumulative `S_H` does not change during a transfer block.
An auditor observes `S_H = S_{H-1}`. They learn nothing about the transfer —
not the amount, not the participants, not even that a transfer occurred.

**Conclusion.** At every block height H, the cumulative commitment `S_H` equals:

```
S_H = pedersen_commit(expected_cumulative_supply(H), total_blind(H))
```

where `expected_cumulative_supply(H)` is a public constant (the sum of all
emission rewards through height H) and `total_blind(H)` is computable from
public chain data (the sum of all deterministic coinbase blinds). Both arguments
are known to every node independent of any private transaction data.

The auditor verifies this equality. The result is a single bit: the chain
matches the emission schedule, or it does not. No information about individual
transactions — amounts, owners, or the sender-recipient graph — is used in the
computation of `S_H`, and therefore none is revealed by verifying it.

The Pedersen binding property ensures the commitment cannot be opened to a
value different from the one originally committed. But the commitment itself
is a function of public data only. The audit is a property of the emission
schedule, not of any user's transaction history.

## Architecture: Two Contracts

DarkWow separates consensus token operations from DeFi token operations across
two contracts. A bug in DeFi logic cannot affect consensus — block rewards and
fees continue regardless.

| | NativeToken | PromissoryNote |
|---|---|---|
| Role | Consensus | DeFi |
| Token | DRKW (single) | Multiple (via TokenMint) |
| Supply tracking | Pedersen cumulative chain | Per-token commitment count |
| EC operations | Yes (Pedersen) | No (Poseidon-only) |

## Reference

### Functions

| Function | Opcode | Purpose |
|----------|--------|---------|
| FeeV1 | 0x00 | REMOVED — returns `InvalidFunction`; use FeeV2 (0x08) |
| MintV1 | 0x01 | **Disabled** — opcode reserved |
| BurnV1 | 0x02 | Destroy commitments |
| TransferV1 | 0x03 | Private transfer (burn inputs, mint outputs) |
| SpendV1 | 0x04 | Spend single commitment with change |
| PoWRewardV1 | 0x05 | Block reward — mints new supply, extends cumulative chain |
| FeeCollectV1 | 0x06 | Fee collection plate — mints the block's plaintext fee pot |
| UncleMintV1 | 0x07 | Uncle note mint — spendable uncle reward, no supply bump |
| FeeV2 | 0x08 | Pay fee — plaintext fee + tier (`FeeParamsV3`) |

### Commitment

```
commitment  = poseidon_hash(pub_x, pub_y, value, asset_id, spend_hook, user_data, blind)
nullifier   = poseidon_hash(spend_secret, commitment)
value_commit = pedersen_commit(value, value_blind)
            = value * G_v + value_blind * G_r
```

### ZK Circuits

| Circuit | Public Inputs | Constraints |
|---------|---------------|-------------|
| mint_v2.zk | 10 | Commitment validity, cumulative chain `ec_add`, 64-bit range checks |
| burn_v2.zk | 11 | Merkle proof, per-burn signature `poseidon_hash(secret, nullifier)` |
| fee_v2.zk | 15 | Value conservation `change + fee == input` (retained for host mass-balance verification — the fee itself is plaintext) |

FeeCollectV1, PoWRewardV1, and UncleMintV1 are plaintext — no circuit.

### Database Trees

```
COMMITMENT_SET_TREE           - commitment → ()
NULLIFIERS_TREE      - nullifier → spent
# commitment_merkle_tree lives inside INFO_TREE (key: b"commitment_merkle_tree")
COMMITMENT_ROOTS_TREE - historical Merkle roots (key: "commitment_roots")
NULLIFIER_ROOTS_TREE - historical nullifier roots
FEES_TREE            - plaintext fee pot per block (`fees_db[height]`)
INFO_TREE            - metadata (total supply, cumulative supply)
```

### Client API

The coinbase is built by `build_linear_coinbase()` in
`bin/dwowd/src/registry/model.rs` — plaintext `PoWRewardParamsV1` (no ZK
proving key or `ZkBinary` since b6bf44f79). The builder computes the expected
reward exactly (`expected_reward(height)`), the deterministic blinds, and the
cumulative supply scalars — the same pure-function derivation the wallet
replays during scan.

### Testing

```bash
cargo test -p dwow_native_token_contract
```

Proof of token balance unit tests:

```bash
cargo test -p dwow_chain --lib -- proof_of_token_balance
```

Three tests verify the mass balance enforcement:
- `test_empty_block_fails_missing_coinbase` — rejects blocks without a coinbase
- `test_block_with_only_coinbase_passes` — accepts coinbase-only blocks
- `test_block_with_coinbase_and_empty_txs_passes` — accepts blocks with non-native transactions

All 3 pass.

- [x] MintV1 disabled — returns `InvalidFunction`; no Mint_V1 circuit on disk
- [x] `build_linear_coinbase()` generates plaintext coinbase calls (no ZK proof)
- [x] BurnV1 client API — real ZK proof generation

### Files

```
src/contract/native_token/
├── src/lib.rs              # Function enum, DRKW_ASSET_ID
├── src/model/mod.rs         # Commitment, Input, Output, etc.
├── src/entrypoint/mod.rs    # WASM entrypoint
├── src/client/              # burn.rs, fee.rs, fee_collect.rs, pow_reward.rs,
│                           # uncle_mint.rs, transfer/, zkbins.rs
└── proof/
    ├── mint.zk              # Mint_V2 — 10 public inputs, cumulative chain
    ├── burn.zk              # Burn_V2 — 11 public inputs, per-burn signature
    └── fee.zk               # Fee_V2 — 15 public inputs, value conservation
                             # (retained for host mass-balance verification;
                             #  PoWRewardV1/FeeCollectV1/UncleMintV1 are plaintext)
```

## See Also

- [PromissoryNote](./promissory_note.md) — DeFi token contract
- [Supply Audit](../../arch/consensus/consensus.md#supply-audit-capability) — Design rationale
- [Smart Contract Safety](./safety.md) — Lesson 20: Supply Audit Capability
- [Block Explorer Guide](../../testnet/block-explorer.md) — Supply audit via RPC
