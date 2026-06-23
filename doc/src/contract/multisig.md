# MultiSig — Threshold Signature Factory

The MultiSig contract is a genesis-deployed (ContractId counter 10) threshold
signature factory. It creates N-of-M groups, collects partial signatures from
group members (proving key ownership via ZK), and produces approval capabilities that any other contract
can compose with. It is an **O-Cap primitive** that extracts duplicated threshold
logic from DAO-Escrow and DrainProtection into a shared, auditable foundation.

## Why Genesis?

Every contract that needs multi-party authorization — escrow release, DAO treasury
spend, drain protection override, bridge withdrawal — currently hand-rolls its own
threshold verification in WASM entrypoint code. The same pattern appears in at
least six contracts: count signatures, check against threshold, produce an
authorization. Each implementation is a separate audit surface.

MultiSig pulls this pattern into a single genesis primitive. A canonical well-known
ContractId means any contract can require "approval from MultiSig group X" without
knowing the threshold, the group size, or even the signature scheme. The primitive
verifies the threshold; the composing contract checks for the approval capability.

This is the same structural relationship that Deployooor has to deployed contracts:
the factory is genesis-tier, the instances are user-created, and trust flows from
the audited primitive to the configured group.

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `CreateGroupV1` | 0x01 | `create_group_v1.zk` | Group parameters valid: threshold ≥ 1, ≤ N. Produces group_capability. |
| `SignV1` | 0x02 | `sign_v1.zk` | Group member proves key ownership: pubkey = secret·G. Produces partial_signature. |
| `FinalizeV1` | 0x03 | `finalize_v1.zk` | Threshold partial signatures collected for a message. Consumes them, produces approval capability. |

## Privacy Properties

- **Group membership public** — pubkeys are stored on-chain (the group IS its member list)
- **Signature attribution hidden** — which specific members signed is not revealed on-chain (nullifiers are opaque)
- **Message content hidden** — only the message hash appears on-chain, not the message itself
- **Approval unlinkable** — the approval capability commitment reveals only that SOME threshold was met, not which group or message
- **Double-sign prevention** via nullifier: `poseidon_hash(group_id, message_hash, signer_pubkey)`

## Data Model

```
MultiSigGroup = {
    group_id:   poseidon_hash(pubkeys || threshold),
    pubkeys:    [P_1, P_2, ..., P_N],   // compressed public keys
    threshold:  M,                        // required signatures
    total_keys: N,                        // total group members
}

PartialSignature = {
    group_id:       Group reference,
    message_hash:   H(msg),
    signer_pubkey:  P_i,
    nullifier:      poseidon_hash(group_id, message_hash, P_i),
}

ApprovalCapability = {
    approval_commit:  poseidon_hash(group_id, message_hash),
    // Produced when ≥ M partial signatures exist for (group_id, message_hash)
}
```

## Database Trees

| Tree | Purpose |
|------|---------|
| `groups` | MultiSigGroup records keyed by group_id |
| `signatures` | PartialSignature records keyed by nullifier |
| `nullifiers` | Spent signature nullifiers (double-sign prevention) |
| `info` | Contract metadata |

## Formal Specification

### Notation

| Symbol | Meaning |
|--------|---------|
| `G` | Pallas curve generator |
| `H(x)` | Poseidon hash of field elements |
| `P_i` | Public key of group member i |
| `GID` | Group identifier: H(P_1.x, P_1.y, ..., P_N.x, P_N.y, M) |
| `MH` | Message hash: H(msg_bytes) |
| `tx_hash` | Transaction commitment |
| `tx_nonce` | Transaction nonce |

### CreateGroupV1 — Group Creation

**Public inputs (exposed via `constrain_instance`):**
```
GID = H(P_1.x, P_1.y, ..., P_N.x, P_N.y, M)
tx_binding = H(tx_hash, tx_nonce)
```

**Constraints:**
```
1. 1 ≤ M ≤ N
2. tx_binding = H(tx_hash, tx_nonce)
3. constrain_instance(GID)
4. constrain_instance(M)
5. constrain_instance(N)
```

**State transition:**
```
groups[GID] ← (pubkeys = [P_1..P_N], threshold = M, total_keys = N)
```

### SignV1 — Partial Signature

**Public inputs:**
```
GID = group being signed for
MH  = message hash
tx_binding = H(tx_hash, tx_nonce)
```

**Constraints:**
```
1. Key ownership:       P_i = sk_i · NULLIFIER_K
2. tx_binding = H(tx_hash, tx_nonce)
3. constrain_instance(GID)
4. constrain_instance(MH)
```

**State transition:**
```
nullifier_i = H(GID, MH, P_i.x, P_i.y)
signatures[nullifier_i] ← (group_id = GID, message_hash = MH, signer_pubkey = P_i)
nullifiers[nullifier_i] ← ∅   (double-sign prevention)
```

### FinalizeV1 — Threshold Finalization

**Public inputs:**
```
GID = group being finalized for
MH  = message hash
tx_binding = H(tx_hash, tx_nonce)
```

**Constraints:**
```
1. approval_commit = H(GID, MH)
2. tx_binding = H(tx_hash, tx_nonce)
3. constrain_instance(GID)
4. constrain_instance(MH)
```

**State transition (WASM entrypoint, not ZK circuit):**
```
signatures_db ← lookup signatures tree
collected ← []
for each P_i in group.pubkeys:
    nullifier_i = H(GID, MH, P_i.x, P_i.y)
    if signatures_db contains nullifier_i:
        collected.append(nullifier_i)

assert len(collected) ≥ group.threshold

for each nullifier in collected:
    signatures[nullifier] ← consumed marker
```

### Transaction Binding

Every ZK circuit binds to a specific transaction via:
```
tx_binding = H(tx_commitment, tx_nonce)
constrain_instance(tx_binding)
constrain_instance(tx_nonce)
```

This prevents signature replay across transactions and ensures partial signatures
can only be finalized within the transaction they were collected for.

### Capability Type Discriminants

| Type | Discriminant (u8) | Structure |
|------|-------------------|-----------|
| `group_capability` | 0x00 | `(group_id, creator_pubkey, nonce)` — holder can manage the group |
| `partial_signature` | 0x01 | `(nullifier, group_id, message_hash)` — consumed by FinalizeV1 |
| `approval` | 0x02 | `(approval_commit, group_id, message_hash)` — composable by other contracts |

## Composing Contracts

MultiSig is a genesis primitive — deployed once at genesis (counter 10). Other
contracts compose with approval capabilities via `Box::Take` or direct capability
checks. The composing contract never needs to know the threshold or group size.

| Contract | What MultiSig Authorizes | Composition |
|----------|-------------------------|-------------|
| [escrow](escrow.md) | Multi-party release approval | `Box::Take(approval)` before claim |
| [dao_escrow](dao_escrow.md) | Treasury spend authorization, endowment withdrawal | `Box::Take(approval)` replaces hand-rolled quorum |
| [drain_protection](drain_protection.md) | Large withdrawal authorization, lock/unlock votes | `Box::Take(approval)` replaces per-action thresholds |
| [pool_stake](pool_stake.md) | Coverage allocation changes, slashing decisions | `Box::Take(approval)` |
| [bridge](bridge.md) | Large withdrawal approval, operator rotation | `Box::Take(approval)` |
| [betting_stake](betting_stake.md) | Risk parameter updates, table configuration | `Box::Take(approval)` |

## References

- [Object Capability Model](../arch/ocap.md) — MultiSig in the O-Cap stack
- [Box](box.md) — The single-capability container (composes with MultiSig approvals)
- [Purse](purse.md) — The fungible asset container (MultiSig secures Purse operations)
- [Wallet Architecture](../arch/wallet.md) — How the wallet discovers MultiSig capabilities
- [Contract Manifest](../arch/manifest.md) — On-chain interface discovery
- Source: `src/contract/multisig/`
