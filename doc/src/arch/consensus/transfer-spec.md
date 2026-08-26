# Capability Exercise (Transfer) Specification

This is the **normative specification** for the operations that move value or capability on-chain.
Its unit of account is the **capability**. There is **no value-unit type**: the value carriers are the
**native token (DRKW)** and **capabilities** (promissory notes, box, purse). The general operation is
the **Exercise+Consume phase of the capability lifecycle**: *consume+create*.

It SHALL be read with the four specs it is grounded in:

- [type-system.md](../type-system.md) — primitive types and barbs (§8.1).
- [ocap.md](../ocap.md) — capability composition and the capability lifecycle (§6).
- [wallet.md](../wallet.md) — the wallet as a capability type construction engine (Path 1 / Path 2,
  write path §6).
- [contract-wasm-type-system.md](../contract-wasm-type-system.md) — the consensus-vs-capability
  taxonomy (§A.0.5) and the fundamental contract invariant (§A.0.3).
- [manifest.md](../manifest.md) — the manifest as the type declaration (`note_schema`).

The mechanization lives in `proofs/lean/src/DarkFi/Capability/` (Types, Composition, and the new
Exercise/Value/NativeToken modules).

## 1. Primitive types (no value-unit primitive)

Per type-system.md §8.1, the primitive types available for composition are:

| Primitive | Barbs | Role |
|---|---|---|
| `SecretKey` | `↓spend`, `↓derive` | ν-restricted authorization name |
| `PublicKey` | `↓verify`, `↓encrypt` | receives encrypted notes |
| `Nullifier` | `↓nullify` | replay prevention (public) |
| `Commitment` | `↓commit` | the commitment face of a capability |
| `ContractId` | `↓dispatch` | routes to the recognizing contract |
| `FuncId` | `↓gate` | constrains which function exercises the capability |
| `AssetId` | `↓denominate` | the asset denomination face |
| `MerkleNode` | `↓prove-inclusion` | proves inclusion in the recognized set |

There is **no value-unit primitive**. The native token's output is a *specific capability instance*
whose `Commitment` carries value and whose `AssetId` is DRKW. (The Lean4 `Capability/Types.lean`
currently uses a legacy identifier for the `Commitment` primitive; it SHALL be read as `Commitment`.)

## 2. The capability lifecycle (the "transfer" is Exercise + Consume)

A capability is a name in the ρ-calculus: a composition of primitives whose barbs cover the action's
`required_barbs` (`wallet_construct`). The lifecycle (ocap.md §6) is:

| Phase | Generic operation | DarkWow mechanism |
|---|---|---|
| Create | `ν name. publish!(commit(name, params))` | `Commitment = poseidon(attributes)` |
| Discover | receive the name through transport | AEAD trial decryption (wallet.md §2) |
| Hold | store `(name, commitment, inclusion_proof)` | `CapRecord` + `capability_proofs` |
| **Exercise** | `generate_proof(name, L_{r,s})` | Halo2 `Proof::create` inhabiting `L_{r,s}` |
| Verify | `verify?(proof, evidence, predicate)` | `Proof::verify` against public inputs |
| **Consume** | `consume!(nullifier)` | nullifier published in `Transaction.nullifiers` |

**A "transfer" is Exercise + Consume together — *consume+create*:** the exercised capabilities are
consumed (their nullifiers published), and the produced capabilities are created (fresh commitments).
The generic grammar applies to **every** capability. What distinguishes contracts is (a) whether the
capability carries *value* (via its `Commitment` + `AssetId`) or *state* (witness-only), and (b) the
contract taxonomy below.

The **fundamental contract invariant** (contract-wasm-type-system.md §A.0.3): a contract SHALL accept
a call iff its data satisfies the declared barbs — and for an exercise, that means the nullifier is
fresh (Consume) and the commitment is new (Create).

## 3. Value conservation is a property of value-denominated capabilities

Some capabilities are **value-denominated**: their `Commitment` carries a hidden value and their
`AssetId` denominates it (the native token DRKW, and PromissoryNote's wrapped/stablecoin tokens). For
these, exercise MUST conserve value per asset: `Σ input value_commit == Σ output value_commit`
(Pedersen homomorphism) — mechanized as `CrossCutting.pedersen_value_conservation`. This is the
anti-inflation gate and applies **only** where a capability carries value.

State capabilities (Box contents, Purse balance) carry **no** value; their transition (old_state →
new_state) is enforced in the ZK circuit, not by value sums. There is no "value conservation" for a
Box or a Purse.

## 4. The native token — the ONE consensus contract

Per contract-wasm-type-system.md §A.0.5, contracts are **Consensus** (NativeToken, Deployooor) or
**Capability** (everything else). NativeToken is the sole consensus asset: it SHALL provide **coinbase,
fee payment, and transfers only** — "rock-dumb", no freeze/ACL/governance/composition.

Its specializations are the *only* value-unit minting and destruction:

- **Coinbase mint (`PoWRewardV1`, `0x05`)** — value created ex nihilo, gated by the emission schedule
  (`input.value = expected_reward(height)`, cumulative supply `S_H = S_{H-1} + C_H`) and held back by
  `COINBASE_MATURITY = 100` (claim nullifier tracked host-side, not as a double-spend).
- **Transfer (`TransferV1`, `0x03`)** — an ordinary capability exercise (Consume + Create) with §3's
  value conservation.
- **Fee (`fee_v2`, `0x08`) / fee-collect (`FeeCollectV1`, `0x06`)** — the fee path; value conservation
  deferred to the host mass-balance proof.

See [consensus-coinbase.md](consensus-coinbase.md) and [fee-spec.md](fee-spec.md).

## 5. Capability contracts are lifecycle instances, not special cases

PromissoryNote, Box, and Purse are **capability** contracts (not consensus). They are instances of the
§2 lifecycle, keyed to the manifest `note_schema` (L1 trajectory vs L2 flat-note — wallet.md §2.3):

- **PromissoryNote** — value-denominated capabilities (a DeFi token layer). `transfer_v1`/`otc_swap_v1`
  are consume+create with §3 value conservation; `revoke_v1`/`redeem_v1` are consume-with-destroy;
  `issue_v1`/`register_type_v1` are create (mint).
- **Box / Purse** — state capabilities (witness-only object state). `put`/`take`, `deposit`/`withdraw`
  are consume+create with the transition enforced in-circuit; they use their own bespoke trees
  (`box_roots`, `purse_roots`) and block-anchor `(nullifier, cid, contract_root)`.

A new contract conforms to this spec by (1) declaring its capabilities in the manifest and (2)
enforcing its declared barbs in exec. No wallet code change is required — the wallet is a generic
capability engine (wallet.md §9).

## Conformance

A change to any of these functions SHALL keep it an instance of the §2 lifecycle with its §3 value
conservation (where value-denominated) or §4 native-token specialization, or the change is a
regression. The wallet's write path (`bin/dww/src/walletdb.rs`) MUST reproduce the native token's
commitment-tree historical root exactly — that is the bespoke Path-1 concern (wallet.md §6.4.0), not a
general capability rule.

The receive side of a transfer is only as safe as its transport framing: the recipient discovers the
inbound note/commitment frames over the P2P channel, whose receive loop SHALL be frame-aligned by
construction (type-system.md §10.5.2, proved in `DarkFi.Net.Framing`). A transport that leaves a frame
half-read is a regression of the transfer receive path even though the lifecycle/value-conservation
logic is unchanged.

## References

- [type-system.md](../type-system.md) — primitive types §8.1, barb preservation
- [ocap.md](../ocap.md) — capability composition §2, lifecycle §6, Authorization Inversion §3
- [wallet.md](../wallet.md) — Path 1/Path 2, write path §6
- [contract-wasm-type-system.md](../contract-wasm-type-system.md) — taxonomy §A.0.5, invariant §A.0.3
- [manifest.md](../manifest.md) — note_schema, actions, functions, circuits
- [consensus-coinbase.md](consensus-coinbase.md), [fee-spec.md](fee-spec.md)
- `proofs/lean/src/DarkFi/Capability/{Types,Composition,Exercise,Value,NativeToken}.lean`
