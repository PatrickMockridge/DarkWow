# Privacy Model

DarkWow's privacy model is a **practical balance**: the emission schedule (coinbase and uncle rewards) and
the per-block fee totals are public quantities, so those amounts are plaintext (see §2); every **subsequent**
transfer/spend amount is private.
Identities are hidden behind per-instance derived addresses, ZK proofs, nullifiers, and AEAD-encrypted notes.

## 1. The model in one paragraph

A user holds a root secret and derives a fresh, unlinkable key per (contract, instance) via
`derive_instance`. Each transaction mints/spends notes whose **values and recipient identities are hidden**
behind Pedersen/Poseidon commitments and AEAD ciphertext; a spend is proven by publishing a **nullifier**
(`nf = poseidon(sk, C)`) that reveals only "some capability was consumed", never which one or by whom. The
only structural on-chain leak is **which contract a transaction calls** (`contract_id` + function selector).
Coinbase, uncle, and fee-total amounts are plaintext because they are public or fixed by design — see §2.

## 2. The plaintext exception: native-token fixed quantities

The model's one deliberate exception: three native-token amounts are plaintext, each named by
its entrypoint selector. The rule behind all three — *a quantity that is public or fixed
reveals nothing by being public* — is applied per element below.

| Element | Entrypoint | Why plaintext | Canonical spec |
|---|---|---|---|
| Coinbase reward | `PoWRewardV1` (0x05) | Fixed by the emission schedule; the on-chain check is **exact equality**: `pr.input.value == expected_reward(height)` (HAZOP F1). Fees are NOT part of the coinbase — coinbase = reward only. | [consensus-coinbase.md §17.3](consensus-coinbase.md) |
| Uncle reward | `UncleMintV1` (0x07) | Fixed per-uncle reward (`base / 2^depth`); only the miner's commitment blinds ride in the AEAD note. No cumulative-supply change. | [consensus-coinbase.md §17.4](consensus-coinbase.md) |
| Fees | `FeeV3` (0x08) + `FeeCollectV1` (0x06) | Per-block public totals: fees accumulate as plain `u64` in `fees_db[height]`; collection checks `total_fees == fees_db[height]` and zeroes the pot. | [consensus-coinbase.md §17.2](consensus-coinbase.md), [fee-spec.md §5](consensus/fee-spec.md) |

**Fees in particular** are public by design: the fee *total* is plaintext
(`fees_db[height]`), and the only fee proof material is what the host needs
to audit the retained Fee_V3 mass-balance proof: `fee_value_commit` and
`fee_v3_tx_binding` (see
[`src/contract/native_token/src/model/fee.rs`](../../../src/contract/native_token/src/model/fee.rs)).

### 2.1 The AEAD-note caveat

Coinbase, uncle, and fee-collect outputs each still carry an AEAD-encrypted note. That note
does **not** hide the reward/fee value — the value rides in plaintext call data and is checked
in the clear by the entrypoint. The note hides the miner's commitment blinds (`value_blind`,
derived from `sk_H`), which must stay private so the reward note behaves like any other
spendable note.

### 2.2 Supply-chain consequences

Because the coinbase commitment equals `expected_reward(height)` exactly and fees are minted
separately by `FeeCollectV1`, fees never enter `TOTAL_SUPPLY` and never enter the cumulative
Pedersen supply chain. The cumulative commit advances by reward commitments only.

## 3. Address derivation (`derive_instance`)

`SecretKey::derive_instance(secret, contract_id, instance_id)` (in
`src/sdk/src/crypto/keypair.rs`) computes
`poseidon(DRK_POSEIDON_DOMAIN_KEY_DERIVE, secret, contract_id, instance_elem)`, giving a deterministic
per-(secret, contract, instance) key. There is no reusable identity:

- **Miner coinbase**: instance = block height (`H.to_le_bytes()`), so `sk_H`/`pk_H` rotate every block.
- **Other contracts**: instance = a per-instance `instance_seed`.

A **stable/master identity SHALL NOT appear** in any block or transaction — a stable public key would be a
linkable identity and a privacy break. The header publishes only the cycled `pk_H` (`header.miner`).

## 4. Public vs private inventory

| Artifact | Visibility | Notes |
|---|---|---|
| `contract_id` + function selector | public | the intended leak |
| `*ParamsV1` clear fields | public | coinbase/uncle `value`, `total_pin`, plaintext fee amounts, cumulative-supply scalars |
| `fees_db[height]` | public | per-block plaintext fee total — the FeeCollectV1 equality check (see §2) |
| transfer/spend output **amount** | **private** | Pedersen `value_commit`; `effective_value` is a hidden witness |
| `header.miner` | public (cycled `pk_H`) | unlinkable across blocks via `derive_instance` |
| `header.total_reward` | public | fixed reward magnitude |
| `Output.note` (AEAD ciphertext) | public bytes | value + blinds + `spend_secret` hidden inside |
| `Output.commitment` (Poseidon) | public | hides pk, value, blind |
| `Output.value_commit` (Pedersen) | public | computationally hides value |
| nullifier `nf` | public | unlinkable spend claim |
| ZK public inputs | public | `total_pin` (coinbase = Σ pin; transfers/spends/uncle = 0) |

## 5. Mechanisms

- **AEAD note encryption** (`AeadEncryptedNote`, Sapling DH + ChaCha20Poly1305): hides `value`, `asset_id`,
  blinds, the `spend_secret`, and memo; only the holder of the matching secret can decrypt.
- **Nullifiers** (`nf = poseidon(sk, C)`): the capability claim — spending publishes `nf` into the nullifier
  set, proving the capability was consumed without revealing which note.
- **Pedersen `value_commit`** (`pedersen(value, blind)`): hides the amount; additively auditable.
- **Poseidon `commitment`** (`poseidon(pk, value, asset, hook, data, blind)`): hides the recipient + amount.
- **Per-burn signature pseudonym** (`poseidon(7, spend_secret, nf)`): a fresh signer key per burn, never the
  owner's long-term key.

## 6. Miner model and justification

The miner carries one extra burden: a **per-block derived coinbase key** `sk_H = derive_instance(sk_owner,
NATIVE_TOKEN_CONTRACT_ID, H)` and publishes `pk_H` in `header.miner`. This is the *only* way consecutive
coinbase rewards are not address-linkable — a static reward address would let an observer sum a miner's total
income and link later spends back to every block they mined.

The burden is bounded: one derivation per block (plus the wallet re-deriving `sk_H` per height during scan).
It does **not** provide IP-level anonymity — that is the transport layer's job (below). Uncle minting is
simplified by the fixed-amount rule: uncle pins are plaintext and derived from the public base reward.

## 7. Transport layer (IP unlinkability)

- **Tor**: implemented (`src/net/transport/tor.rs`, Arti client + ephemeral onion service), gated on the
  `p2p-tor` feature and enabled per-operator via `active_profiles` (`tor`, `tor+tls`).
- **Nym**: a **stub** only (`src/net/transport/nym.rs` no-op dialer; empty `p2p-nym` feature). The only Nym
  path today is the SOCKS5 escape hatch (`nym_socks5_proxy`).
- **No enforced anonymity**: plain `tcp`/`tcp+tls` is the default; Tor/Nym are opt-in. A node operator must
  explicitly select an anonymous transport.

## 8. Known gaps

- Nym is unimplemented; IP-level unlinkability depends on the operator opting into Tor (or a future Nym).
