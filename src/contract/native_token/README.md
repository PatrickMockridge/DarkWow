# Native Token — Consensus-Critical Commitment (L1)

## The Capability

NativeToken is the single consensus-critical asset: block rewards (PoWRewardV1),
fee payment (FeeV3) and fee collection (FeeCollectV1), and the cumulative supply
audit. A commitment is a
**consume+create** L1 capability — spending nullifies an input and mints blind
outputs. It is intentionally **rock-dumb**: no multi-token, no authorization
hooks, no freezing — minimal consensus attack surface.

**Trust tier:** consensus-critical (genesis counter 4). The only genesis contract
that can halt the chain. The only contract with a bespoke wallet path
(`wallet.md` §6.4 — the "one bespoke citizen").

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | — | — | Unassigned — returns `InvalidFunction`; all fee payment uses FeeV3 `0x08` (fee-spec §10) |
| `0x01` | `mint` | — | **DISABLED** — returns `InvalidFunction`; the only new-supply path is `pow_reward` (`uncle_mint` 0x07 mints notes carved out of the coinbase — no supply bump) |
| `0x02` | `burn` | `Burn_V2` | Burn commitments — publishes nullifiers (used by fee payment) |
| `0x03` | `transfer` | `Burn_V2` + `Mint_V2` | Atomic burn + blind output with value conservation |
| `0x04` | `spend` | `Burn_V2` + `Mint_V2` | Single in/out spend with value conservation |
| `0x05` | `pow_reward` | — (plaintext) | Block reward — plaintext; verifies cumulative supply + expected reward + Pedersen commit |
| `0x06` | `fee_collect` | — (plaintext) | Close the fee epoch: plaintext `total_fees == fees_db[height]` check, mint fee note, zero the pot |
| `0x07` | `uncle_mint` | — (plaintext) | Uncle note mint — one spendable note per accepted uncle, carved out of the coinbase; no supply write |
| `0x08` | `fee` | `Fee_V3` | Pay fees (plaintext fee + tier, `FeeParamsV3`) — burns input, creates change output; adds fee to the plaintext `fees_db[height]` total |

Only three circuits exist — `mint.zk` (Mint_V2), `burn.zk` (Burn_V2),
`fee.zk` (Fee_V3). Every other call is plaintext: no threshold proofs, no
encrypted-fee channel, no fee accumulator.

## Domain Constants

`NULLIFIER = witness_base(1)`, `TOKEN_COMMIT = witness_base(2)`,
`TX_BINDING = witness_base(3)`, `COMMITMENT = witness_base(4)`,
`USER_DATA_ENC = witness_base(6)`, `SIGNATURE_SECRET = witness_base(7)`.

## Data Model

```
pk             = ec_mul_base(spend_secret, NULLIFIER_K)                        # EC-point public key
C (commitment) = poseidon_hash(4, pk_x, pk_y, value, asset_id, spend_hook, user_data, blind)
nullifier      = poseidon_hash(1, spend_secret, C)
token_commit   = poseidon_hash(2, asset_id, token_blind)
value_commit   = pedersen_commit(value, value_blind)                          # ec_mul_short(V) + ec_mul(R)
tx_binding     = poseidon_hash(3, tx_commitment, tx_nonce)
```

Unlike PN, native_token uses an **EC-point public key** (`ec_mul_base(secret, NULLIFIER_K)`);
the commitment hash takes both coordinates (`pk_x, pk_y`). `DRKW_ASSET_ID = AssetId::DRKW`
(zero); the canonical DRKW `token_commit` is `poseidon_hash([DOMAIN_TOKEN_COMMIT, 0, 0])`.

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
| `↓spend` | `pk = ec_mul_base(spend_secret, NULLIFIER_K)` bound to `public_x/y` |
| `↓nullify` | `nf = poseidon_hash(1, spend_secret, C)` |
| `↓prove-inclusion` | `merkle_root(leaf_pos, path, commitment) == expected_root` (zero-value guard) |
| `↓denominate` | `token_commit = poseidon_hash(2, asset_id, token_blind)` |
| `↓conserve` | per `token_commit`, `Σ input value_commit == Σ output value_commit` |
| `↓commit` | Apply `merkle_add` outputs, `db_mark_spent` nullifiers |

## The Four-Component Flow

1. **Circuit** — computes commitment/nullifier/commitments; constrains to witnesses.
2. **Params** — caller pre-computes public inputs with matching domain constants.
3. **Metadata** — pure echo; for non-coinbase mints `S_H == value_commit` (identity base).
4. **Exec** — validates nullifier/root/`token_commit`/conservation; **Apply** — writes.

`pow_reward_v1` is consensus: exact `expected_reward(height)` equality, cumulative
chain check, and supply audit. `fee_v3` reads the running plaintext total
`fees_db[height]` in Exec and writes the new total in Apply.

## State Trees

| Tree | Purpose |
|------|---------|
| `commitment_set` | Commitment Merkle tree |
| `nullifiers` | Nullifier SMT (double-spend prevention) |
| `info` | Contract metadata and state (incl. `commitment_merkle_tree` checkpoints) |
| `commitment_roots` | Historical commitment-tree roots |
| `nullifier_roots` | Historical nullifier-tree roots |
| `fees` | Per-height plaintext fee total (u64), zeroed at fee-collect |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `commitment` | `0` | `SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId, MerkleNode` | `{ value: u64, commitment: pallas_base }` |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `transfer` | `any(commitment)` | `commitment` | `commitment` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `fee` | `any(commitment)` | `commitment` | `commitment` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `spend` | `any(commitment)` | `commitment` | `commitment` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |

## Authorization

A `commitment` is spent by proving `spend_secret` in the nullifier. There is no
per-account authorization — the commitment IS the capability. Block rewards are minted
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
