lilith
======

A tool to deploy multiple P2P network seed nodes for DarkWow
applications with a single daemon.

## Usage

```
lilith
Daemon that spawns P2P seeds

USAGE:
    lilith [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               Increase verbosity (-vvv supported)

OPTIONS:
    -c, --config <config>                           Configuration file to use
    -l, --log <log>                                 Set log file to output into
        --whitelist-refinery-interval <seconds>     Interval for whitelist peer checks [default: 120]
```

Accept addresses, host files, and RPC listen URLs are configured per-network
in the TOML config file (`~/.config/dwow/lilith_config.toml`), not via CLI flags.
The config file defines each network's seed parameters including accept addresses
and hostlist paths.

On first execution, lilith will create a default config file.
Configuration must be verified, and application networks should be configured accordingly.

Run lilith as follows:

```
$ lilith
[INFO] Found configuration for network: foo_network
[INFO] Starting seed network node for "foo_network" on ["tcp://0.0.0.0:18911"]
[INFO] [P2P] Seeding P2P subsystem
[WARN] [P2P] Skipping seed sync process since no seeds are configured.
[INFO] [P2P] Running P2P subsystem
[INFO] [P2P] Starting Inbound session #0 on tcp://0.0.0.0:18911
[INFO] [P2P] Starting 0 outbound connection slots.
[INFO] [P2P] P2P subsystem started
[INFO] Starting periodic host purge task for "foo_network"
```
