# Contract Trust Model

## Principle: Don't Trust, Verify

A contract manifest is self-reported by the deployer. A malicious deployer can
put "DEX" in the manifest while the WASM drains funds. The wallet applies three
independent layers of verification. None requires trust.

## Three-Layer Model

| Layer | Question | Mechanism | Trust Required |
|-------|----------|-----------|----------------|
| **1. Trust Tier** | Who deployed this? | Genesis check, self-deploy check, attestation lookup | Social (genesis = chain, self = user, attested = issuer) |
| **2. WASM Verification** | Does the manifest match the binary? | Parse WASM exports + circuits, compare against manifest | **None** — mechanical string comparison |
| **3. Attestation** | Does the binary do what it claims? | Trusted issuer inspects WASM, creates on-chain attestation | Social (requires human/DAO judgment) |

## Layer 1: Trust Tier

Every contract gets a trust tier at scan time:

| Tier | Display | How It's Determined |
|------|---------|---------------------|
| **Genesis** | `[GENESIS]` | Contract ID matches one of the [9 genesis contracts](genesis.md) |
| **Self-deployed** | `[OWN]` | Deployer's public key is in the user's wallet |
| **Attested** | `[ATTESTED by <issuer>]` | On-chain attestation from a trusted issuer exists (deferred) |
| **Unverified** | `[UNVERIFIED — manifest is self-reported, verify before use]` | None of the above |

Trust is additive — it can only be upgraded, never downgraded. Genesis and
self-deployed are determined at scan time. Attested requires on-chain
attestation infrastructure (deferred).

The wallet **never blocks interaction** based on trust tier. It warns.
Users decide their own risk tolerance. This is the same principle as
browser SSL warnings: "Not Secure" is shown but the site isn't blocked.

See [Contract Manifest](manifest.md) for how manifests are discovered and stored.

## Layer 2: WASM Verification

**The comparison is implemented (`OBL-C237`), but since the upstream Deployooor port
(2026-10-22) a `DeployV1` call carries no manifest.** `bin/dww/src/scan.rs` still extracts the
embedded zkas binaries from the deployed WASM, and `bin/dww/src/manifest_verify.rs` can compare
them with a manifest — the genesis manifests are embedded in the wallet and checked by its own
controls. A user deployment's manifest is a wallet-side declaration, so there is no
scan-time manifest to compare against that deployment; this is tracked as `OBL-C262`. The
stored verdict (`contract_metadata.manifest_wasm_verdict`) therefore remains empty unless a
manifest was supplied out of band.

This layer answers a mechanical question: does the manifest accurately describe
the binary?

**What it checks**:

- The module parses and exports the five fixed SDK symbols — `memory`, `__initialize`,
  `__entrypoint`, `__update`, `__metadata` (and optionally `__spend_hook`).
- Every ZK circuit declared in the manifest exists in the WASM's data sections, matched as a
  multiset of `(namespace, witness count, opcode count)`. The zkas binary header carries no
  circuit name, and a manifest may declare one namespace for several circuits, so position or
  namespace alone cannot identify a circuit.
- No embedded zkas binary belongs to a namespace the manifest does not declare (a possible
  backdoor).
- A function that names a `proof_circuit` names one the manifest declares.

**What it does not check, and why the old wording was wrong**:

- **"Every function exists as a WASM export" was never checkable.** The SDK exports five fixed
  symbols (`memory` and four functions); contract functions are selector arms inside
  `__entrypoint`, not exports. The deploy entrypoint already requires those five symbols on
  chain (`deployooor`'s `deploy_v1`).
- Whether the WASM logic is correct, whether the circuits are sound, whether the capability
  model is implemented (Layer 3 concern).
- Whether a manifest's `namespace` values agree with the compiled circuits is *checked against
  the binary*, but the repository's own manifests are not all correct: `native_token`,
  `promissory_note` and `identity` declare a contract-wide namespace while their zkas binaries
  carry the circuit name (`Fee_V3`, `Transfer_V2`, `IssueCredentialV2`). The check reports them
  as mismatches until those manifests are corrected with the genesis re-roll (`OBL-C249`).

This is a script-kiddie filter. It catches the most basic deception — someone
editing a manifest TOML to claim functions and circuits that don't exist in
the deployed binary. It provides zero-trust mechanical verification before
any social trust is extended. It does not make a mismatched contract unusable
(the wallet warns; it never blocks), and it records the verdict rather than
discarding the contract.

CLI: `dwow_wallet contract show <contract_id>`

## Layer 3: Attestation

Attestation answers the question that mechanics cannot: does the binary actually
do what it claims? A trusted issuer (auditor, DAO, identity-verified developer):

1. Inspects the deployed WASM and manifest
2. Verifies the manifest accurately describes the WASM's behavior
3. Creates an on-chain attestation via `CreateAttestationV1`

The wallet checks for attestations from issuers in the user's configured trusted
set. This is social verification — it requires trusting the issuer's judgment.

Attestation infrastructure is specified but implementation is deferred.

## Defense in Depth

Each layer catches what the layer below cannot:

```
                    Layer 1            Layer 2            Layer 3
                    (Trust Tier)       (WASM Verify)      (Attestation)

Malicious manifest
with fake functions     ✗ misses          ✓ catches           —
(script kiddie)

Correct manifest,
malicious logic         ✗ misses          ✗ misses            ✓ catches
(sophisticated attack)

Genesis contract        ✓ catches          —                   —
(self-evident trust)
```

No single layer is sufficient. Together they provide defense in depth for
wallet users interacting with contracts they did not deploy themselves.

## See Also

- [Wallet Architecture](wallet.md) — How the wallet performs contract discovery
- [Contract Manifest](manifest.md) — Manifest format, lifecycle, and implementation
- [Object Capabilities](ocap.md) — The O-Cap model that manifests describe
