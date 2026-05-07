# Attaching dnet

`dnet` is DarkWow's network topology visualization tool. To use it with your
application, you need to attach the dnet subscription to your P2P stack.

## Overview

The dchatd implementation includes dnet integration by:

1. Creating a `JsonSubscriber` for dnet events
2. Starting a `StoppableTask` that subscribes to dnet events
3. Forwarding received events to subscribers via JSON-RPC notifications

## Implementation

In `dchatd/src/main.rs`, dnet integration is handled as follows:

```rust
{{#include ../../../../../example/dchat/dchatd/src/main.rs:dnet}}
```

### Key Components

**JsonSubscriber**: Creates a subscription channel for JSON-RPC notifications.
The subscriber is initialized with the method name `"dnet.subscribe_events"`.

**StoppableTask**: A task that runs the dnet event loop. It:
1. Subscribes to dnet events via `p2p.dnet_subscribe()`
2. In a loop, receives events and notifies subscribers
3. Handles graceful shutdown via the `StoppableTask` mechanism

## RPC Methods for dnet

The dnet integration exposes two RPC methods:

### dnet.switch

Enable or disable dnet in the P2P stack:

```json
{
  "jsonrpc": "2.0",
  "method": "dnet.switch",
  "params": [true],
  "id": 1
}
```

Pass `true` to enable, `false` to disable.

### dnet.subscribe_events

Subscribe to dnet network events. Returns JSON-RPC notifications when
network events occur (connections, disconnections, messages, etc.).

```json
{
  "jsonrpc": "2.0",
  "method": "dnet.subscribe_events",
  "params": [],
  "id": 1
}
```

## Visualizing with dnet

Once your application has dnet attached and is running, you can use the
`dnet` tool to visualize your P2P network. See [Using dnet](using-dnet.md)
for instructions on running and configuring dnet.
