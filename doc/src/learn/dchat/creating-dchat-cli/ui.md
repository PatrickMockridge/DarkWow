# Python UI

The dchat CLI is a Python client that communicates with `dchatd` via JSON-RPC.
This document explains the implementation of `example/dchat/dchat-cli/main.py`.

## JsonRpc Class

The `JsonRpc` class handles all JSON-RPC communication with the daemon:

```python
{{#include ../../../../../example/dchat/dchat-cli/main.py:18:81}}
```

### Methods

* `start(server, port)`: Establishes a TCP connection to the RPC server
* `stop()`: Closes the TCP connection gracefully
* `_make_request(method, params)`: Internal method to send a JSON-RPC request and receive a response
* `_subscribe(method, params)`: Internal method to send a JSON-RPC subscription request

### Public API Methods

* `ping()`: Send a ping request
* `dnet_switch(state)`: Enable or disable dnet (boolean parameter)
* `dnet_subscribe_events()`: Subscribe to dnet network events
* `send(message)`: Broadcast a message to all peers
* `recv()`: Retrieve buffered messages

## Main Entry Point

```python
{{#include ../../../../../example/dchat/dchat-cli/main.py:84:136}}
```

The `main()` function parses command-line arguments, establishes an RPC connection,
and dispatches to the appropriate handler based on the subcommand provided.

## Extending the CLI

To add new RPC methods:

1. Add the corresponding handler method to the `JsonRpc` class (e.g., `async def new_method(self):`)
2. Call `self._make_request("new_method", [])` with appropriate parameters
3. Add a new branch in `main()` to handle the subcommand
