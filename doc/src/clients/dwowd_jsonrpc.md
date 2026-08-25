# dwowd JSON-RPC API

dwowd exposes a JSON-RPC 2.0 API over HTTP. All methods use `POST` with
`Content-Type: application/json`. The default RPC port depends on network:

| Network | RPC Port |
|---------|----------|
| `darkwow-devnet` | 28345 |
| `darkwow-testnet` | 31345 |

Requests follow standard JSON-RPC 2.0 format:

```json
{"jsonrpc": "2.0", "method": "<method>", "params": [...], "id": 1}
```

---

## Blockchain Methods

### blockchain.get_target

Returns the current proof-of-work target.

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.get_target", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": 4294967295, "id": 1}
```

### blockchain.get_height

Returns the current blockchain height (number of mined blocks).

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.get_height", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": 15234, "id": 1}
```

### blockchain.last_confirmed_block

Returns the most recently confirmed block hash and header data.

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.last_confirmed_block", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"hash": "...", "header": {...}}, "id": 1}
```

### blockchain.get_block_linear

Returns a block by height in the linear (Uncle Merkle) chain.

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.get_block_linear", "params": [15200], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"block": "base64..."}, "id": 1}
```

### blockchain.get_contract_state_linear

Returns the state tree for a contract at a given block height.

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.get_contract_state_linear", "params": ["contract_id_hex", 15200], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"state": [...]}, "id": 1}
```

### blockchain.get_cumulative_supply

Returns the cumulative commitment supply at the current height. Used for supply audit.

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.get_cumulative_supply", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"supply": 10500000000000}, "id": 1}
```

### blockchain.subscribe_blocks

Subscribes to new block notifications via pub-sub. Notifications are sent
as JSON-RPC notifications (no `id` field).

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_blocks", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
// ... later, as blocks arrive:
// <-- {"jsonrpc": "2.0", "method": "blockchain.new_block", "params": {"block": "base64..."}}
```

### blockchain.lookup_zkas

Looks up the ZKAS binary for a given contract and circuit namespace.

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.lookup_zkas", "params": ["contract_id_hex", "native_token"], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"zkbin": "base64..."}, "id": 1}
```

### blockchain.get_tx

Returns transaction data by transaction hash.

```json
// --> {"jsonrpc": "2.0", "method": "blockchain.get_tx", "params": ["tx_hash_hex"], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"tx": "base64..."}, "id": 1}
```

---

## Transaction Methods

### tx.submit_linear

Submits a linear-chain transaction for inclusion in the next block.

```json
// --> {"jsonrpc": "2.0", "method": "tx.submit_linear", "params": ["base64encodedTX"], "id": 1}
// <-- {"jsonrpc": "2.0", "result": "txHash...", "id": 1}
```

### tx.simulate

Simulates a transaction against the current chain state. Returns `true` if
all ZK proofs verify and state transitions are valid. The transaction is not
included in a block.

```json
// --> {"jsonrpc": "2.0", "method": "tx.simulate", "params": ["base64encodedTX"], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
```

### tx.calculate_fee

Calculates the recommended transaction fee based on recent block fee data.

```json
// --> {"jsonrpc": "2.0", "method": "tx.calculate_fee", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"fee": 42000000, "utilization": 0.35, "blocks_sampled": 12}, "id": 1}
```

---

## Contract Methods

### contract.invoke

Invokes a smart contract function.

```json
// --> {"jsonrpc": "2.0", "method": "contract.invoke", "params": [{"contract": "contract_id_hex", "func_code": 3, "data": "base64..."}], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"output": "base64..."}, "id": 1}
```

### contract.deploy

Deploys a new smart contract. Available only in `darkwow-devnet` mode.
Requires the Deployooor contract to be present in genesis.

```json
// --> {"jsonrpc": "2.0", "method": "contract.deploy", "params": [{"wasm": "base64..."}], "id": 1}
// <-- {"jsonrpc": "2.0", "result": {"status": "deployed"}, "id": 1}
```

---

## Miner Methods

### miner.mine_linear

Mines a block using the linear (Uncle Merkle) consensus. Primarily used for
local devnet testing. On public testnet, mining is handled via the Stratum
protocol (see [Stratum Protocol](../arch/consensus/stratum.md)).

```json
// --> {"jsonrpc": "2.0", "method": "miner.mine_linear", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": "blockHash...", "id": 1}
```

---

## Miscellaneous Methods

### ping

Health check. Returns `"pong"`.

```json
// --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": "pong", "id": 1}
```

### clock

Returns the node's wall clock time.

```json
// --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": 1720000000, "id": 1}
```

---

## Management API

The management API runs on a separate RPC port (default: 28346 for devnet,
31346 for testnet). These methods control the node's networking and accounts.

### dnet.switch

Enables or disables the P2P network.

```json
// --> {"jsonrpc": "2.0", "method": "dnet.switch", "params": [true], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
```

### dnet.subscribe_events

Subscribes to P2P network events (peer connections, disconnections, messages).

```json
// --> {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": true, "id": 1}
```

### accounts.show

Displays account information for all managed accounts.

```json
// --> {"jsonrpc": "2.0", "method": "accounts.show", "params": [], "id": 1}
// <-- {"jsonrpc": "2.0", "result": [{"name": "...", "address": "..."}], "id": 1}
```

## Error Codes

| Code | Meaning |
|------|---------|
| -32700 | Parse error |
| -32600 | Invalid request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |

## See Also

- [dwowd Daemon Documentation](../dwowd.md)
- [Stratum Protocol](../arch/consensus/stratum.md)
- [Testing Overview](../dev/testing/overview.md)
