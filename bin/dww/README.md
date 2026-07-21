# dww

DarkWow wallet CLI. A full-node wallet that holds the complete blockchain on
local disk, scans for capabilities, manages keys via the AccountManager, and
builds/signs transactions. Uses manifest-first architecture — contracts carry
their own interfaces on-chain; zero wallet code changes for new contracts.

## Building

```shell
make
```

## Usage

```shell
./target/release/dwow_wallet --help
./target/release/dwow_wallet -c dww_config.toml -n darkwow-devnet wallet balance
```

## Documentation

- [Node Operator Guide](../../doc/src/for-node-operators.md) — Wallet node operation
- [Wallet Architecture](../../doc/src/arch/wallet.md) — Full wallet specification
- [Local Devnet Setup](../../doc/src/localnet-dev.md) — Mining and balance checks
