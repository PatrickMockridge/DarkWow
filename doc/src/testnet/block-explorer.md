# Block Explorer Guide

This guide explains how to query DarkWow nodes via JSON-RPC for block data,
uncle blocks, cumulative supply verification, and chain statistics. It serves
as a reference for anyone building a block explorer, exchange integration, or
analytics service.

## Quick Start

```bash
# Local dockernet
python3 bin/explorer/explorer.py height
python3 bin/explorer/explorer.py block 5
python3 bin/explorer/explorer.py scan 1 20

# Remote testnet (use a fullnode RPC, not a lilith seed — lilith is P2P-only)
python3 bin/explorer/explorer.py --host <testnet-fullnode> --port 31345 height
```

The explorer script (`bin/explorer/explorer.py`) uses only Python stdlib — no
dependencies. It works against any DarkWow node running the JSON-RPC server.

## JSON-RPC Endpoints

DarkWow nodes expose JSON-RPC on port 31345 (default). All methods take an
array of parameters and return a result object.

### `blockchain.get_height`

Returns the current canonical block height.

```json
{"jsonrpc":"2.0","method":"blockchain.get_height","params":[],"id":1}
→ {"result":{"height":42},"id":1}
```

### `blockchain.get_block_linear`

Returns a block at the given height, serialized as JSON.

```json
{"jsonrpc":"2.0","method":"blockchain.get_block_linear","params":[5],"id":1}
→ {"result":"{\"header\":{...},\"transactions\":[...]}","id":1}
```

The result is a JSON-encoded string containing the full block. Parse it with
`json.loads()` to access the header and transactions.

**Block header fields:**

| Field | Type | Description |
|-------|------|-------------|
| `version` | u8 | Block version |
| `height` | u64 | Block height |
| `previous` | [u8;32] | Previous block hash |
| `merkle_root` | [u8;32] | Transaction merkle root |
| `timestamp` | u64 | Unix timestamp |
| `target` | u32 | PoW target |
| `nonce` | u32 | PoW nonce |
| `uncle_merkle_root` | [u8;32] | Uncle merkle tree root |
| `total_reward` | u64 | Coinbase reward (base units) |
| `coin_merkle_root` | [u8;32] | Coin commitment tree root |
| `nullifier_root` | [u8;32] | Nullifier tree root |
| `finality_flags` | u8 | Anchor flags |
| `anchor_tx_id` | [u8;32] | Arweave anchor tx |
| `anchor_monero_height` | u64 | Monero anchor height |

### `blockchain.get_cumulative_supply`

Returns the Pedersen cumulative supply commitment chain state. See
[Supply Audit](../arch/consensus/consensus.md#supply-audit-pedersen-cumulative-commitment-chain).

```json
{"jsonrpc":"2.0","method":"blockchain.get_cumulative_supply","params":[],"id":1}
→ {"result":{"height":42,"total_supply":...,"cumulative_value_commit":"...","cumulative_blind":"..."},"id":1}
```

### `blockchain.get_target`

Returns the current PoW target.

```json
{"jsonrpc":"2.0","method":"blockchain.get_target","params":[],"id":1}
→ {"result":{"target":65535},"id":1}
```

## Denomination

All on-chain values are in **base units** (the smallest divisible unit).

```
1 DRKW = 100,000,000 base units (10^8)
1 base unit = 0.00000001 DRKW (10^-8)
```

The `total_reward` and `total_supply` fields in RPC responses are `u64` values
in base units. To convert to DRKW for display, divide by 100,000,000. The
emission schedule constants (`INITIAL_REWARD = 1_383_764_049`) are also in
base units (~13.84 DRKW per block).

## Understanding Uncle Blocks

### What Uncles Are

When two miners produce blocks at the same height, the first one received
becomes canonical and the second becomes an **uncle**. The next canonical
block includes a merkle root over the uncle's header, committing it to the
chain. The uncle miner receives a partial reward (50% at depth 1, halving
each depth thereafter).

### Detecting Uncles On-Chain

The `uncle_merkle_root` field in the block header tells you whether a block
includes uncles:

- `uncle_merkle_root == [0u8; 32]` → No uncles included
- `uncle_merkle_root != [0u8; 32]` → Uncles are included (the root is the
  merkle root of all included uncle headers)

Uncle blocks themselves are stored in the uncle sled tree and can be queried
via the block explorer for full verification.

### Why Early Blocks Have Zero Uncle Roots

Competeing blocks take time to form. In a fresh network with few miners:
- Blocks 1-5 typically have zero uncle roots (no competing blocks yet)
- Block 6+ may start showing non-zero roots as miners diverge
- With 5+ miners, uncle activity is common from block 8 onward

This matches the Python model's predictions exactly.

### Verifying Uncle Proofs

The `UncleProof` structure (see [uncle-merkle consensus](../arch/consensus/uncle_merkle.md))
allows stateless verification:

```
verify_uncle_proof(proof, uncle_merkle_root, target) → bool
```

This verifies:
1. The uncle's PoW hash is correct
2. The PoW meets the target
3. The merkle proof is valid against the block's `uncle_merkle_root`
4. The uncle is within the maximum depth (6)

## Cumulative Supply Verification

Any node can verify total supply against the emission schedule. The
`blockchain.get_cumulative_supply` RPC endpoint returns the cumulative
supply value computed by the node from the canonical chain.

The explorer's `supply` command verifies that the RPC's reported `total_supply`
matches the emission schedule — this confirms the node computed the expected
value. For full cryptographic Pedersen chain verification (recomputing every
`S_H = S_{H-1} + C_H` independently from block data, without trusting the
node's RPC response), use the Rust SDK's `verify_cumulative_supply()` function.

```python
from explorer import expected_cumulative_supply

# Query the node
supply = rpc.get_cumulative_supply()
height = supply["height"]
stored = supply["total_supply"]

# Verify against emission schedule
expected = expected_cumulative_supply(height)
assert stored == expected, f"Supply mismatch at height {height}"
```

See [Proof of Token Balance](../arch/consensus/consensus.md#supply-audit-capability)
for the two-layer defense architecture (active mass balance + cumulative supply chain).

## Reference Implementation

`bin/explorer/explorer.py` — A complete, dependency-free Python block explorer
using only `json`, `socket`, `struct`, and `argparse` from the standard library.
It demonstrates all the patterns described above: block retrieval, uncle detection,
supply audit, and chain scanning.

## See Also

- [Consensus: Supply Audit](../arch/consensus/consensus.md#supply-audit-pedersen-cumulative-commitment-chain)
- [Uncle-Merkle Consensus](../arch/consensus/uncle_merkle.md)
- [NativeToken Documentation](../dev/contracts/native_token.md)
- [Python Models](../testing/python-simulations.md)
