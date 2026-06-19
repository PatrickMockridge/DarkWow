# Box — ZK Capability Container

The Box contract is the DarkWow equivalent of capability delegation — put a
capability into a Box, and whoever Takes it receives it. It is an **O-Cap
primitive** deployed at genesis (ContractId counter 9).

## Why Genesis?

In the Agoric o-cap model, Invitations are Payments that grant access to a
specific contract interaction. Box is the ZK-native equivalent: it holds an
arbitrary capability and transfers it via linear consumption (nullifier).
Having a canonical well-known ContractId makes Box available as a composable
primitive for every contract in the ecosystem — any contract can deposit a
capability into a Box, and any wallet can Take from a Box it holds the secret for.

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `PutV1` | 0x01 | `put_v1.zk` | Box was empty, now contains H(capability_data). Schnorr ownership proof. |
| `TakeV1` | 0x02 | `take_v1.zk` | Knowledge of box_secret. Box consumed via nullifier. Contents not empty. |

## Privacy Properties

- **Capability type hidden** — the contents commitment reveals only that SOMETHING was placed
- **No on-chain link between Put and Take** — different public keys, fresh blinds
- **Box consumption via nullifier** — prevents double-opening (linear use)
- **Ownership transfer via AEAD note re-encryption** — same pattern as coin transfer

## Data Model

```
Box = {
    box_id:           poseidon_hash(creator_pub, nonce),
    contents_commit:  poseidon_hash(capability_data),
    is_empty:         bool,
}
```

## Database Trees

| Tree | Purpose |
|------|---------|
| `boxes` | Box records keyed by box_id |
| `nullifiers` | Spent take nullifiers (double-open prevention) |
| `info` | Contract metadata |

## References

- [Object Capability Model](../arch/ocap.md) — Box in the O-Cap stack
- [Wallet Architecture](../arch/wallet.md) — How the wallet interacts with Boxes
- Source: `src/contract/box/`
