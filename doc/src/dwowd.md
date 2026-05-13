# DarkWow Daemon

`dwowd` is the DarkWow full node daemon. It validates the blockchain, processes
transactions, and provides a JSON-RPC interface for wallets and miners.

## Native Contracts

dwowd ships with two genesis contracts deployed at block 1:

- **Deployooor** — enables WASM contract deployment via `DeployV1` calls.
  All user contracts are deployed through Deployooor.
- **NativeToken** — consensus token handling network fees (`FeeV1`) and block
  rewards (`PoWRewardV1`).

## WASM Contracts

Additional WASM contracts are deployed by users via Deployooor:

- **Money V3** — DeFi token operations (mint, transfer, burn, OTC swaps)
- **DAO Escrow** — decentralized governance with escrow-backed proposals
- **Stablecoin** — collateralized stablecoin

Money V1 and V2 are deprecated on this fork.

## Configuration

Configs live in `~/.config/dwow/dwowd_config.toml`. Example for darkwow-testnet:

```toml
network = "darkwow-testnet"

[network_config."darkwow-testnet"]
database = "~/.local/share/dwow/dwowd/darkwow-testnet"
threshold = 3
pow_target = 120
recipient = "YOUR_WALLET_ADDRESS"

[network_config."darkwow-testnet".rpc]
rpc_listen = "tcp://127.0.0.1:31345"

[network_config."darkwow-testnet".stratum_rpc]
rpc_listen = "tcp://127.0.0.1:31347"

[network_config."darkwow-testnet".net]
localnet = false
inbound = ["tcp+tls://0.0.0.0:31342"]
seeds = ["tcp+tls://seed.darkwow.org:31340"]
allowed_transports = ["tcp+tls"]
outbound_connections = 8
```

See [testnet-mining.md](testnet/testnet-mining.md) for a full mining setup guide.
