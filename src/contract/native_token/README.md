# Native Token — Consensus-Critical Coin (L1)

## The Capability

NativeToken is the single consensus-critical asset: block rewards (PoWRewardV1),
fee payment (FeeV2 / FeeCollectV2), and the cumulative supply audit. A coin is a
**consume+create** L1 capability — spending nullifies an input and mints blind
outputs. It is intentionally **rock-dumb**: no multi-token, no authorization
hooks, no freezing — minimal consensus attack surface.

**Trust tier:** consensus-critical (genesis counter 4). The only genesis contract
that can halt the chain. The only contract with a bespoke wallet path
(`wallet.md` §6.4 — the "one bespoke citizen").

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `fee_v1` | — | **REMOVED** — returns `InvalidFunction`; all fees use FeeV2 `0x08` (fee-spec §10) |
| `0x01` | `mint` | — | **DISABLED** — returns `InvalidFunction`; the only mint path is `pow_reward` |
| `0x02` | `burn` | `BurnV2` | Burn coins — publishes nullifiers (used by fee payment) |
| `0x03` | `transfer` | `BurnV2` + `MintV2` | Atomic burn + blind output with value conservation |
| `0x04` | `spend` | `BurnV2` + `MintV2` | Single in/out spend with value conservation |
| `0x05` | `pow_reward` | `MintV2` | Block reward — verifies cumulative supply + expected reward + Pedersen commit |
| `0x06` | `fee_collect` | `FeeCollectV2` | Close the fee epoch, distribute accumulated fees |
| `0x08` | `fee` | `FeeV2` | Pay fees — burns fee, creates change output; writes fee accumulator |

`fee_threshold_v1` is a fifth circuit, **stored but not in the manifest** — it is
mempool-only (`fee >= threshold` proof at admission, not verified at `accept_block`).

## Domain Constants

`NULLIFIER = witness_base(1)`, `TOKEN_COMMIT = witness_base(2)`,
`TX_BINDING = witness_base(3)`, `COIN_COMMIT = witness_base(4)`,
`USER_DATA_ENC = witness_base(6)`, `SIGNATURE_SECRET = witness_base(7)`.

## Data Model

```
pk             = ec_mul_base(coin_secret, NULLIFIER_K)                        # EC-point public key
C (coin)       = poseidon_hash(4, pk_x, pk_y, value, asset_id, spend_hook, user_data, blind)
nullifier      = poseidon_hash(1, coin_secret, C)
token_commit   = poseidon_hash(2, asset_id, token_blind)
value_commit   = pedersen_commit(value, value_blind)                          # ec_mul_short(V) + ec_mul(R)
tx_binding     = poseidon_hash(3, tx_commitment, tx_nonce)
```

Unlike PN, native_token uses an **EC-point public key** (`ec_mul_base(secret, NULLIFIER_K)`);
the coin hash takes both coordinates (`pk_x, pk_y`). `DRKW_ASSET_ID = AssetId::DRKW`
(zero); the canonical DRKW `token_commit` is `poseidon_hash([0, 0])`.

### Cumulative Supply (consensus)

`pow_reward_v1` enforces the Pedersen supply chain `S_H = S_{H-1} + C_H`:

```
TOTAL_SUPPLY              — new_supply == expected_cumulative_supply(height)   (missing → 0 at genesis)
CUMULATIVE_VALUE_COMMIT   — old + value_commit == new_cumulative_commit        (missing → identity)
CUMULATIVE_BLIND          — old + value_blind == new_blind                     (missing → zero; skipped at genesis)
```

## Barbs

| Barb | Mechanism |
|------|-----------|
| `↓spend` | `pk = ec_mul_base(coin_secret, NULLIFIER_K)` bound to `coin_public_x/y` |
| `↓nullify` | `nf = poseidon_hash(1, coin_secret, C)` |
| `↓prove-inclusion` | `merkle_root(leaf_pos, path, coin) == expected_root` (zero-value guard) |
| `↓denominate` | `token_commit = poseidon_hash(2, asset_id, token_blind)` |
| `↓conserve` | per `token_commit`, `Σ input value_commit == Σ output value_commit` |
| `↓commit` | Apply `merkle_add` outputs, `db_mark_spent` nullifiers |

## The Four-Component Flow

1. **Circuit** — computes coin/nullifier/commitments; constrains to witnesses.
2. **Params** — caller pre-computes public inputs with matching domain constants.
3. **Metadata** — pure echo; for non-coinbase mints `S_H == value_commit` (identity base).
4. **Exec** — validates nullifier/root/`token_commit`/conservation; **Apply** — writes.

`pow_reward_v1` is consensus: exact `expected_reward(height)` equality, cumulative
chain check, and supply audit. `fee_v2` reads the accumulator in Exec and blind-writes
the new accumulator in Apply (`write_accumulator`).

## State Trees

| Tree | Purpose |
|------|---------|
| `coins` | Coin commitment Merkle tree |
| `nullifiers` | Nullifier SMT (double-spend prevention) |
| `merkle` | Merkle tree checkpoints |
| `info` | Contract metadata and state |
| `coin_roots` | Historical coin-tree roots |
| `nullifier_roots` | Historical nullifier-tree roots |
| `fees` | Fee collection accumulator |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `coin` | `0` | `SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId, MerkleNode` | `{ value: u64, commitment: pallas_base }` |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `transfer` | `any(coin)` | `coin` | `coin` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `fee` | `any(coin)` | `coin` | `coin` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `spend` | `any(coin)` | `coin` | `coin` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |

## Authorization

A `coin` is spent by proving `coin_secret` in the nullifier. There is no
per-account authorization — the coin IS the capability. Block rewards are minted
only by `pow_reward_v1` (consensus-governed); the `mint` function is disabled so
no non-consensus path can inflate supply. The wallet handles native_token via a
hardcoded `TransferCallBuilder` (bespoke path) rather than the generic manifest prover.

## References

- [Native Token Specification](../../../doc/src/contract/native_token.md)
- [Consensus & Coinbase](../../../doc/src/arch/consensus-coinbase.md)
- [Contract Manifest](../../../doc/src/arch/manifest.md)
- [Wallet Architecture](../../../doc/src/arch/wallet.md) — the bespoke citizen
- [Privacy Model](../../../doc/src/arch/privacy.md)
- Source: `src/contract/native_token/`
