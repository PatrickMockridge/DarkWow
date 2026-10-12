# Node configurations

This section provides configuration examples for hosting DarkWow P2P nodes.

## Default Ports

### `dwowd` Mainnet
- `8340`: Inbound P2P
- `_ +1`: Inbound Tor/I2p/...
- `8345`: Public RPC server
- `8346`: Restricted RPC server
- `8347`: Stratum RPC server
- `8348`: P2pool Merge Mining RPC server


###  `dwowd` Testnet
- `18340`: Inbound clear P2P
- `_ + 1`: Inbound Tor/I2p/...
- `18345`: General query RPC server
- `18346`: Restricted RPC server
- `18347`: Stratum RPC server
- `18348`: P2pool Merge Mining RPC server


###  `darkirc`
- `6667`: IRC server (plaintext)
- `6697`: IRC server (TLS)
- `26661`: Inbound P2P (`bin/darkirc/darkirc_config.toml`'s commented default)
- `_ +1`: Inbound Tor/I2p/...
- `26660`: RPC server


### `fud`
- The ports below are **unverified**: `bin/fud/` holds only the `fu` and `fud` binaries, with no
  config file to read a default from. Treat the numbers as historical until a config lands.
- `9700`: Inbound P2P
- `_ +1`: Inbound Tor/I2p/...
- `9705`: Public RPC server
- `9706`: Restricted RPC server


### `taud`
- `23331`: Inbound P2P (`bin/tau/taud/taud_config.toml`'s commented default; the lilith seeds use it too)
- `_ +1`: Inbound Tor/I2p/...
- `23330`: RPC server (`rpc_listen = "tcp://127.0.0.1:23330"`)

`darkirc` and `taud` were listed at `9600` and `9800` with RPC at `9605`/`9606` and `9805`/`9806`
until 2026-10-12 (`F-611`). Neither number appears in either project's config; the values above are
the files' own. A node that copies the old ports will not find its seeds.
