fud
=======

File-sharing Utility Daemon, using DHT for records discovery.

## Usage

```
fud
File-sharing Utility Daemon, using DHT for records discovery.

USAGE:
    fud [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               Increase verbosity (-vvv supported)

OPTIONS:
    -c, --config <config>             Configuration file to use
        --log <log>                   Set log file path to output daemon logs into
        --base-dir <base-dir>         Base directory for filesystem storage
                                      [default: ~/.local/share/dwow/fud]
    -d, --downloads-path <downloads>  Default path to store downloaded files
                                      [default: <base-dir>/downloads]
        --chunk-timeout <seconds>     Chunk transfer timeout in seconds [default: 60]
```

P2P, DHT, and RPC settings are configured via the TOML config file, not CLI flags.
On first execution, fud will create a default config file at
`~/.config/dwow/fud/fud_config.toml`. Review and adjust before running.

Run fud as follows:

```
% fud
13:23:04 [INFO] Starting JSON-RPC server
13:23:04 [INFO] Starting sync P2P network
13:23:04 [WARN] Skipping seed sync process since no seeds are configured.
13:23:04 [INFO] Initializing fud dht state for folder: "/home/x/.local/share/dwow/fud"
13:23:04 [INFO] Not configured for accepting incoming connections.
13:23:04 [INFO] JSON-RPC listener bound to tcp://127.0.0.1:9705
13:23:04 [INFO] Starting 8 outbound connection slots.
13:23:04 [INFO] Caught termination signal, cleaning up and exiting...
```

fu
=======

Command-line client for fud.

## Usage

```
fu
Command-line client for fud

USAGE:
    fu [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -e, --endpoint <ENDPOINT>    fud JSON-RPC endpoint [default: tcp://127.0.0.1:9705]
    -h, --help                   Print help information
    -v                           Increase verbosity (-vvv supported)
    -V, --version                Print version information

SUBCOMMANDS:
    get        Retrieve provided file from the fud network
    put        Upload a file to the fud network
    ls         List fud folder contents
    watch      Watch for changes in the fud folder
    rm         Remove a file from the fud network
    buckets    Get the current node buckets
    seeders    Lookup seeders of a resource from the network
    verify     Verify a downloaded resource
    lookup     Look up a resource in the DHT
    help       Print this message or the help of the given subcommand(s)
```

Execution examples:

```
% fu ls
13:25:14 [INFO] ----------Content-------------
13:25:14 [INFO]   seedd_config.toml
13:25:14 [INFO]   lt.py
13:25:14 [INFO] ------------------------------

% fu get -f lt.py
13:26:23 [INFO] File waits you at: /home/x/.local/share/dwow/fud/lt.py

% fu get -f sdsd
Error: JsonRpcError("\"Did not find key\"")
```
