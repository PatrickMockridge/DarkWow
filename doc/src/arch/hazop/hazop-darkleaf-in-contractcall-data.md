# HAZOP: Preserving DarkLeaf Call Tree in ContractCall.data

**Date:** 2026-07-26
**Scope:** Change `protocol_tx.rs:132` from `data: leaf.data.data.clone()` to
serializing the full `DarkLeaf<ContractCall>` tree into `contract_calls[i].data`.

## 0. Current State (Baseline)

### 0.1 Data Flow

```
Wallet constructs: dwow_core::tx::Transaction {
    calls: Vec<DarkLeaf<ContractCall>>,   // call tree with parent/children
    proofs: Vec<Vec<Vec<u8>>>,            // ZK proofs per call
    signatures: Vec<Signature>,
    nullifiers: Vec<Nullifier>,
    tx_commitment: [u8; 32],
}
    │
    ▼ protocol_tx.rs:129-134  ── STRIPS to leaf.data.data.clone()
    │
ChainTransaction {
    contract_calls: Vec<ContractCall { contract_id, data: raw_payload }>,
    witness: serialize(&core_tx),  // full authenticated tx, opaque
}
    │
    ▼ execution.rs:222  ── call_data = call.data.clone()
    │
WASM receives ix: &[u8]  = raw_payload  = [func_selector][params...]
```

### 0.2 Dual Format Reality

There are already TWO byte-level formats in `contract_calls[i].data`:

| Contract Class | Format | Example |
|---------------|--------|---------|
| **Simple** (NativeToken, Identity, Deployooor, Darkbet, etc.) | `[func_selector: u8][params...]` | `[0x00][fee: u64 LE 8 bytes][FeeParamsV1]` |
| **Tree** (MultiSig, PromissoryNote, Bridge, Lottery) | `serialize(Vec<DarkLeaf<ContractCall>>)` | Deserialized by contract with `get_call_index()` |

The tree format is set by the wallet when it packs multiple related calls into a
single `ContractCall.data` field. The `if let Ok(...)` pattern at execution.rs:471
(Deployooor post-processing) already handles this duality — it tries the tree
deserialization and silently falls through for raw-payload calls.

### 0.3 What the Proposal Changes

Change `protocol_tx.rs:132` from:
```rust
data: leaf.data.data.clone(),  // raw inner payload only
```
to:
```rust
data: dwow_serial::serialize(&leaf),  // full DarkLeaf<ContractCall>
```

The new byte format for ALL calls would be a serialized `DarkLeaf<ContractCall>`:
```
[contract_id: 32 bytes] [data_len: varint] [data: variable] [parent_present: u8] [parent_index: varint?] [children_count: varint] [children: varint*]
```

## 1. Attack Vectors Introduced

### A1. Tree Structure Forgery (MEDIUM)

**What changes:** The WASM contract receives `parent_index` and `children_indexes`
from untrusted wallet input. Currently the contract has no way to access this
tree information. With it, a malicious wallet can fabricate arbitrary tree
relationships.

**Attack scenario:** Wallet constructs a tx where call[1] claims `parent_index =
Some(0)` pointing to call[0]. The contract reads call[0]'s state output and
assumes it was "authorized by" call[0]. But there is no cryptographic binding
between the tree indices and the actual authorization. The `parent_index` is
purely a data-structure hint, not a proof of authority.

**Mitigation defeat:** The ZK proof covers the inner call data, not the tree
indices. The DarkLeaf serialization is outside the ZK statement. A prover can
freely choose `parent_index` and `children_indexes` values.

**Requires:** Any contract consuming tree structure MUST independently verify
that the claimed relationships correspond to actual authority. The tree indices
are informational only and SHALL NOT be used for authorization decisions.

### A2. Information Side-Channel via Tree Topology (LOW)

**What changes:** The WASM contract can observe the full call topology within a
transaction.

**Attack scenario:** A contract could encode information in the tree structure
(e.g., parent_index values as ciphertext fragments) that bypasses the ZK
predicate. The ZK proof constrains the call data but not the tree indices.

**Mitigation:** Tree indices are small (usize/varint). Practical bandwidth for
covert channels is negligible (a few bytes per call).

### A3. Contract_id Desynchronization (HIGH)

**What changes:** The DarkLeaf serialization embeds `contract_id` inside
`data`. But `ContractCall.contract_id` is also a top-level field on the
chain-level struct. These two can diverge.

**Attack scenario:** A malicious full node modifies `contract_calls[i].contract_id`
(the chain-level field) but leaves the serialized DarkLeaf's inner
`contract_id` unchanged. The contract reads `ix[0..32]` as the contract_id
from the DarkLeaf, but the execution.rs code at line 191 reads
`call.contract_id` (the chain-level field) to look up the WASM binary.

**Impact:** The WASM loaded for execution is determined by `call.contract_id`.
The contract sees a different `contract_id` in its input data. This creates a
type-confusion scenario: contract A's WASM receives input claiming to be for
contract B.

**Mitigation requirement:** The contract SHALL verify that the received
`cid` parameter (passed by the host separately from `ix`) matches the
`contract_id` embedded in the DarkLeaf data. Currently most contracts ignore
`cid` entirely — they trust the host to have routed correctly.

### A4. Function Selector Obfuscation (HIGH)

**What changes:** The function selector is no longer at `data[0]`. It is
buried inside the DarkLeaf encoding at a variable offset.

**Attack scenario:** Consensus-level checks (Phase 0 block validation,
mempool fee extraction, coinbase detection) that inspect `data[0]` for the
function selector will see serialization framing bytes instead. A malicious
tx could craft a serialized DarkLeaf whose first byte accidentally matches
`0x00` (FeeV1) or `0x05` (PoWRewardV1), bypassing or confusing detection logic.

**Specific bypass vector:** If `data[0]` is `0x05`, the block validation code
at validation.rs:248-253 counts this as a PoWRewardV1 call, even if the
actual inner call is a completely different function. This creates
counterfeit coinbase detection.

**Requires:** All `data[0]` readers MUST be updated to extract the function
selector from the correct location in the serialized DarkLeaf. This is a
high-effort change touching 15+ files (see Section 4 below).

### A5. Tree Depth DOS (LOW-MEDIUM)

**What changes:** The WASM contract now deserializes the full DarkLeaf per call.

**Attack scenario:** A malicious tx includes extreme `children_indexes` vectors
with thousands of entries, consuming WASM gas in deserialization before any
business logic runs.

**Existing mitigation:** The WASM gas meter covers deserialization cost.
`children_indexes` is size-delimited by dwow_serial's varint encoding, so very
large vectors cost proportionally. But parent_index/children_indexes are
`Vec<usize>`, which could hold valid-looking garbage that inflates memory
usage without consuming more gas than a legitimate call.

### A6. Block Hash Non-Determinism Through Tree Structure (VERIFIED SAFE)

**What changes:** Including tree structure in `data` changes what goes into
`contract_calls[i].data`, which IS hashed in `Transaction::hash()`
(transaction.rs:328-329: `h.update(&call.data)`).

**Risk:** If two different tree serializations produce different blocks of
genuine calls, validators might diverge.

**Verdict: SAFE.** The witness carries the FULL authenticated tx, and
`contract_calls[i].data` is set deterministically from it. Two nodes
processing the same witness produce identical `data` bytes. The tree
structure is part of the authenticated payload (the witness) — it cannot
diverge between honest nodes.

## 2. Malicious WASM Contract Capabilities

### C1. Parent-Child Authority Inference (MEDIUM)

A WASM contract receiving the DarkLeaf tree can:
- See which calls in the transaction claim parent/child relationships
- Access sibling calls' data through the tree (by walking `children_indexes`)
- Infer transaction structure intent without cryptographic verification

**Currently impossible:** Contracts receive only their own call's inner
payload. They have zero visibility into other calls in the same transaction.

**New capability:** Contract A can observe that call[3] targets Contract B
and has `parent_index = Some(0)` pointing to call[0]. Contract A now knows
there is a composed operation spanning A and B, even though A never
authorized this observation.

**O-Cap violation:** Per type-system.md §5: "A process SHALL perform action A
if and only if it possesses the name for A." Observation of other calls'
structure is a new barb (`↓observe-tree-topology`) that the contract has no
capability to exercise. It gains ambient authority over tree topology
visibility.

### C2. Cross-Contract Data Leakage via Tree Walking (MEDIUM-HIGH)

A WASM contract can parse `children_indexes` to discover what OTHER contracts
are being called in the same transaction, with what function selectors.

**Example:** A DEX contract executing a swap observes that call[2] (child of
call[0]) targets Identity contract with selector `0x01` (AttestV1). The DEX
now knows an identity attestation is happening in the same tx as the swap,
linking the swap to a specific identity operation — even though the DEX has
no business purpose for this information.

**Currently impossible:** Contract A's `ix` contains only Contract A's call
data. It cannot enumerate the transaction's other calls.

**O-Cap violation:** Cross-contract information flow without capability
passing. Per ocap.md: capabilities MUST be explicitly passed. The tree
structure is ambient, not capability-mediated.

### C3. Tree-Structure Replay (LOW-MEDIUM)

A WASM contract could use `parent_index` and `children_indexes` as storage
keys, creating state that depends on the tree structure rather than the
authenticated call data.

**Impact:** If nullifier derivation does not cover the tree indices, the same
call data with different tree topology could produce different state
transitions, bypassing nullifier-based replay protection.

**Mitigation:** Nullifiers MUST be derived from the inner call data, not the
tree structure. This is already the case — nullifiers are in
`core_tx.nullifiers`, which are set by the wallet and verified by the ZK proof.

## 3. Invariant Analysis

### 3.1 Type-System Barbs (type-system.md §5, §10.5)

| Invariant | Status | Detail |
|-----------|--------|--------|
| "A process SHALL perform action A iff it possesses the name for A" (§5) | **WEAKENED** | Tree topology becomes ambient authority. A contract observes sibling calls without possessing their capabilities. |
| Re-lift validation (§10.5 obligation 1) | **NEW BURDEN** | The data field format changes. Every boundary that reads `data` must re-lift through the new DarkLeaf deserializer. |
| Channel boundary as barb absorber (§10.5) | **SHIFTED** | The `contract_calls[i].data` boundary now absorbs a larger barb set. Previously it was `{↓dispatch}`; now it also absorbs `{↓observe-tree, ↓cross-contract-peek}`. |
| Bytes round-trip forbidden (§2.2) | **RISK** | The contract_id appears twice: in the chain-level field AND inside the serialized DarkLeaf. This creates a round-trip opportunity that must be validated. |

### 3.2 O-Cap Authority Model (ocap.md)

| Invariant | Status | Detail |
|-----------|--------|--------|
| Capability IS a name possession (§0) | **PRESERVED** | ZK proofs still prove possession. Tree indices do not confer authority. |
| Minimal disclosure (§2, item 4) | **VIOLATED** | The verifier (contract WASM) now observes the caller's tree topology — information beyond the predicate result. |
| Cross-contract composition barbs (§7 of contract-wasm-type-system.md) | **NEW SURFACE** | The composite barb set `B_A ∪ B_B` now includes tree topology observation in both contracts' barb sets. This was never declared. |

### 3.3 Contract-WASM Type System (contract-wasm-type-system.md §1.3, §7)

| Claim | Status | Detail |
|-------|--------|--------|
| "payload begins with the function selector byte" (§1.3 line 135) | **BREAKS** | The function selector is no longer at offset 0. It's inside the DarkLeaf.contract_call.data[0]. |
| "contract receives the complete ContractCall.data as ix" (§1.3 line 136) | **CHANGES SEMANTICS** | `ix` is now the serialized DarkLeaf, not the ContractCall's inner data. |
| Composition rule (§7.1) | **PRESERVED** | Barb union still holds, but the barb set is now larger. |
| Inter-contract state isolation (§7.3) | **WEAKENED** | Contracts can observe sibling calls' contract_ids and function selectors through tree indices, even without state access. Information isolation is breached. |

### 3.4 Witness Binding (contract-wasm-type-system.md §6)

| Invariant | Status | Detail |
|-----------|--------|--------|
| Witness binding rule (§6.2) | **PRESERVED** | The witness still carries the full authenticated tx. Tree structure in `data` does not weaken the ZK proof binding. |
| Witness type checking (§6.3) | **UNAFFECTED** | The witness is opaque to the chain layer. Only the verifier (L2) inspects it. |

## 4. Code Paths Reading `data[0]` Directly

Every one of these breaks if `data` changes from raw payload to serialized DarkLeaf.
The function selector (`data[0]`) moves from offset 0 to a variable offset inside
the DarkLeaf encoding.

### 4.1 Consensus-Critical Paths (BLOCK REJECTION if broken)

| File | Line(s) | What it reads | Impact |
|------|---------|--------------|--------|
| `src/linear/src/validation.rs` | 248-258 | Coinbase detection (`data[0] == 0x05`) | Block structural validation. Wrong detection = block rejected or coinbase bypass |
| `src/linear/src/validation.rs` | 315-352 | FeeV1/FeeCollectV1 selectors (`0x00`, `0x06`) | Fee accounting. Block rejected if mismatched |
| `src/linear/src/execution.rs` | 490-493 | Coinbase detection for L2 witness skip | Coinbase txs get ZK verification bypass. Wrong skip = valid tx rejected or forged coinbase accepted |
| `src/linear/src/execution.rs` | 765-772 | DeployV1 selector (`0x00`) for genesis detection | Genesis block setup. Wrong detection = genesis fails or deploys phantom contracts |
| `src/linear/src/execution.rs` | 474 | Deployooor inner data (`inner.data[0] == 0x00`) | Deployooor post-processing. Prevents new contract registration |
| `bin/dwowd/src/block_acceptor.rs` | 166-168 | Coinbase detection | L2 witness verification bypass. Same risk as execution.rs |
| `bin/dwowd/src/proto/protocol_tx.rs` | 155-158 | Coinbase detection (mempool admission) | Mem pool rejects or passes coinbase txs incorrectly |
| `bin/dwowd/src/rpc/tx.rs` | 86-90 | Coinbase detection (RPC submission) | Users can submit coinbase txs via RPC (currently blocked) |

### 4.2 Economic Security Paths

| File | Line(s) | What it reads | Impact |
|------|---------|--------------|--------|
| `bin/dwowd/src/lib.rs` | 95 | FeeV1 selector (`0x00`) in `NativeTokenFeeExtractor` | Miners underpaid or overpaid. Fee minimum bypass |
| `crates/dwow-mempool/src/lib.rs` | 259-261 | Coinbase detection for fee minimum bypass | Mempool admits zero-fee txs |
| `src/linear/src/proof_of_token_balance.rs` | 110 | Function selector for mass-balance routing | Token supply audit breaks — inflation/negative supply undetected |

### 4.3 ZK Verification Paths

| File | Line(s) | What it reads | Impact |
|------|---------|--------------|--------|
| `src/linear/src/zk_verifier.rs` | 290-292 | Native token proof-requiring selectors (`0x00`, `0x02`, `0x03`, `0x04`, `0x06`) | Proof-requiring calls admitted without proofs |

### 4.4 WASM Contract Paths (15+ contracts)

| Contract | Entrypoint | Pattern | Impact |
|----------|-----------|---------|--------|
| All contracts | `process_instruction` | `XxxFunction::try_from(self_.data[0])` | Function dispatch broken. All calls fail |
| Bridge | `entrypoint.rs:239,341` | `BridgeFunction::try_from(self_.data[0])` | Bridge operations fail |
| Bridge | `entrypoint.rs:379,767,895,992` | `child_call.data[0] != 0x04` | Cross-contract validation broken |
| PromissoryNote | `entrypoint/mod.rs:249,435` | `PromissoryNoteFunction::try_from(self_.data[0])` | Token operations fail |
| LaborMarket | `entrypoint.rs:175+` | Function dispatch + child call checks | All labor market operations fail |
| Darkbet | `entrypoint.rs:143,250` | `DarkbetFunction::try_from(self_.data[0])` | Betting operations fail |
| Identity | `entrypoint.rs:128,219` | `IdentityFunction::try_from(self_.data[0])` | Identity operations fail |
| MultiSig | `entrypoint/mod.rs:56,115` | `MultiSigFunction::try_from(self_.data[0])` + `self_.data.data[0]` | MultiSig broken at both levels |
| Lottery | `entrypoint.rs:86,138` | `LotteryFunction::try_from(self_.data[0])` | Lottery operations fail |

### 4.5 SDK / Utility Paths

| File | Line(s) | What it reads | Impact |
|------|---------|--------------|--------|
| `src/sdk/src/tx.rs` | 109 | `ContractCall::matches_contract_call_type` | Wallet-side function detection broken |
| `src/sdk/src/crypto/transition_payload.rs` | 51 | `decode_payload` function code check | Generic payload decoding fails |
| `src/sdk/python/src/contract/mod.rs` | 128 | Python FFI `self.0.data[0]` | Python SDK broken |
| `script/research/tx-replayer/src/main.rs` | 258 | `call.data.data[0]` for state update prefix | Research tool broken |

**Total: ~50 code locations across ~20 files** that directly access `data[0]` as a
function selector and would produce incorrect results.

### 4.6 Indirect `data[0]` Access via Convenience Methods

The SDK's `dwow_sdk::tx::ContractCall` (NOT the chain-level one) has:
```rust
pub fn matches_contract_call_type(&self, contract_id: ContractId, func_code: u8) -> bool {
    !self.data.is_empty() && self.contract_id == contract_id && self.data[0] == func_code
}
```

And its callers:
- `is_deployment()` → checks `DEPLOYOOOR_CONTRACT_ID` + `0x00`
- `is_native_token_fee()` → checks `NATIVE_TOKEN_CONTRACT_ID` + `0x00`
- `is_native_token_pow_reward()` → checks `NATIVE_TOKEN_CONTRACT_ID` + `0x05`

These methods are used by the wallet, test harness, and bridge client. All
would silently return `false` for valid calls after the format change.

## 5. Redundancy Analysis: Tree in Both `data` and `witness`

### 5.1 What's Redundant

| Information | In `contract_calls[i].data` (proposed) | In `witness` |
|-------------|--------------------------------------|-------------|
| ContractId | Inside DarkLeaf.contract_call.contract_id | Inside core_tx.calls[i].data.contract_id |
| Function selector | Inside DarkLeaf.contract_call.data[0] | Inside core_tx.calls[i].data.data[0] |
| Call params | Inside DarkLeaf.contract_call.data[1..] | Inside core_tx.calls[i].data.data[1..] |
| parent_index | DarkLeaf.parent_index | core_tx.calls[i].parent_index |
| children_indexes | DarkLeaf.children_indexes | core_tx.calls[i].children_indexes |
| ZK proofs | NOT in data | core_tx.proofs |
| Signatures | NOT in data | core_tx.signatures |

### 5.2 Divergence Risk

The same information stored in two places that are NOT cryptographically bound
to each other creates a divergence surface:

**Scenario 1 — Malicious full node:**
- Takes a valid witness W
- Modifies `contract_calls[i].data` (the proposed DarkLeaf serialization)
- Leaves witness unchanged
- Block hash changes (data is hashed)
- But L2 verification passes (witness is unmodified)

This creates a fork: honest validators include the witness's tree structure,
malicious ones include a modified tree structure. Both pass L2 verification
because the witness is authoritative and unmodified.

**Scenario 2 — Honest node receiving spoofed chain tx:**
- Receives chain tx with `contract_calls[i].data = fabricated DarkLeaf`
- Verifies witness against `contract_calls[i]` — witness has REAL DarkLeaf
- Rejects if they diverge, BUT:
  - zk_verifier.rs `decode_and_reconcile` only checks call count and
    per-call data, not tree indices (line 276-280 checks proofs.len() vs
    calls.len(); the reconciliation at line 338+ compares inner data only)
  - The reconciliation does NOT compare tree indices

**Gap:** If `decode_and_reconcile` does not verify tree indices, a malicious
node can inject fabricated parent/children relationships into
`contract_calls[i].data` that differ from the witness, and the reconciliation
still passes.

### 5.3 Storage Bloat

Each `contract_calls[i].data` grows from `~1+N bytes` (selector + params) to
`~40+N bytes` (32-byte contract_id + framing + selector + params).
Across a block with thousands of calls, this adds up. For tree contracts
that already serialize `Vec<DarkLeaf<ContractCall>>` into data, the bloat is
compounded: `data = serialize(Vec<serialize(DarkLeaf)>)`.

### 5.4 Redundancy Verdict

**HIGH risk.** The redundancy creates a new reconciliation obligation between
`contract_calls[i].data` and the witness that does not exist in the current
reconciliation logic. If this obligation is missed, the chain accepts valid
witnesses with fabricated tree structures in the hash-committed data — a
consensus fork vector.

## 6. Simplest, Safest Implementation

### 6.1 Do NOT Change the `data` Field Format

**Recommendation: REJECT the proposal as stated.** Changing the byte-level
format of `contract_calls[i].data` from raw payload to serialized DarkLeaf
breaks ~50 code locations across the consensus, economic, ZK verification,
and WASM contract layers. The blast radius is essentially the entire
transaction processing pipeline.

### 6.2 Alternative A: Add a Separate Field (Recommended)

Add a new optional field to `ChainTransaction` or `ContractCall`:

```rust
pub struct ContractCall {
    pub contract_id: ContractId,
    pub data: Vec<u8>,              // unchanged — raw payload
    pub tree_hint: Option<TreeHint>, // new field, excluded from hash or
                                     // verified against witness
}

pub struct TreeHint {
    pub parent_index: Option<usize>,
    pub children_indexes: Vec<usize>,
}
```

**Pros:**
- Zero breakage to existing `data[0]` readers
- Zero breakage to existing WASM contracts (they ignore unknown fields)
- Tree information is explicit and typed, not buried in opaque serialization
- Can be excluded from block hash (no determinism risk) or verified against witness
- Easy to add `wasm::host::get_tree_hint(call_idx)` to expose to WASM

**Cons:**
- New field on the chain-level struct (but it's small — 16 bytes typical)
- Witness reconciliation must verify tree_hint matches witness tree indices

### 6.3 Alternative B: Host Function (Most Conservative)

Instead of putting tree structure in data at all, expose it through a WASM
host function:

```rust
// In src/runtime/vm_runtime.rs
fn get_call_parent_index(&self, call_idx: u32) -> Option<u32>;
fn get_call_children_indexes(&self, call_idx: u32) -> Vec<u32>;
```

These load the tree structure from the witness (already available in the
host's Runtime context). The host function is only available to contracts
that declare a tree-aware barb in their manifest.

**Pros:**
- Zero change to chain-level data structures
- Zero change to `data` format
- Zero breakage to any existing code
- Authority model preserved: tree access requires declared barb

**Cons:**
- Requires extending the WASM host interface
- Contracts must opt in via manifest

### 6.4 Alternative C: Encode in Apply Data (Tree Contracts Only)

For the tree-structured contracts that already receive `Vec<DarkLeaf<ContractCall>>`
in their `data` field, no change is needed — they already have the tree.
For other contracts, if tree information is needed, the wallet can include it
inside the existing `data` payload as an optional suffix:

```
[func_selector][params...][optional_tree_hint_present: u8][tree_hint if present]
```

**Pros:**
- No chain-level format change
- Backward compatible (old contracts ignore the suffix)
- Per-contract opt-in

**Cons:**
- Ad-hoc encoding per contract
- Not standardized

## 7. Summary of Recommendations

### Immediate

1. **REJECT the proposal to serialize DarkLeaf into `contract_calls[i].data`**
   as the universal format. The blast radius is too large (~50 code locations)
   and the security risks (A3, A4, C2) are HIGH.

2. **Prefer Alternative B (host function)** for exposing tree structure to
   WASM contracts. This is the safest approach — zero changes to chain-level
   data structures, zero breakage, and authority is explicitly gated through
   the manifest barb system.

3. **If Alternative B is insufficient**, implement Alternative A (separate
   `tree_hint` field) with witness reconciliation enforcement. This is a
   smaller blast radius than changing `data` format.

### If the Original Proposal is Required (Override)

The following MUST be addressed before merging:

1. **Function selector extraction function.** Create a single canonical
   function `fn extract_function_selector(data: &[u8]) -> Option<u8>` that
   handles both old (raw payload) and new (serialized DarkLeaf) formats.
   Replace ALL ~50 `data[0]` accesses with calls to this function.

2. **Witness reconciliation extension.** Extend `decode_and_reconcile` to
   verify tree indices match between the witness and `contract_calls[i].data`.

3. **Contract_id validation in WASM.** Add a validation step in
   `process_instruction` that asserts `cid == leaf.data.contract_id`.

4. **Spec update.** Update contract-wasm-type-system.md §1.3 to document the
   new data format consensus rule.

5. **Rebuild all contract WASM binaries.** Every contract's WASM must be
   rebuilt and the bytecode hashes updated in genesis.

6. **Full regression test.** The change is consensus-critical — a single
   missed `data[0]` reader causes silent incorrect behavior (not a panic).

## Appendix: Complete `data[0]` Reader Inventory

See Section 4 above for the categorized inventory. The raw grep count is
~100+ matches across ~20 files, of which ~50 are consensus-critical or
economic-security-critical reads that would produce incorrect behavior
(not just panics) after the format change.
