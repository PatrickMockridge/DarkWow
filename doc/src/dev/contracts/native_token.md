# NativeToken

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

The emission schedule follows Bitcoin's model: 21 million DRKW hard cap,
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

This is a capability the token provides — a verifiable property that any holder
of the blockchain can exercise. Like Bitcoin's halving schedule, the audit
doesn't halt block production. It informs consensus. Nodes that detect a
discrepancy can choose to mine on a fork without it. The WASM execution path
(`execute_block` in `bin/dwowd/src/execution.rs`) is a possible future upgrade
that would make supply validation an active consensus rule, rejecting blocks
with invalid cumulative commitments at execution time. Currently it is
intentionally passive.

### Why Privacy Survives

The audit reveals exactly one fact: total coinbase issuance matches the emission
schedule. It reveals nothing about individual transactions. Here is why.

**At coinbase heights — the chain is extended.**

The coinbase reward `expected_reward(H)` is a public constant determined by the
emission schedule. The blind `coinbase_blind(prev_coin, H)` is deterministically
derived from two public inputs: the previous coinbase commitment (on-chain) and
the block height. So:

```
C_H = pedersen_commit(expected_reward(H), coinbase_blind(prev_coin, H))
```

Both arguments to the Pedersen commitment are publicly computable. `C_H` contains
zero private information. It is a deterministic function of public chain state.

The cumulative commitment `S_H = S_{H-1} + C_H` is therefore a sum of publicly
computable values. At any height H:

```
S_H = pedersen_commit(expected_cumulative_supply(H), total_blind(H))
```

where `expected_cumulative_supply(H)` is the sum of all rewards through height H
(a public constant) and `total_blind(H)` is the sum of all deterministic blinds
(recomputable from public chain data). The auditor recomputes both and compares.
A match confirms supply integrity. A mismatch is cryptographic proof of anomaly.

**At transfer heights — the chain is not extended.**

Transfers use the same `mint_v1.zk` circuit for output creation, but the
cumulative chain must not change — transfers conserve value, they don't create
it. The prover sets `old_cumulative = identity` (the Pedersen commitment to
zero). The circuit computes:

```
new_cumulative = identity + coin_value_commit
               = pedersen_commit(0, 0) + pedersen_commit(output_value, output_blind)
               = coin_value_commit
```

The "new cumulative" coordinates are exactly the output's value commitment
coordinates. But those coordinates are *already* public inputs — `constrain_instance`
at lines 55-56 of the circuit exposes `vc_x` and `vc_y`. The cumulative public
inputs are redundant. They duplicate information the verifier already has.

**What the auditor learns.**

At each block, one of two things happens:

- Coinbase block: `S_H = S_{H-1} + C_H` where `C_H` commits to a public value
  with a deterministic blind. The auditor learns that the coinbase rewarded the
  correct amount. They already knew the amount from the emission schedule.

- Transfer block: `S_H = S_{H-1}`. The cumulative doesn't move. The auditor
  learns nothing about the transfer — not the amount, not the participants,
  not even that a transfer occurred.

**What the auditor never learns.**

- Individual transfer amounts. Value commitments hide these. The audit sums
  coinbase values only, which are public.
- Which public key owns which coin. Coin commitments are hashes that hide the
  owner's public key behind a blinding factor.
- The link between burned and minted coins. Burn-mint unlinkability is
  preserved — the cumulative chain doesn't track coin ownership.
- Total actual supply. Burns reduce supply below the cumulative ceiling. The
  chain proves only an *upper bound*.

The Pedersen binding property ensures the commitment cannot be opened to a
different value. But the commitment itself reveals nothing beyond what the
emission schedule and block headers already make public.

The burden of proof is on miners earning coinbase rewards. Every coinbase must
carry a ZK proof that correctly extends the supply chain. Users transacting
privately carry no such burden.

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
coin        = poseidon_hash(pub_x, pub_y, value, token_id, spend_hook, user_data, blind)
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

- [x] MintV1 test passes (circuit decode validation)
- [x] PoWRewardCallBuilder generates real ZK proofs
- [x] BurnV1 client API — real ZK proof generation

### Files

```
src/contract/native_token/
├── src/lib.rs              # Function enum, DRKW_TOKEN_ID
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
