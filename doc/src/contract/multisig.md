# MultiSig — Threshold Signature Factory

The MultiSig contract is a genesis-deployed (ContractId counter 10) threshold
signature factory. It creates N-of-M groups, collects partial signatures from
key holders (proving key ownership via ZK), and produces approval capabilities that any other contract
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

## Private Voting — The Missing Primitive

Private voting is not a feature. It is a protected democratic right. From the
nation state to the housing association, from the multinational corporation to
the street vendor collective, every human organization faces moments where the
stakes are high and power is asymmetric — and in those moments, the ability to
vote without fear of retaliation is the difference between genuine consent and
coerced acquiescence.

### The Problem

Every existing blockchain governance system has the same structural flaw: votes
are public. In DAOs, token holders vote with their wallets on-chain for everyone
to see. In corporate governance, shareholder votes are a matter of public record.
In trade union ballots, employer retaliation is a documented reality — workers
who vote to strike can be identified and punished. In referendums, voters in
authoritarian contexts face imprisonment, property seizure, or violence for
voting the wrong way. In all of these cases, the **absence of private voting
is the presence of coercion**.

The cryptographic techniques that could solve this — ring signatures, stealth
addresses, zero-knowledge proofs — have existed for over a decade. But no
blockchain has deployed them as a **governance primitive**. Voting systems exist
in individual DAO smart contracts, each hand-rolling its own tally logic, each
leaking who voted and how. What has never existed is a **genesis-deployed,
zero-knowledge, fully private threshold signature factory** that any contract,
any organization, any collective can compose with.

### What MultiSig Changes

MultiSig is that primitive. It is a **capability threshold system** — what matters
is whether enough valid capabilities (partial signatures proving key ownership) have
been presented, not which specific keys presented them:

- **The capability holder set is public** — the list of public keys authorized to
  produce partial signatures is known. This is essential for legitimacy: the
  participants in any collective decision need to agree on who the eligible
  voters are. A union needs to know which workers hold bargaining rights, a
  nation needs to know which citizens hold voting rights, a corporation needs to
  know which shareholders hold voting shares. The group IS its capability holder
  set — a list of cryptographic keys, not identities.

- **Capability exercise is hidden** — which specific key holders exercised their
  capability is not revealed on-chain. The nullifier proves that *some* authorized
  key holder signed, but not *which* one. This is the cryptographic equivalent of
  a ballot box: you can verify that only authorized capability holders cast votes,
  but you cannot trace a ballot back to a voter. The capability is presented; the
  holder is not identified.

- **The threshold is the verdict** — when M of N partial signatures are
  collected and finalized, the approval capability is produced. The world
  learns that a threshold was met. It does not learn who signed, how they
  voted, or even what the question was — only that the authorized set of
  capability holders reached a decision by the required margin.

### Absolute vs. Contextual Privacy

MultiSig supports two configurations:

| Mode | Capability holder set | Capability exercise attribution | Use case |
|------|-----------------|------------------|----------|
| **Absolute privacy** | Public key list on-chain | Keys known to group, nullifier opaque to chain | Trade union ballot, corporate board vote, independence referendum |
| **Contextual privacy** | Group created off-chain, nullifiers submitted by delegates | Neither holder set nor exercise visible | Whistleblower protection, dissident coordination, human rights monitoring |

In absolute privacy mode, the capability holders know each other — like a union
local, a housing cooperative, or a parliamentary committee. The chain sees the
public keys but cannot connect any individual nullifier to any individual key.

In contextual privacy mode, even the holder set is hidden. A delegate submits
partial signatures on behalf of capability holders without revealing who
delegated. This is the cryptographic equivalent of a secret ballot in a
jurisdiction where even being ON the voter roll is dangerous.

### Why This Has Never Existed Before

The blockchain and DAO space has spent a decade building governance systems
that are either transparent (and therefore coercible) or centralized (and
therefore corruptible). The transparent systems — Moloch, Compound Governor,
Snapshot — make every vote public. The centralized systems — multisig wallets,
Gnosis Safe — put trust in a small number of identified signers who can be
targeted. Neither approach solves the problem of **asymmetric power**. When the
employer can see how you voted, the union ballot is not free. When the state
can see how you voted, the referendum is not fair. When the majority can see
how the minority voted, the vulnerable are not protected.

MultiSig is the first genesis-deployed, zero-knowledge, fully composable
threshold signature primitive in any blockchain. It does not replace DAOs —
it provides the cryptographic foundation for DAOs to become genuinely
democratic rather than merely transparent.

### The UN Declaration and Decolonization

The UN Declaration on the Rights of Indigenous Peoples (UNDRIP, 2007) affirms
the right of all peoples to self-determination: "to freely determine their
political status and freely pursue their economic, social and cultural
development." Article 3 explicitly extends the right of self-determination
in the International Covenant on Civil and Political Rights to indigenous
peoples — the same right that underpinned decolonization across Africa, Asia,
and the Caribbean in the 20th century.

What has always been missing is the **mechanism**. A colonized region cannot
ask the colonizing administration for permission to hold an independence
referendum. A minority within a nation cannot ask the majority to provide
neutral referendum infrastructure. An indigenous community cannot trust a
state that has historically dispossessed them to count their votes fairly.

MultiSig provides the mechanism. A community creates a MultiSig group from
their own public keys. They set their own threshold. They vote. The result
is cryptographically verifiable by anyone — but no individual vote is
traceable. This is **grassroots, ground-up self-determination**: not asking
permission, not trusting intermediaries, not depending on colonizing
administrations for democratic legitimacy.

From the nation state to the housing association, from the trade union to
the street vendor collective, private voting is the primary governance and
dispute resolution mechanism at all scales. Every human right — to food, to
shelter, to healthcare, to education, to work, to family life — depends on
the ability of people to organize, to make collective decisions, and to
hold power to account. That starts with a vote that cannot be coerced.
MultiSig is that vote.

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `InitializeV1` | 0x00 | — | Initialize MultiSig contract state |
| `CreateGroupV1` | 0x01 | `create_group.zk` | Group parameters valid: threshold ≥ 1, ≤ N. Produces group_capability. |
| `SignV1` | 0x02 | `sign.zk` | Key holder proves key ownership: pubkey = secret·G. Produces partial_signature. |
| `FinalizeV1` | 0x03 | `finalize.zk` | Threshold partial signatures collected for a message. Consumes them, produces approval capability. |

## Privacy Properties

- **Capability holder set public** — pubkeys are stored on-chain (the group IS its holder set)
- **Capability exercise hidden** — which specific key holders exercised their capability is not revealed on-chain (nullifiers are opaque)
- **Approval unlinkable** — the approval capability commitment reveals only that SOME threshold was met, not which group or message
- **Double-exercise prevention** via nullifier: `poseidon_hash(group_id, message_hash, signer_pubkey)`

## Data Model

```
MultiSigGroup = {
    group_id:   poseidon_hash(pubkeys || threshold),
    pubkeys:    [P_1, P_2, ..., P_N],   // compressed public keys
    threshold:  M,                        // required signatures
    total_keys: N,                        // total key holders
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
| `P_i` | Public key of capability holder i |
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
| [escrow](escrow.md) | Multi-party release approval | `Box::TakeV1(approval)` before claim |
| [dao_escrow](dao_escrow.md) | Treasury spend authorization, endowment withdrawal | `Box::TakeV1(approval)` replaces hand-rolled quorum |
| [drain_protection](drain_protection.md) | Large withdrawal authorization, lock/unlock votes | `Box::TakeV1(approval)` replaces per-action thresholds |
| [pool_stake](pool_stake.md) | Coverage allocation changes, slashing decisions | `Box::TakeV1(approval)` |
| [bridge](bridge.md) | Large withdrawal approval, operator rotation | `Box::TakeV1(approval)` |
| [betting_stake](betting_stake.md) | Risk parameter updates, table configuration | `Box::TakeV1(approval)` |

## References

- [Object Capability Model](../arch/ocap.md) — MultiSig in the O-Cap stack
- [Box](box.md) — The single-capability container (composes with MultiSig approvals)
- [Purse](purse.md) — The fungible asset container (MultiSig secures Purse operations)
- [Wallet Architecture](../arch/wallet.md) — How the wallet discovers MultiSig capabilities
- [Contract Manifest](../arch/manifest.md) — On-chain interface discovery
- Source: `src/contract/multisig/`
