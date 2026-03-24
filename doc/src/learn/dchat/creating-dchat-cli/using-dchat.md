# Using dchat

Now that `dchatd` is running, we can connect to it using the Python CLI client.

## Running the CLI

The dchat CLI is located at `example/dchat/dchat-cli/main.py`. Before running,
ensure you have Python 3 installed. The CLI connects to a dchatd instance via
JSON-RPC.

```shell
python example/dchat/dchat-cli/main.py --help
```

## Command-line Options

* `-e, --endpoint`: RPC endpoint address (default: `localhost:51054`)
* `-h, --help`: Show help information

## Subcommands

### send

Send a message to all connected peers:

```shell
python example/dchat/dchat-cli/main.py send "Hello, world!"
```

### recv

Receive and display all buffered messages:

```shell
python example/dchat/dchat-cli/main.py recv
```

### ping

Send a ping to verify connectivity:

```shell
python example/dchat/dchat-cli/main.py ping
```

## Example Usage

Start two `dchatd` instances (one as seed, one as node) following the
deployment instructions. Then in two separate terminals:

```shell
# Terminal 1: Send a message
python example/dchat/dchat-cli/main.py send "Hello from Alice"

# Terminal 2: Receive messages
python example/dchat/dchat-cli/main.py recv
```

## RPC Protocol

The CLI communicates with `dchatd` using JSON-RPC 2.0 over TCP. The `JsonRpc`
class in `main.py` handles connection management, request serialization,
and response parsing. See [Adding methods](../creating-dchatd/rpc-methods.md)
for details on the available RPC methods.
