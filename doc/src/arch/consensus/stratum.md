# Stratum Protocol

DarkWow uses an **xmrig-compatible stratum protocol** over raw TCP for
external miner connections. The protocol enables CPU miners running xmrig
to connect to a `dwowd` node and mine blocks using RandomX.

Source: [`bin/dwowd/src/rpc/stratum.rs`](../../../bin/dwowd/src/rpc/stratum.rs).

References:
- [xmrig STRATUM.md](https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM.md)
- [xmrig STRATUM_EXT.md](https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM_EXT.md)

## Message Flow

```
Miner (xmrig)                           Daemon (dwowd)
     │                                       │
     │──── mining.login ────────────────────►│  wallet, agent, algo=["rx/0"]
     │                                       │  → generate block template
     │                                       │  → lazy-init ZK materials
     │                                       │  → return job + target
     │◄─── mining.set_target ───────────────│
     │     { id, job: { blob, job_id,        │
     │       target, seed_hash, height } }   │
     │                                       │
     │  [miner hashes blob with RandomX]     │
     │  [finds nonce where hash ≤ target]    │
     │                                       │
     │──── mining.submit ───────────────────►│  id, job_id, nonce, result
     │                                       │  → verify PoW
     │                                       │  → insert block
     │                                       │  → anchor (if Caribina enabled)
     │                                       │  → push new job to all miners
     │◄─── mining.set_target ───────────────│
     │     { status: "OK" }                  │
     │                                       │
```

After block submission, the daemon pushes a **new mining job** to all
connected stratum clients via a shared `Publisher`. Miners don't need to
re-login between blocks.

## Login

### Request

```json
{
    "jsonrpc": "2.0",
    "method": "login",
    "params": {
        "login": "<wallet_address>",
        "pass": "<any_string>",
        "agent": "xmrig/6.21.0",
        "algo": ["rx/0"]
    },
    "id": 1
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `login` | Yes | DarkWow testnet wallet address for receiving mining rewards |
| `pass` | Yes | Password field (xmrig protocol requirement, value ignored) |
| `agent` | Yes | Miner identifier string (logged for diagnostics) |
| `algo` | Yes | Must include `"rx/0"` — RandomX algorithm identifier |

The wallet address is parsed via `LinearMinerRewardsRecipientConfig::from_str()`
which validates the network prefix (must be testnet).

### Response

The response is a **raw JSON string** (not constructed via a JSON library)
to ensure precise integer formatting that xmrig's rapidjson parser accepts:

```json
{
    "jsonrpc": "2.0",
    "id": 1,
    "result": {
        "id": "<client_id>",
        "job": {
            "blob": "<227-byte hex-encoded mining blob>",
            "job_id": "linear-job-<height>",
            "target": "FFFFFFFF<target_hex>",
            "algo": "rx/0",
            "seed_hash": "<hex-encoded RandomX key>",
            "height": <u64>,
            "reserved_offset": 39
        },
        "status": "OK"
    },
    "error": null
}
```

| Field | Description |
|-------|-------------|
| `id` | Stratum client ID: `"{agent}-{timestamp_nanos}"` |
| `blob` | Hex-encoded 227-byte mining blob (header with nonce=0) |
| `job_id` | `"linear-job-{height}"` — must match in submit |
| `target` | Encoded difficulty target (see Target Encoding below) |
| `algo` | Always `"rx/0"` |
| `seed_hash` | Hex-encoded 32-byte RandomX key for VM initialization |
| `height` | Block height (integer, no decimal) |
| `reserved_offset` | Byte offset of nonce in mining blob (always 39) |

## Submit

### Request

```json
{
    "jsonrpc": "2.0",
    "method": "submit",
    "params": {
        "id": "<client_id>",
        "job_id": "linear-job-<height>",
        "nonce": "<4-byte hex-encoded nonce>",
        "result": "<RandomX hash hex>"
    },
    "id": 2
}
```

`nonce` is 4 bytes, little-endian, hex-encoded. `result` is the RandomX hash
(xmrig includes it for logging; daemon re-computes independently).

### Response

```json
{"jsonrpc": "2.0", "result": {"status": "OK"}, "id": 2}
```

Status values: `"OK"` (accepted), `"rejected"` (PoW invalid), `"stale"`
(height mismatch or rate-limited).

### Submit Validation Steps

1. **Serialize** — `linear_submit_lock` prevents concurrent RandomX VM access
2. **Rate limit** — reject if `now - last_block_time < min_block_interval` (default 10s)
3. **Height check** — reject if `submitted_height != current_height + 1`
4. **PoW verify** — reconstruct block with found nonce, hash with RandomX,
   check `u32_le(hash[0..4]) <= target`
5. **Anchor** — if Caribina enabled, anchor to Arweave (best-effort, non-blocking)
6. **Insert** — `insert_validated_block()` → record timestamp, adjust target,
   store block, clear mempool (RPC path only)
7. **Push** — generate new template, notify all stratum clients via publisher

## 227-Byte Mining Blob Layout

The mining blob is a compact binary serialization of `BlockHeader`. It is what
miners hash with RandomX — the PoW covers all fields except anchor data (which
is set after mining).

| Offset | Size | Field | Encoding |
|--------|------|-------|----------|
| 0 | 32 | `previous` | blake3 hash bytes (previous block hash) |
| 32 | 1 | `version` | u8 |
| 33 | 4 | `target` | u32 LE |
| 37 | 2 | reserved | `[0u8; 2]` (padding) |
| 39 | 4 | `nonce` | u32 LE (mined value) |
| 43 | 8 | `height` | u64 LE |
| 51 | 32 | `merkle_root` | blake3 hash (transaction merkle root) |
| 83 | 8 | `timestamp` | u64 LE (Unix seconds) |
| 91 | 32 | `uncle_merkle_root` | 32 bytes (uncle merkle tree root) |
| 123 | 8 | `total_reward` | u64 LE (base units) |
| 131 | 32 | `randomx_key` | 32 bytes (RandomX VM key for this block) |
| 163 | 32 | `coin_merkle_root` | 32 bytes |
| 195 | 32 | `nullifier_root` | 32 bytes |
| **227** | | **Total** | |

Fields **excluded** from the mining blob (set after PoW is found):
`anchor_tx_id`, `anchor_monero_height`, `anchor_monero_hash`, `finality_flags`.

### Nonce Offset

The nonce is at byte offset **39** — matching xmrig's hardcoded Monero rx/0
nonce offset. xmrig inits RandomX with `seed_hash`, XORs the nonce bytes at
offset 39..42 with its counter, hashes the blob, and checks the result against
the target.

## Target Encoding Bridge

The daemon and xmrig use different difficulty checks:

| Component | Check |
|-----------|-------|
| Daemon (Rust) | `u32::from_le_bytes(hash[0..4]) <= target` (32-bit) |
| xmrig (C++) | `u64::from_le_bytes(hash[0..8]) <= strtoull(target_hex)` (64-bit) |

To bridge this, the target is encoded as:

```
FFFFFFFF{target:08x}
```

The upper 32 bits (`0xFFFFFFFF`) ensure that any value in `hash[4..8]` passes
xmrig's 64-bit check. The lower 32 bits are the actual target, matching the
daemon's 32-bit comparison exactly.

**Example**: If `target = 0x00FFFFFF` (~1/256 hashes pass), the encoded target
string sent to xmrig is `"FFFFFFFF00ffffff"`.

## RandomX Key Derivation

Each block gets a unique RandomX key derived from height:

```rust
pub fn derive_key_from_height(height: u64) -> [u8; 32] {
    let height_bytes = height.to_le_bytes();
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&height_bytes);
    key
}
```

This prevents pre-computation attacks — a miner can't reuse RandomX state
across blocks. The key changes every block, requiring a new RandomX VM
initialization (~2 seconds for 4 GB dataset).

## xmrig Compatibility Notes

- **Agent string**: xmrig sends `"xmrig/6.21.0"` (version varies). The daemon
  logs it but doesn't enforce a specific format.
- **Algo**: Must include `"rx/0"` — this is the RandomX algorithm identifier in
  the stratum protocol.
- **Job ID**: xmrig checks that the job_id in `mining.notify` matches what it's
  currently mining. If the daemon pushes a new job, xmrig switches to it.
- **Reserved offset**: Hardcoded to 39 in the response. xmrig uses this to know
  where to write the nonce.
- **Integer formatting**: All integers use raw format (no `.0` suffix) because
  rapidjson's `GetUint64`/`GetInt` on float values triggers assertions.
- **Password field**: Required by the protocol but value is ignored. Pass any
  non-empty string (e.g. `"x"`).

## Error Responses

| Error | Condition | Response |
|-------|-----------|----------|
| Missing login | No `login` field in params | `server_error(MinerMissingLogin)` |
| Invalid algo | `algo` doesn't contain `"rx/0"` | `server_error(MinerRandomXNotSupported)` |
| Invalid address | Wallet address can't be parsed | `server_error(MinerInvalidRecipientPrefix)` |
| Network mismatch | Address isn't testnet | `server_error(MinerInvalidRecipientPrefix)` |
| Stale height | `submitted_height != current_height + 1` | `miner_status_response("stale")` |
| Rate limited | Block too soon after previous | `miner_status_response("stale")` |
| PoW invalid | `hash_u32 > target` | `miner_status_response("rejected")` |
