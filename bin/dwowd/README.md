# dwowd

DarkWow full-node daemon. Provides chain state management, block validation,
WASM smart contract execution, P2P networking, and RandomX mining infrastructure.

## Building

```shell
make
```

## Usage

```shell
./target/release/dwowd --help
./target/release/dwowd -c dwowd_config.toml
```

## Documentation

- [dwowd Architecture Reference](../../doc/src/dwowd.md) — Complete daemon reference
- [Node Operator Guide](../../doc/src/for-node-operators.md) — Running a node or mining
- [Local Devnet Setup](../../doc/src/localnet-dev.md) — Development network setup

## Configuration

The default config is at `~/.config/dwow/dwowd_config.toml`. A heavily commented
template is shipped in this directory: [dwowd_config.toml](dwowd_config.toml).
