# get_info

The `p2p.get_info` RPC method returns information about the P2P network state,
including connected channels and outbound slots.

## Request

```json
{
  "jsonrpc": "2.0",
  "method": "p2p.get_info",
  "params": [],
  "id": 1
}
```

## Response

```json
{
  "jsonrpc": "2.0",
  "result": {
    "channels": [
      {
        "url": "/ip4/127.0.0.1/tcp/50515",
        "session": "inbound",
        "id": 12345
      }
    ],
    "outbound_slots": [0, 1, 2, 3]
  },
  "id": 1
}
```

## Response Fields

| Field | Type | Description |
|-------|------|-------------|
| `channels` | Array | List of connected channels |
| `channels[].url` | String | The address of the remote peer |
| `channels[].session` | String | Session type: `inbound`, `outbound`, `manual`, `refine`, `seed`, or `direct` |
| `channels[].id` | Number | Unique identifier for this channel |
| `outbound_slots` | Array | List of outbound slot IDs |

## Session Types

* `inbound`: Incoming connection from a remote peer
* `outbound`: Outgoing connection to a remote peer
* `manual`: Manually established connection
* `refine`: Connection used for state sync
* `seed`: Connection to a seed node
* `direct`: Direct peer-to-peer connection

## Example Usage

```shell
# Using the dchat CLI (if extended to support this method)
python example/dchat/dchat-cli/main.py p2p_get_info

# Or directly via JSON-RPC
echo '{"jsonrpc": "2.0", "method": "p2p.get_info", "params": [], "id": 1}' | nc localhost 51054
```

## Implementation

The `p2p.get_info` method is provided by the `HandlerP2p` trait in
`darkfi::rpc::p2p_method`. It iterates over the P2P node's hosts and collects
channel information including addresses, session types, and IDs.
