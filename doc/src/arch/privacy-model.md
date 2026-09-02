# Privacy Model

DarkWow's privacy model is a **practical balance**: the emission schedule (coinbase and uncle rewards) is a
fixed, public quantity, so those amounts are plaintext; every **subsequent** transfer/spend amount is private.
Identities are hidden behind per-instance derived addresses, ZK proofs, nullifiers, and AEAD-encrypted notes.

## 1. The model in one paragraph

A user holds a root secret and derives a fresh, unlinkable key per (contract, instance) via
`derive_instance`. Each transaction mints/spends notes whose **values and recipient identities are hidden**
behind Pedersen/Poseidon commitments and AEAD ciphertext; a spend is proven by publishing a **nullifier**
(`nf = poseidon(sk, C)`) that reveals only "some capability was consumed", never which one or by whom. The
only structural on-chain leak is **which contract a transaction calls** (`contract_id` + function selector).
Coinbase and uncle reward amounts are plaintext because they are fixed by the emission schedule
(`expected_reward(H)`, `base / 2^depth`) and therefore reveal nothing an observer cannot already compute.

## 2. Address derivation (`derive_instance`)

`SecretKey::derive_instance(secret, contract_id, instance_id)` (in
`src/sdk/src/crypto/keypair.rs`) computes
`poseidon(DRK_POSEIDON_DOMAIN_KEY_DERIVE, secret, contract_id, instance_elem)`, giving a deterministic
per-(secret, contract, instance) key. There is no reusable identity:

- **Miner coinbase**: instance = block height (`H.to_le_bytes()`), so `sk_H`/`pk_H` rotate every block.
- **Other contracts**: instance = a per-instance `instance_seed`.

A **stable/master identity SHALL NOT appear** in any block or transaction — a stable public key would be a
linkable identity and a privacy break. The header publishes only the cycled `pk_H` (`header.miner`).

## 3. Public vs private inventory

| Artifact | Visibility | Notes |
|---|---|---|
| `contract_id` + function selector | public | the intended leak |
| `*ParamsV1` clear fields | public | coinbase/uncle `value`, `total_pin`, fee amounts, cumulative-supply scalars |
| transfer/spend output **amount** | **private** | Pedersen `value_commit`; `effective_value` is a hidden witness |
| `header.miner` | public (cycled `pk_H`) | unlinkable across blocks via `derive_instance` |
| `header.total_reward` | public | fixed reward magnitude |
| `Output.note` (AEAD ciphertext) | public bytes | value + blinds + `spend_secret` hidden inside |
| `Output.commitment` (Poseidon) | public | hides pk, value, blind |
| `Output.value_commit` (Pedersen) | public | computationally hides value |
| nullifier `nf` | public | unlinkable spend claim |
| ZK public inputs | public | `total_pin` (coinbase = Σ pin; transfers/spends/uncle = 0) |

## 4. Mechanisms

- **AEAD note encryption** (`AeadEncryptedNote`, Sapling DH + ChaCha20Poly1305): hides `value`, `asset_id`,
  blinds, the `spend_secret`, and memo; only the holder of the matching secret can decrypt.
- **Nullifiers** (`nf = poseidon(sk, C)`): the capability claim — spending publishes `nf` into the nullifier
  set, proving the capability was consumed without revealing which note.
- **Pedersen `value_commit`** (`pedersen(value, blind)`): hides the amount; additively auditable.
- **Poseidon `commitment`** (`poseidon(pk, value, asset, hook, data, blind)`): hides the recipient + amount.
- **Per-burn signature pseudonym** (`poseidon(7, spend_secret, nf)`): a fresh signer key per burn, never the
  owner's long-term key.

## 5. Miner model and justification

The miner carries one extra burden: a **per-block derived coinbase key** `sk_H = derive_instance(sk_owner,
NATIVE_TOKEN_CONTRACT_ID, H)` and publishes `pk_H` in `header.miner`. This is the *only* way consecutive
coinbase rewards are not address-linkable — a static reward address would let an observer sum a miner's total
income and link later spends back to every block they mined.

The burden is bounded: one derivation per block (plus the wallet re-deriving `sk_H` per height during scan).
It does **not** provide IP-level anonymity — that is the transport layer's job (below). Uncle minting is
simplified by the fixed-amount rule: uncle pins are plaintext and derived from the public base reward.

## 6. Transport layer (IP unlinkability)

- **Tor**: implemented (`src/net/transport/tor.rs`, Arti client + ephemeral onion service), gated on the
  `p2p-tor` feature and enabled per-operator via `active_profiles` (`tor`, `tor+tls`).
- **Nym**: a **stub** only (`src/net/transport/nym.rs` no-op dialer; empty `p2p-nym` feature). The only Nym
  path today is the SOCKS5 escape hatch (`nym_socks5_proxy`).
- **No enforced anonymity**: plain `tcp`/`tcp+tls` is the default; Tor/Nym are opt-in. A node operator must
  explicitly select an anonymous transport.

## 7. Known gaps

- Reward/fee magnitudes and cumulative-supply scalars (`total_reward`, `expected_cumulative_supply`,
  `old_cumulative_blind`) are public by design.
- Nym is unimplemented; IP-level unlinkability depends on the operator opting into Tor (or a future Nym).
