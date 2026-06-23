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

This layer answers a mechanical question: does the manifest accurately describe
the binary? It downloads the contract WASM from dwowd, extracts the export
section and ZK circuit data, and cross-references against the manifest.

**What it checks**:
- Every function declared in the manifest exists as a WASM export
- Every ZK circuit declared in the manifest exists in the WASM data sections
- Circuit namespaces match between manifest and WASM
- No circuit is declared without a function referencing it
- No undeclared circuits exist in the WASM (possible backdoors)

**What it does NOT check** (Layer 3 concern):
- Whether the WASM logic is correct
- Whether the ZK circuits are sound
- Whether the capability model is properly implemented

This is a script-kiddie filter. It catches the most basic deception — someone
editing a manifest TOML to claim functions and circuits that don't exist in
the deployed binary. It provides zero-trust mechanical verification before
any social trust is extended.

CLI: `dwow_wallet contract verify <contract_id>`

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
