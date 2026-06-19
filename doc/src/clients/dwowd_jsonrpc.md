# dwowd JSON-RPC API
## Methods
<!-- toc -->
All methods follow the JSON-RPC 2.0 request/response format:

```json
// Request
{"jsonrpc":"2.0","method":"<name>","params":[...],"id":1}
// Response
{"jsonrpc":"2.0","result":<value>,"id":1}
```

Only the first method shows full JSON examples below.


## blockchain methods
### `blockchain.get_block` {#blockchainget_block}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L40)

Queries the blockchain database for a block in the given height.
Returns a readable block upon success.

**Params:**
* `array[0]`: `u32` block height

**Returns:**
* `BlockInfo` serialized into base64.

```rust,no_run,noplayground
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}
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

### `blockchain.get_tx` {#blockchainget_tx}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L88)

Queries the blockchain database for a given transaction.
Returns a base64 encoded `Transaction` object.

**Params:**
* `array[0]`: Hex-encoded transaction hash string

**Returns:**
* `Transaction` serialized into base64.

```rust,no_run,noplayground
{{#include ../../../src/tx/mod.rs:transaction-struct}}
```

### `blockchain.get_block_linear`

Queries the linear blockchain for a block at the given height.
Returns a `Block` serialized into base64. This endpoint only
works in linear blockchain mode (darkwow-devnet, darkwow-testnet, or the legacy-named linear-testnet).

**Params:**
* `array[0]`: `u32` block height

**Returns:**
* `Block` serialized into base64.

```rust,no_run,noplayground
{{#include ../../../src/linear/src/block.rs:block-struct}}
```

### `blockchain.get_difficulty` {#blockchainget_difficulty}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L134)

Queries the blockchain database to fetch the difficulty and cumulative
difficulty for a specific block height.

**Params:**
* `array[0]`: Block height

**Returns:**
* `difficulty`: Block difficulty as integer
* `cumulative_difficulty`: Cumulative block difficulty as integer

### `blockchain.last_confirmed_block` {#blockchainlast_confirmed_block}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L175)

Queries the blockchain database to find the last confirmed block.

**Params:**
* Empty

**Returns:**
* `f64`   : Height of the last confirmed block
* `String`: Header hash of the last confirmed block

### `blockchain.block_target` {#blockchainblock_target}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L240)

Queries the validator to get the currently configured block target time.

**Params:**
* Empty

**Returns:**
* `f64`: Current block target time

### `blockchain.subscribe_blocks` {#blockchainsubscribe_blocks}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L264)

Initializes a subscription to new incoming blocks.

Once a subscription is established, `dwowd` will send JSON-RPC notifications of
new incoming blocks to the subscriber.

The notifications contain base64-encoded `BlockInfo` structs.

```rust,no_run,noplayground
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}
```

### `blockchain.subscribe_txs` {#blockchainsubscribe_txs}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L289)

Initializes a subscription to new incoming transactions.

Once a subscription is established, `dwowd` will send JSON-RPC notifications of
new incoming transactions to the subscriber.

The notifications contain hex-encoded transaction hashes.

### `blockchain.subscribe_proposals` {#blockchainsubscribe_proposals}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L310)

Initializes a subscription to new incoming proposals. Once a subscription is established,
`dwowd` will send JSON-RPC notifications of new incoming proposals to the subscriber.

The notifications contain base64-encoded `BlockInfo` structs.

```rust,no_run,noplayground
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}
```

### `blockchain.lookup_zkas` {#blockchainlookup_zkas}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L333)

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

### `blockchain.lookup_wasm` {#blockchainlookup_wasm}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L406)

Perform a lookup of a WASM contract binary deployed on-chain and
return the base64-encoded binary.

**Params:**
* `array[0]`: base58-encoded contract ID string

**Returns:**
* `String`: base64-encoded WASM binary

### `blockchain.get_contract_state` {#blockchainget_contract_state}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L439)

Queries the blockchain database for a given contract state records.
Returns the records value raw bytes as a `BTreeMap`.

**Params:**
* `array[0]`: base58-encoded contract ID string
* `array[1]`: Contract tree name string

**Returns:**
* Records serialized `BTreeMap` encoded with base64

### `blockchain.get_contract_state_key` {#blockchainget_contract_state_key}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/blockchain.rs#L489)

Queries the blockchain database for a given contract state key raw bytes.
Returns the record value raw bytes.

**Params:**
* `array[0]`: base58-encoded contract ID string
* `array[1]`: Contract tree name string
* `array[2]`: Key raw bytes, encoded with base64

**Returns:**
* Record value raw bytes encoded with base64

## tx methods
### `tx.simulate` {#txsimulate}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L36)

Simulate a network state transition with the given transaction.
Returns `true` if the transaction is valid, otherwise, a corresponding
error.

### `tx.broadcast` {#txbroadcast}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L84)

Append a given transaction to the mempool and broadcast it to
the P2P network. The function will first simulate the state
transition in order to see if the transaction is actually valid,
and in turn it will return an error if this is the case.
Otherwise, a transaction ID will be returned.

### `tx.pending` {#txpending}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L140)

Queries the node pending transactions store to retrieve all transactions.
Returns a vector of hex-encoded transaction hashes.

### `tx.clean_pending` {#txclean_pending}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L174)

Queries the node pending transactions store to reset all
transactions. Unproposed transactions are removed.
Returns `true` if the operation was successful, otherwise, a
corresponding error.

### `tx.calculate_fee` {#txcalculate_fee}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/tx.rs#L208)

Compute provided transaction's total gas, against current best fork.
Returns the gas value if the transaction is valid, otherwise, a corresponding
error.

## stratum methods
### `login` {#login}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/stratum.rs#L71)

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

### `submit` {#submit}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/stratum.rs#L224)

Miner submits a job solution.

**Request:**
* `id`     : Registry client ID
* `job_id` : Registry mining job ID
* `nonce`  : The hex encoded solution header nonce.
* `result` : RandomX calculated hash

**Response:**
* `status`: Block submit status

### `keepalived` {#keepalived}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/stratum.rs#L365)

Miner sends `keepalived` to prevent connection timeout.

**Request:**
* `id` : Registry client ID

**Response:**
* `status`: Response status

## xmr methods
### `merge_mining_get_chain_id` {#merge_mining_get_chain_id}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/xmr.rs#L83)

Gets a unique ID that identifies this merge mined chain and
separates it from other chains.

* `chain_id`: A unique 32-byte hash that identifies this merge
mined chain.

dwowd will send the hash:
H(genesis_hash || network || hard_fork_height)

### `merge_mining_get_aux_block` {#merge_mining_get_aux_block}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/xmr.rs#L130)

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

### `merge_mining_submit_solution` {#merge_mining_submit_solution}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/xmr.rs#L254)

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

## misc methods
### `clock` {#clock}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/misc.rs#L29)

Returns current system clock as a UNIX timestamp.

## management methods
### `dnet.switch` {#dnetswitch}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/management.rs#L71)

Activate or deactivate dnet in the P2P stack.
By sending `true`, dnet will be activated, and by sending `false` dnet
will be deactivated.

Returns `true` on success.

### `dnet.subscribe_events` {#dnetsubscribe_events}

[source](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/dwowd/src/rpc/management.rs#L99)

Initializes a subscription to P2P dnet events.
Once a subscription is established, `dwowd` will send JSON-RPC
notifications of new network events to the subscriber.

