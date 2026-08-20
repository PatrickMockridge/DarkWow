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

In a transparent blockchain, moving coins is simple: subtract from sender, add
to recipient. Everyone can see both sides of the transaction, so they can verify
the arithmetic.

In a privacy blockchain, you can't do that. If you reveal which coins were
spent and which were created, you reveal who paid whom. The sender and recipient
are linked by the transaction itself.

The solution is to destroy the old coins and create entirely new ones, with no
visible connection between the two events. This is the burn-mint pattern:

- **Burn**: The sender's coins are permanently destroyed. Each produces a
  *nullifier* — a unique fingerprint that proves the coin existed and prevents
  it from being spent twice. The nullifier reveals nothing about which coin was
  burned. Only the spender, who knows the coin's secret, can compute it.

- **Mint**: New coins are created for the recipient. Each is a cryptographic
  commitment — a hash of the recipient's public key, the value, and a random
  blinding factor. The commitment hides everything. Only the recipient, who
  knows the blinding factor, can open it.

The ZK proof ties these together. It proves: "I destroyed valid coins of total
value V, and I created new coins of total value V, and I know the secrets for
the destroyed coins." The verifier learns that value was conserved. They learn
nothing about who sent what to whom.

This pattern — burn old coins, prove validity in ZK, mint new coins — is the
architectural foundation of NativeToken. Every operation follows it. The only
exception is the coinbase reward, which mints new supply without burning
anything.

## How Value Moves

### Transfer

Alice wants to send 50 DRKW to Bob.

Alice's wallet selects coins she owns whose total value is at least 50. It
burns them — producing nullifiers that go on-chain and prevent those coins
from ever being used again. It mints two new coins: one worth 50 DRKW for Bob,
and one worth the change for Alice.

```
burn:  [Coin_A (30), Coin_B (40)]  →  nullifiers [N_a, N_b]
mint:  [Coin_C (50, to Bob), Coin_D (20, to Alice)]
```

Two things must be proven. First, that Alice owned coins A and B — the ZK
proof verifies each coin exists in the Merkle tree and that Alice knows its
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
       fee = 2            // collected by miner in coinbase
```

The ZK circuit enforces `change + fee == input`. Charlie gets his change back
to the same public key. The miner collects the 2 DRKW fee as part of their
coinbase reward. The fee is not burned — it transfers to the miner.

### Coin Destruction

Coins can be permanently destroyed, reducing actual supply below the cumulative
ceiling.

```
burn:  [Coin_G (100)]  →  nullifier [N_g]
mint:  (nothing)
```

Each burn uses a per-burn unique signature:

```
signature_secret = poseidon_hash(coin_secret, nullifier)
```

This binds the transaction signer to the coin owner — only the coin's owner
can burn it — while keeping each burn unlinkable. Even if the same owner burns
multiple coins, each burn uses a different signature secret because each
nullifier is unique.

## How Supply Is Created

New DRKW enters circulation exclusively through coinbase rewards. When a miner
finds a block, they earn:

- The block reward per the emission schedule
- All fees from transactions in the block

The coinbase output goes through the same `mint_v1.zk` circuit used for transfer
outputs, with one critical addition: the circuit extends a Pedersen cumulative
commitment chain.

```
S_0 = pedersen_commit(0, 0)              // genesis: zero supply
S_H = S_{H-1} + C_H                      // each coinbase extends the chain
```

Where `C_H = pedersen_commit(expected_reward(H), blind_H)` is the coinbase's
value commitment, and the blind is deterministically derived from chain state:

```
blind_H = blake3("native_token_coinbase_blind" || prev_coin || height)
```

The circuit enforces `ec_add(S_{H-1}, C_H) == S_H` and exposes the new
cumulative commitment as a public input. This is not a separate mechanism
bolted onto the side — it is part of how minting works. Every coinbase proof
carries the cumulative chain forward by exactly the expected reward.

The emission schedule begins with Bitcoin's 21M DRKW supply target, after which perpetual 1% tail emission takes over — there is no supply cap.
continuous exponential decay with a 4-year half-life, and a permanent tail
emission for long-term security.

## The Supply Audit

Because the cumulative chain is built from Pedersen commitments, it has a
property the ZK circuit alone does not: any node can verify it independently,
without trusting a single ZK proof.

Pedersen commitments are binding — once published, the value inside cannot be
changed without breaking the discrete log assumption between the commitment's
generators. Walk the canonical chain from genesis, recompute every blind and
commitment from the emission schedule, and compare against the stored `S_H`:

```
verify_cumulative_supply(chain, cumulative_commits)
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

NativeToken has **two witnesses**: the ZK circuit and the Pedersen chain. A ZK
soundness bug alone cannot hide inflation from the audit — the forged `S_H`
won't match `pedersen_commit(expected_supply, expected_blind)`. A Pedersen
binding break alone cannot fool the circuit — `ec_add` still rejects the
invalid chain extension. Both must fail simultaneously to hide inflation from
all observers. Either failure alone raises the alarm.

### Active Enforcement: Proof of Token Balance

The supply audit is no longer passive. As of June 2026, it is an **active
consensus rule** enforced at every block acceptance path in `dwowd`. The check
— called **proof of token balance** — is performed before any block is applied
to the chain, across all six block acceptance paths (P2P broadcast, built-in
miner, RPC miner, stratum, merge mining, and consensus sync).

The proof of token balance extends the cumulative chain audit with a per-block
**Pedersen mass balance equation**:

```
Σ output_commits + Σ burn_commitments + Σ fee_commits == Σ input_commits
```

This verifies that non-coinbase transactions are collectively net-neutral (or
net-negative) for darkw token supply. Every input and output value commitment
across every native token call in the block — `FeeV1`, `BurnV1`, `TransferV1`,
`SpendV1`, and `MintV1` — is summed as a Pedersen point. The coinbase is
excluded from these sums and verified separately against the emission schedule.
Together, the mass balance and cumulative chain prove that the only new darkw
entering circulation is the coinbase reward.

A block that fails the mass balance check is **rejected** — it will not be
applied to the chain, will not be broadcast to peers, and will not be mined
upon. This is not advisory. It is consensus.

The implementation is in `bin/dwowd/src/proof_of_token_balance.rs`. The Python
model is at `contrib/model/proof_of_token_balance.py`.

### Why the Audit Does Not Break Privacy — Proof

**Claim.** The cumulative supply audit reveals exactly one bit of information
per block height: whether the stored `S_H` matches `pedersen_commit(expected_cumulative_supply(H), total_blind(H))`.
It reveals zero information about individual transaction amounts, participants,
or the link between burned and minted coins.

**Proof.** We examine the two cases.

**Case 1 — Coinbase block (supply is created).** At a block height H where a
coinbase reward is issued, the circuit extends the cumulative chain:

```
S_H = S_{H-1} + C_H
     = S_{H-1} + pedersen_commit(expected_reward(H), coinbase_blind(prev_coin, H))
```

The value `expected_reward(H)` is a public constant from the emission schedule.
The blind `coinbase_blind(prev_coin, H)` is a deterministic function of two
public inputs: the previous coinbase commitment `prev_coin` (on-chain) and the
block height `H`. Both are known to every node.

Therefore `C_H` is a Pedersen commitment to two publicly computable scalars.
It contains zero private information. It is a deterministic function of public
chain state. The same holds for `S_H`, which is a sum of such commitments.

An auditor who recomputes `S_H` and compares it to the stored value learns
exactly one bit: match or mismatch. If match, the coinbase correctly extended
the chain. If mismatch, an anomaly occurred. The auditor already knew
`expected_reward(H)` from the emission schedule. The audit adds no new
information beyond what the schedule already declares.

**Case 2 — Transfer block (supply is unchanged).** Transfers conserve value:
the sum of burned coin values equals the sum of minted coin values. The
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
| Supply tracking | Pedersen cumulative chain | Per-token coin count |
| EC operations | Yes (Pedersen) | No (Poseidon-only) |

## Reference

### Functions

| Function | Opcode | Purpose |
|----------|--------|---------|
| FeeV1 | 0x00 | Pay miner to process transaction |
| MintV1 | 0x01 | **Disabled** — opcode reserved |
| BurnV1 | 0x02 | Destroy coins |
| TransferV1 | 0x03 | Private transfer (burn inputs, mint outputs) |
| SpendV1 | 0x04 | Spend single coin with change |
| PoWRewardV1 | 0x05 | Block reward — mints new supply, extends cumulative chain |

### Coin

```
coin        = poseidon_hash(pub_x, pub_y, value, asset_id, spend_hook, user_data, blind)
nullifier   = poseidon_hash(coin_secret, coin)
value_commit = pedersen_commit(value, value_blind)
            = value * G_v + value_blind * G_r
```

### ZK Circuits

| Circuit | Public Inputs | Constraints |
|---------|---------------|-------------|
| mint_v1.zk | 6 | Coin validity, cumulative chain `ec_add`, 64-bit range checks |
| burn_v1.zk | 9 | Merkle proof, per-burn signature `poseidon_hash(secret, nullifier)` |
| fee_v1.zk | 12 | Value conservation `change + fee == input` |

### Database Trees

```
COINS_TREE           - coin → ()
NULLIFIERS_TREE      - nullifier → spent
MERKLE_TREE          - incremental Merkle tree
COIN_ROOTS_TREE      - historical Merkle roots
NULLIFIER_ROOTS_TREE - historical nullifier roots
FEES_TREE            - fee accumulator per block
INFO_TREE            - metadata (total supply, cumulative supply)
```

### Client API

```rust
pub struct PoWRewardCallBuilder {
    pub secret: SecretKey,
    pub block_height: u32,
    pub fees: u64,
    pub recipient: Option<PublicKey>,
    pub expected_cumulative_supply: u64,
    pub old_cumulative_commit: pallas::Point,
    pub old_cumulative_blind: pallas::Scalar,
    pub mint_zkbin: ZkBinary,
    pub mint_pk: ProvingKey,
}
```

### Testing

```bash
cargo run -p dwow-contract-test-harness --bin test_native_token
```

Proof of token balance unit tests:

```bash
cargo test -p dwowd --lib -- proof_of_token_balance
```

Three tests verify the mass balance enforcement:
- `test_empty_block_fails_missing_coinbase` — rejects blocks without a coinbase
- `test_block_with_only_coinbase_passes` — accepts coinbase-only blocks
- `test_block_with_coinbase_and_empty_txs_passes` — accepts blocks with non-native transactions

All 3 pass.

- [x] MintV1 test passes (circuit decode validation)
- [x] PoWRewardCallBuilder generates real ZK proofs
- [x] BurnV1 client API — real ZK proof generation

### Files

```
src/contract/native_token/
├── src/lib.rs              # Function enum, DRKW_ASSET_ID
├── src/model/mod.rs         # Coin, Input, Output, etc.
├── src/entrypoint/mod.rs    # WASM entrypoint
├── src/client/              # burn_v1, pow_reward_v1, fee_v1, transfer_v1
└── proof/
    ├── mint_v1.zk           # 6 public inputs, cumulative chain
    ├── burn_v1.zk           # 9 public inputs, per-burn signature
    └── fee_v1.zk            # 12 public inputs, value conservation
```

## See Also

- [PromissoryNote](./promissory_note.md) — DeFi token contract
- [Supply Audit](../../arch/consensus/consensus.md#supply-audit-capability) — Design rationale
- [Smart Contract Safety](./safety.md) — Lesson 20: Supply Audit Capability
- [Block Explorer Guide](../../testnet/block-explorer.md) — Supply audit via RPC
