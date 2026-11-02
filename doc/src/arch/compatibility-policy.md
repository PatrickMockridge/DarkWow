# Compatibility policy

What a release may change, and what a user, node operator or contract developer can rely on. DarkWow
is pre-1.0 (`Cargo.toml` version `0.5.0`); the table states the current posture, not a promise of
backward compatibility.

| Domain | What is stable within a release | What a breaking change means |
|---|---|---|
| Chain identity | The genesis hash and the nine genesis contracts. Genesis PoW is target-MAX, nonce 0; its identity is the pin, not a mined hash. | A change that alters the genesis hash is a new chain. Hard forks coexist; no governance vote decides between them ([overview](overview.md), [genesis](genesis.md)). |
| Consensus rules | Block validity and the supply schedule as implemented. `BlockVersion` (`CURRENT = 1`) is carried in hash preimages and is the soft-fork-signaling field ([type-system](type-system.md)). | A rule change that accepts or rejects a block the old rule did not is a hard fork. Announce the activation height; do not ship it silently. |
| P2P and sync | The handshake checks `app_name`/`app_version` and the sync protocol version; a mismatch is refused (401). The genesis hash is checked at handshake ([p2p-network](net/p2p-network.md), [sync protocol](sync-protocol.md)). | A wire change bumps the version and the peer refuses the mismatch. There is no fallback to an old schema. |
| Transaction, contract params and manifests | The versioned param structs and the manifest `[contract].version` field ([manifest](manifest.md)). | A new version is a new param version or a new contract. Old readers refuse an unknown version; they do not guess. |
| Circuits and manifests | Circuit names are stable; `[[functions]].proof_circuit` selects the active circuit; a contract may keep old circuits for historical reference ([circuit versioning](circuit-versioning.md), [manifest](manifest.md) §Backwards Compatibility). | A hardened circuit is a new `.zk` file with a new name, and the function points at it. Editing an existing circuit under the same name is a breaking change. |
| Deployed contracts | The behavior of the WASM a deployment identifies. A replacement is a new deployment through Deployooor; the chain is not forked ([contracts](../contracts.md)). | Changing a deployed contract's behavior means deploying a new contract, not mutating the old one. |
| SDK and serialization | No stability promise. Every edit under `src/sdk/` or `src/serial/` is a `SOURCE_MANIFEST` input for all 32 contracts: it stales every artifact and can move the genesis pin ([AGENTS.md](../../../AGENTS.md)). | Any such change is rebuilt and freshness-checked for all 32; the pin is re-recorded only through the genesis path. |
| Wallet and keys | Address and instance derivation and the wallet's scan rules within a release ([wallet](wallet.md)). Forwarding keys may be rotated; mining keys are never copied. | A change to derivation or scan is announced; an old wallet that cannot scan the new chain must re-sync. |
| Local data | No compatibility promise for the data directory. | Re-sync from genesis across an incompatible release. Do not hand-edit consensus state. |

## Definition of a breaking change

A change is breaking if it changes a block's validity under rules an existing node applies; changes
the genesis hash or the genesis contracts; changes a wire format, param version or manifest field in
a way an existing reader cannot parse; changes a circuit under the same name; changes wallet scan or
derivation so an existing wallet cannot find its notes; or changes a public CLI or JSON-RPC field in
a way an existing client cannot read.

## Release discipline

A release is clear only when the tracker is settled (no Intolerable row; every Tolerable row closed
or residual; the approval hash matches), the build and pin evidence exists in the run that produced
it, and this policy is updated for every breaking change in the release. The authoritative checklist
is in [`launch-risk-register.md`](launch-risk-register.md) and the bands and approval are in
[`risk-management.md`](risk-management.md).

## Deprecation

Where a change can be phased, the release that introduces the replacement states what is deprecated,
what still works, and the release in which it will be removed. A deprecation with no removal release
is a note, not a deprecation. A security fix is exempt from the phase-in when the defect is
Intolerable.

## Exceptions

There are no silent exceptions. An incompatibility that ships without a version bump or an
announcement is a defect and is recorded in the tracker.
