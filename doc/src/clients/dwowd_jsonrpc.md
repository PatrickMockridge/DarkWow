# dwowd JSON-RPC API
## Methods
<!-- toc -->
## blockchain methods
### `blockchain.get_block` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L40">[source]</a></sup> {#blockchainget_block}

Queries the blockchain database for a block in the given height.
Returns a readable block upon success.

**Params:**
* `array[0]`: `u32` block height

**Returns:**
* `BlockInfo` serialized into base64.

```rust,no_run,noplayground
{{#include ../../../src/blockchain/block_store.rs:blockinfo}}
```

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.get_block",
  "params": [
    0
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": "base64encodedblock",
  "id": 1
}
```

### `blockchain.get_tx` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L88">[source]</a></sup> {#blockchainget_tx}

Queries the blockchain database for a given transaction.
Returns a base64 encoded `Transaction` object.

**Params:**
* `array[0]`: Hex-encoded transaction hash string

**Returns:**
* `Transaction serialized into base64.

```rust,no_run,noplayground
{{#include ../../../src/tx/mod.rs:transaction-struct}}
```

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.get_tx",
  "params": [
    "TxHash"
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": "base64encodedtx",
  "id": 1
}
```

### `blockchain.get_block_linear`

Queries the linear blockchain for a block at the given height.
Returns a `LinearBlockAdapter` serialized into base64. This endpoint only
works in linear-testnet mode.

**Params:**
* `array[0]`: `u32` block height

**Returns:**
* `LinearBlockAdapter` serialized into base64.

```rust,no_run,noplayground
{{#include ../../../../../bin/dwowd/src/rpc/blockchain.rs:linearblockadapter}}
```

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.get_block_linear",
  "params": [
    1
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": "base64encodedlinearlockadapter",
  "id": 1
}
```

### `blockchain.get_difficulty` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L134">[source]</a></sup> {#blockchainget_difficulty}

Queries the blockchain database to fetch the difficulty and cumulative
difficulty for a specific block height.

**Params:**
* `array[0]`: Block height

**Returns:**
* `difficulty`: Block difficulty as integer
* `cumulative_difficulty`: Cumulative block difficulty as integer

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.get_difficulty",
  "params": [
    1
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": [
    123,
    456
  ],
  "id": 1
}
```

### `blockchain.last_confirmed_block` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L175">[source]</a></sup> {#blockchainlast_confirmed_block}

Queries the blockchain database to find the last confirmed block.

**Params:**
* Empty

**Returns:**
* `f64`   : Height of the last confirmed block
* `String`: Header hash of the last confirmed block

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.last_confirmed_block",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": [
    1234,
    "HeaderHash"
  ],
  "id": 1
}
```

### `blockchain.best_fork_next_block_height` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L209">[source]</a></sup> {#blockchainbest_fork_next_block_height}

Queries the validator to find the current best fork next block height.

**Params:**
* Empty

**Returns:**
* `f64`: Current best fork next block height

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.best_fork_next_block_height",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": 1234,
  "id": 1
}
```

### `blockchain.block_target` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L240">[source]</a></sup> {#blockchainblock_target}

Queries the validator to get the currently configured block target time.

**Params:**
* Empty

**Returns:**
* `f64`: Current block target time

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.block_target",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": 120,
  "id": 1
}
```

### `blockchain.subscribe_blocks` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L264">[source]</a></sup> {#blockchainsubscribe_blocks}

Initializes a subscription to new incoming blocks.

Once a subscription is established, `dwowd` will send JSON-RPC notifications of
new incoming blocks to the subscriber.

The notifications contain base64-encoded `BlockInfo` structs.

```rust,no_run,noplayground
{{#include ../../../src/blockchain/block_store.rs:blockinfo}}
```

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.subscribe_blocks",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.subscribe_blocks",
  "params": [
    "base64encodedblock"
  ]
}
```

### `blockchain.subscribe_txs` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L289">[source]</a></sup> {#blockchainsubscribe_txs}

Initializes a subscription to new incoming transactions.

Once a subscription is established, `dwowd` will send JSON-RPC notifications of
new incoming transactions to the subscriber.

The notifications contain hex-encoded transaction hashes.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.subscribe_txs",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.subscribe_txs",
  "params": [
    "tx_hash"
  ]
}
```

### `blockchain.subscribe_proposals` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L310">[source]</a></sup> {#blockchainsubscribe_proposals}

Initializes a subscription to new incoming proposals. Once a subscription is established,
`dwowd` will send JSON-RPC notifications of new incoming proposals to the subscriber.

The notifications contain base64-encoded `BlockInfo` structs.

```rust,no_run,noplayground
{{#include ../../../src/blockchain/block_store.rs:blockinfo}}
```

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.subscribe_proposals",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.subscribe_proposals",
  "params": [
    "base64encodedblock"
  ]
}
```

### `blockchain.lookup_zkas` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L333">[source]</a></sup> {#blockchainlookup_zkas}

Performs a lookup of zkas bincodes for a given contract ID and returns all of
them, including their namespace.

**Params:**
* `array[0]`: base58-encoded contract ID string

**Returns:**
* `array[n]`: Pairs of: `zkas_namespace` strings and base64-encoded
`ZkBinary` objects.

```rust,no_run,noplayground
{{#include ../../../src/zkas/decoder.rs:zkbinary-struct}}
```

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.lookup_zkas",
  "params": [
    "DgmXpuU1EcM54E8GuNTAkBUThcCoYzGN5kRCNXA4cPtw"
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": [
    [
      "Foo",
      "ABCD..."
    ],
    [
      "Bar",
      "EFGH..."
    ]
  ],
  "id": 1
}
```

### `blockchain.lookup_wasm` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L406">[source]</a></sup> {#blockchainlookup_wasm}

Perform a lookup of a WASM contract binary deployed on-chain and
return the base64-encoded binary.

**Params:**
* `array[0]`: base58-encoded contract ID string

**Returns:**
* `String`: base64-encoded WASM binary

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.lookup_wasm",
  "params": [
    "DgmXpuU1EcM54E8GuNTAkBUThcCoYzGN5kRCNXA4cPtw"
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": "ABCD...",
  "id": 1
}
```

### `blockchain.get_contract_state` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L439">[source]</a></sup> {#blockchainget_contract_state}

Queries the blockchain database for a given contract state records.
Returns the records value raw bytes as a `BTreeMap`.

**Params:**
* `array[0]`: base58-encoded contract ID string
* `array[1]`: Contract tree name string

**Returns:**
* Records serialized `BTreeMap` encoded with base64

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.get_contract_state",
  "params": [
    "BZHK...",
    "tree"
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": "ABCD...",
  "id": 1
}
```

### `blockchain.get_contract_state_key` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L489">[source]</a></sup> {#blockchainget_contract_state_key}

Queries the blockchain database for a given contract state key raw bytes.
Returns the record value raw bytes.

**Params:**
* `array[0]`: base58-encoded contract ID string
* `array[1]`: Contract tree name string
* `array[2]`: Key raw bytes, encoded with base64

**Returns:**
* Record value raw bytes encoded with base64

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "blockchain.get_contract_state_key",
  "params": [
    "BZHK...",
    "tree",
    "ABCD..."
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": "ABCD...",
  "id": 1
}
```

## tx methods
### `tx.simulate` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L36">[source]</a></sup> {#txsimulate}

Simulate a network state transition with the given transaction.
Returns `true` if the transaction is valid, otherwise, a corresponding
error.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "tx.simulate",
  "params": [
    "base64encodedTX"
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": true,
  "id": 1
}
```

### `tx.broadcast` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L84">[source]</a></sup> {#txbroadcast}

Append a given transaction to the mempool and broadcast it to
the P2P network. The function will first simulate the state
transition in order to see if the transaction is actually valid,
and in turn it will return an error if this is the case.
Otherwise, a transaction ID will be returned.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "tx.broadcast",
  "params": [
    "base64encodedTX"
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": "txID...",
  "id": 1
}
```

### `tx.pending` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L140">[source]</a></sup> {#txpending}

Queries the node pending transactions store to retrieve all transactions.
Returns a vector of hex-encoded transaction hashes.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "tx.pending",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": [
    "TxHash",
    "..."
  ],
  "id": 1
}
```

### `tx.clean_pending` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L174">[source]</a></sup> {#txclean_pending}

Queries the node pending transactions store to reset all
transactions. Unproposed transactions are removed.
Returns `true` if the operation was successful, otherwise, a
corresponding error.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "tx.clean_pending",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": true,
  "id": 1
}
```

### `tx.calculate_fee` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L208">[source]</a></sup> {#txcalculate_fee}

Compute provided transaction's total gas, against current best fork.
Returns the gas value if the transaction is valid, otherwise, a corresponding
error.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "tx.calculate_fee",
  "params": [
    "base64encodedTX",
    "include_fee"
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": true,
  "id": 1
}
```

## stratum methods
### `login` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/stratum.rs#L71">[source]</a></sup> {#login}

Register a new mining client to the registry and generate a new
job.

**Request:**
* `login` : A wallet address or its base-64 encoded mining configuration
* `pass`  : Unused client password field
* `agent` : Client agent description
* `algo`  : Client supported mining algorithms

**Response:**
* `id`     : Registry client ID
* `job`    : The generated mining job
* `status` : Response status

The generated mining job map consists of the following fields:
* `blob`      : The hex encoded block hashing blob of the job block
* `job_id`    : Registry mining job ID
* `height`    : The job block height
* `target`    : Current mining target
* `algo`      : The mining algorithm - RandomX
* `seed_hash` : Current RandomX key
* `next_seed_hash`: (optional) Next RandomX key if it is known

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "login",
  "params": {
    "login": "WALLET_ADDRESS",
    "pass": "x",
    "agent": "XMRig",
    "algo": [
      "rx/0"
    ]
  },
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "id": "unique_connection-id",
    "job": {
      "blob": "abcdef...001234",
      "job_id": "unique_job-id",
      "height": 1234,
      "target": "abcd1234",
      "algo": "rx/0",
      "seed_hash": "deadbeef...0234",
      "next_seed_hash": "c0fefe...1243"
    },
    "status": "OK"
  },
  "id": 1
}
```

### `submit` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/stratum.rs#L224">[source]</a></sup> {#submit}

Miner submits a job solution.

**Request:**
* `id`     : Registry client ID
* `job_id` : Registry mining job ID
* `nonce`  : The hex encoded solution header nonce.
* `result` : RandomX calculated hash

**Response:**
* `status`: Block submit status

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "submit",
  "params": {
    "id": "unique_connection-id",
    "job_id": "unique_job-id",
    "nonce": "d0030040",
    "result": "e1364b8782719d7683e2ccd3d8f724bc59dfa780a9e960e7c0e0046acdb40100"
  },
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "OK"
  },
  "id": 1
}
```

### `keepalived` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/stratum.rs#L365">[source]</a></sup> {#keepalived}

Miner sends `keepalived` to prevent connection timeout.

**Request:**
* `id` : Registry client ID

**Response:**
* `status`: Response status

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "keepalived",
  "params": {
    "id": "foo"
  },
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "KEEPALIVED"
  },
  "id": 1
}
```

## xmr methods
### `merge_mining_get_chain_id` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/xmr.rs#L83">[source]</a></sup> {#merge_mining_get_chain_id}

Gets a unique ID that identifies this merge mined chain and
separates it from other chains.

* `chain_id`: A unique 32-byte hash that identifies this merge
mined chain.

dwowd will send the hash:
H(genesis_hash || network || hard_fork_height)

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "merge_mining_get_chain_id",
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "chain_id": "0f28c...7863"
  },
  "id": 1
}
```

### `merge_mining_get_aux_block` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/xmr.rs#L130">[source]</a></sup> {#merge_mining_get_aux_block}

Gets a blob of data, the blocks hash and difficutly used for
merge mining.

**Request:**
* `address` : A wallet address or its base-64 encoded mining configuration on the merge mined chain
* `aux_hash`: Merge mining job that is currently being polled
* `height`  : Monero height
* `prev_id` : Hash of the previous Monero block

**Response:**
* `aux_blob`: A hex-encoded blob of empty data
* `aux_diff`: Mining difficulty (decimal number)
* `aux_hash`: A 32-byte hex-encoded hash of merge mined block

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "merge_mining_get_aux_block",
  "params": {
    "address": "MERGE_MINED_CHAIN_ADDRESS",
    "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f",
    "height": 3000000,
    "prev_id": "ad505b0be8a49b89273e307106fa42133cbd804456724c5e7635bd953215d92a"
  },
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "aux_blob": "fad344115...3151531",
    "aux_diff": 123456,
    "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f"
  },
  "id": 1
}
```

### `merge_mining_submit_solution` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/xmr.rs#L254">[source]</a></sup> {#merge_mining_submit_solution}

Submits a PoW solution for the merge mined chain's block. Note that
when merge mining with Monero, the PoW solution is always a Monero
block template with merge mining data included into it.

**Request:**
* `aux_blob`: Blob of data returned by `merge_mining_get_aux_block`
* `aux_hash`: A 32-byte hex-encoded hash of merge mined block
* `blob`: Monero block template that has enough PoW to satisfy the difficulty
returned by `merge_mining_get_aux_block`. It must also have a merge mining
tag in `tx_extra` of the coinbase transaction.
* `merkle_proof`: A proof that `aux_hash` was included when calculating the
Merkle root hash from the merge mining tag
* `path`: A path bitmap (32-bit unsigned integer) that complements `merkle_proof`
* `seed_hash`: A 32-byte hex-encoded key that is used to initialize the
RandomX dataset

**Response:**
* `status`: Block submit status

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "merge_mining_submit_solution",
  "params": {
    "aux_blob": "124125....35215136",
    "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f",
    "blob": "...",
    "merkle_proof": [
      "hash1",
      "hash2",
      "hash3"
    ],
    "path": 3,
    "seed_hash": "22c3d47c595ae888b5d7fc304235f92f8854644d4fad38c5680a5d4a81009fcd"
  },
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "accepted"
  },
  "id": 1
}
```

## misc methods
### `clock` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/misc.rs#L29">[source]</a></sup> {#clock}

Returns current system clock as a UNIX timestamp.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "clock",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": 1767015913,
  "id": 1
}
```

## management methods
### `dnet.switch` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/management.rs#L71">[source]</a></sup> {#dnetswitch}

Activate or deactivate dnet in the P2P stack.
By sending `true`, dnet will be activated, and by sending `false` dnet
will be deactivated.

Returns `true` on success.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "dnet.switch",
  "params": [
    true
  ],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": true,
  "id": 1
}
```

### `dnet.subscribe_events` <sup><a href="https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/management.rs#L99">[source]</a></sup> {#dnetsubscribe_events}

Initializes a subscription to P2P dnet events.
Once a subscription is established, `dwowd` will send JSON-RPC
notifications of new network events to the subscriber.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "dnet.subscribe_events",
  "params": [],
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "method": "dnet.subscribe_events",
  "params": [
    {
      "chan": {
        "Channel": "Info"
      },
      "cmd": "command",
      "time": 1767016282
    }
  ]
}
```

