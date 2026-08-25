# Identity — O-Cap Authorization Primitive (L2)

## The Capability

Identity is the **object-capability authorization** primitive: it issues
credentials and verifies capabilities — proving *access* without revealing
*identity*. It is an **L2 static record** contract (direct KV lookup, no
consume+create commitment state); the two ZK circuits prove predicate relationships
over recorded credentials.

**Trust tier:** ecosystem infrastructure (genesis counter 5). Not consensus-critical.

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `initialize` | — | Seed config |
| `0x01` | `issue_credential` | `IssueCredentialV2` | Issuer issues a credential to a holder |
| `0x02` | `revoke_credential` | — (Schnorr sig) | Issuer revokes a credential |
| `0x04` | `register_capability` | — | Register a capability type |
| `0x05` | `issue_capability` | — (Box::Put child) | Issue a capability to a holder |
| `0x06` | `verify_capability` | `VerifyCapabilityV2` | Verify a capability proof against a credential |
| `0x07` | `revoke_capability` | — | Revoke a holder's capability |
| `0x08` | `register_issuer` | — | Register a trusted issuer |

`0x03` (`create_claim`) was removed — the 5-mode claim was unconsumed dead
weight; consumers (dao_escrow, labor_market) only call `verify_capability`.

## Domain Constants

`NULLIFIER = witness_base(1)`, `TX_BINDING = witness_base(3)`,
`COIN_COMMIT = witness_base(4)`. Both circuits use `NULLIFIER_K` as the EC
base point for key derivation.

## Data Model

```
issuer_public     = ec_mul_base(issuer_secret, NULLIFIER_K)
credential_data   = poseidon_hash(4, issuer_pub_x, issuer_pub_y, holder_pub_x, holder_pub_y, schema_hash, attr_1, attr_2, attr_blind)
commitment        = poseidon_hash(4, credential_data, credential_secret, issued_at, expires_at)
credential_nullifier = poseidon_hash(1, credential_secret, commitment)
tx_binding        = poseidon_hash(3, tx_commitment, tx_nonce)
```

## Barbs

| Barb | Mechanism |
|------|-----------|
| `↓spend` | credential nullifier `poseidon_hash(1, credential_secret, commitment)` proves holder secret |
| `↓verify` | `VerifyCapabilityV2` constrains the predicate and the capability relation `poseidon_hash(1, capability_secret, capability_id)` |

## The Four-Component Flow

1. **Circuit** — derives issuer/credential/nullifier/predicate; constrains to witnesses.
2. **Params** — caller pre-computes `commitment`/`nullifier`/`tx_binding` with domain constants.
3. **Metadata** — echoes the `constrain_instance` values (`[nullifier, tx_binding, tx_nonce]` or `[commitment, tx_binding, tx_nonce]`).
4. **Exec** — validates credential exists + not revoked (issue/verify); **Apply** — writes records (credential, capability, nullifier mark). Non-ZK functions (revoke, register, issue-capability) are signature/state-authorized, not circuit-gated.

## State Trees

| Tree | Purpose |
|------|---------|
| `credentials` | Issued credentials (keyed by nullifier) |
| `nullifiers` | Revocation tracking |
| `issuers` | Trusted issuers (keyed by `compute_issuer_key`) |
| `config` | Version |
| `capabilities` | Capability definitions + issued count |
| `identity_info` | Metadata |

## Authorization

Three roles, each a distinct o-cap:

- **Issuer** — registers (`register_issuer`), issues credentials (`issue_credential`
  proves `issuer_secret`), revokes (`revoke_credential` Schnorr-signed by `issuer_pub`).
- **Holder** — receives a credential, then proves predicates over it via
  `verify_capability` without revealing the attributes.
- **Verifier** — verifies a capability proof (`verify_capability`) against a
  registered capability requirement.

The ZK proof reveals only the predicate result (`predicate_result`), never the
attribute values or holder identity — the Authorization Inversion Theorem
(`type-system.md`): *prove access without revealing who you are*.

## References

- [Identity Specification](../../../doc/src/contract/identity.md)
- [O-Cap Model](../../../doc/src/arch/ocap.md)
- [Type System](../../../doc/src/arch/type-system.md)
- [Contract WASM Type System](../../../doc/src/arch/contract-wasm-type-system.md) — Part B (L2)
- [Wallet Architecture](../../../doc/src/arch/wallet.md)
- Source: `src/contract/identity/`
