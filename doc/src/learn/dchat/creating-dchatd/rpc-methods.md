# Adding methods

Now we'll implement the RPC methods for our `Dchat` application.
These methods allow clients to interact with the P2P network and exchange messages.

## Request Handler Implementation

The `RequestHandler` trait processes incoming JSON-RPC requests by matching
on the method name and dispatching to the appropriate handler:

```rust
{{#include ../../../../../example/dchat/dchatd/src/rpc.rs:req_match}}
```

## Implemented Methods

### send

Broadcasts a message to all connected peers.

```json
{
  "jsonrpc": "2.0",
  "method": "send",
  "params": ["Hello, world!"],
  "id": 42
}
```

```json
{
  "jsonrpc": "2.0",
  "result": true,
  "id": 42
}
```

### recv

Retrieves all buffered messages received from the P2P network.

```json
{
  "jsonrpc": "2.0",
  "method": "recv",
  "params": [],
  "id": 42
}
```

```json
{
  "jsonrpc": "2.0",
  "result": ["Hello, world!", "Hi there!"],
  "id": 42
}
```

### ping

A simple ping/pong test method. This method is automatically provided by
`RequestHandler` and requires no parameters.

```json
{
  "jsonrpc": "2.0",
  "method": "ping",
  "params": [],
  "id": 42
}
```

```json
{
  "jsonrpc": "2.0",
  "result": true,
  "id": 42
}
```

### p2p.get_info

Returns information about connected P2P channels and outbound slots.
Provided by the `HandlerP2p` trait. See [get_info](../network-tools/get-info.md)
for full documentation.

### dnet.switch

Enables or disables the dnet subsystem in the P2P stack.

```json
{
  "jsonrpc": "2.0",
  "method": "dnet.switch",
  "params": [true],
  "id": 42
}
```

```json
{
  "jsonrpc": "2.0",
  "result": true,
  "id": 42
}
```

### dnet.subscribe_events

Subscribes to dnet network events. Returns a subscription that will
receive JSON-RPC notifications when network events occur.

```json
{
  "jsonrpc": "2.0",
  "method": "dnet.subscribe_events",
  "params": [],
  "id": 1
}
```

## Handler Implementation

```rust
{{#include ../../../../../example/dchat/dchatd/src/rpc.rs:58:102}}
```
