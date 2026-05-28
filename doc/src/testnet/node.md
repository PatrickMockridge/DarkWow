# Compiling and Running a Node

> **See also:** [Level 4: Containerized Devnet Node](../dev/testing/level-4-devnet.md)
> for running a Docker-based devnet node across multiple machines on LAN or internet.

This guide covers setting up and running a DarkWow testnet node. For a quick
start with simple shell scripts, see the [Quick Start](#quick-start) section.
For detailed manual configuration, continue reading below.

> **Conda Users**: If using conda environments, run `conda deactivate` before running DarkWow binaries. Conda's Python and library paths may conflict with DarkWow's native dependencies. See [Using dnet](../learn/dchat/network-tools/using-dnet.md) for venv setup.

## Overview

This tutorial will cover the three DarkWow blockchain components and
their current features. The components covered are:

* `dwowd` is the DarkWow fullnode. It validates blockchain
transactions and stays connected to the p2p network.
* `dwow_wallet` is a CLI wallet. It provides an interface to smart contracts
such as NativeToken and PromissoryNote, manages our keys and coins, and scans the
blockchain to update our balances.
* `xmrig` is the mining daemon used in DarkWow. Connects to `dwowd`
over its `Stratum` RPC, and requests new block headers to mine.

The config files for `dwowd` and `dwow_wallet` are sectioned into three
parts, each marked `[network_config]`. The sections look like this:

* `[network_config."darkwow-testnet"]`
* `[network_config."mainnet"]`
* `[network_config."localnet"]`

At the top of each daemon config file, we can modify the network being
used by changing the following line:

```toml
# Blockchain network to use
network = "darkwow-testnet"
```

This enables us to configure the daemons for different contexts, namely
mainnet, testnet and localnet. Mainnet is not active yet. Localnet can
be setup by following the instructions [here](#local-deployment). The
rest of this tutorial assumes we are setting up a testnet node.

## Quick Start

For a simplified setup, use the provided shell scripts in `contrib/testnet/`:

```shell
# 1.  dwowd and dwow_wallet
cargo build --release -p dwowd -p dwow_wallet

# 2. Setup testnet node (creates directories and config)
cd contrib/testnet
./setup.sh

# 3. Start the daemon
./start.sh

# 4. Check status
./status.sh

# 5. View logs
./logs.sh

# 6. Stop the daemon
./stop.sh
```

### Available Scripts

| Script | Description |
|--------|-------------|
| `setup.sh` | First-time setup (creates directories and config) |
| `start.sh` | Start dwowd daemon in background |
| `stop.sh` | Stop the daemon gracefully |
| `restart.sh` | Restart the daemon |
| `status.sh` | Check sync status via RPC |
| `logs.sh` | View daemon logs (tail -f) |
| `wallet-init.sh` | Initialize a wallet |
| `upgrade.sh` | Pull latest changes and rebuild |

### Docker

Alternatively, run testnet in Docker:

```shell
cd contrib/testnet
docker-compose up -d
docker-compose logs -f
docker-compose down
```

### Manual Setup

If you prefer manual configuration, continue with the sections below.

Since this is still an early phase, we will not be installing any of
the software system-wide. Instead, we'll be running all the commands
from the git repository, so we're able to easily pull any necessary
updates.

Refer to the main [DarkWow](../index.html#build) page for instructions
on how to install Rust and necessary deps. Skip last step of the build
process, as you don't need to compile all binaries of the project.

Once you have the repository in place, and everything is installed, we
can compile the `dwowd` node and the `dwow_wallet` wallet CLI:

```shell
$ make dwowd dwow_wallet

...
make -C bin/dwowd \
        PREFIX="/home/anon/.cargo" \
        CARGO="cargo" \
        RUST_TARGET="x86_64-unknown-linux-gnu" \
        RUSTFLAGS=""
make[1]: Entering directory '/home/anon/dwow/bin/dwowd'
RUSTFLAGS="" cargo build --target=x86_64-unknown-linux-gnu --release --package dwowd
...
   Compiling dwowd v0.5.0 (/home/anon/dwow/bin/dwowd)
    Finished `release` profile [optimized] target(s) in 4m 19s
cp -f ../../target/x86_64-unknown-linux-gnu/release/dwowd dwowd
cp -f ../../target/x86_64-unknown-linux-gnu/release/dwowd ../../dwowd
make[1]: Leaving directory '/home/anon/dwow/bin/dwowd'
make -C bin/drk \
        PREFIX="/home/anon/.cargo" \
        CARGO="cargo" \
        RUST_TARGET="x86_64-unknown-linux-gnu" \
        RUSTFLAGS=""
make[1]: Entering directory '/home/anon/dwow/bin/drk'
RUSTFLAGS="" cargo build --target=x86_64-unknown-linux-gnu --release --package dwow_wallet
...
   Compiling dwow_wallet v0.5.0 (/home/anon/dwow/bin/drk)
    Finished `release` profile [optimized] target(s) in 2m 16s
cp -f ../../target/x86_64-unknown-linux-gnu/release/dwow_wallet dwow_wallet
cp -f ../../target/x86_64-unknown-linux-gnu/release/dwow_wallet ../../dwow_wallet
make[1]: Leaving directory '/home/anon/dwow/bin/drk'
```

This process will now compile the node and the wallet CLI tool.
When finished, we can begin using the network. Run `dwowd` and `dwow_wallet`
once so their config files are spawned on your system. These config files
will be used to `dwowd` and `dwow_wallet`.

Please note that the exact paths may differ depending on your local setup.

```shell
$ ./dwowd

Config file created in "~/.config/dwow/dwowd_config.toml". Please review it and try again.
```

```shell
$ ./dwow_wallet interactive

Config file created in "~/.config/dwow/dww_config.toml". Please review it and try again.
```

## Running

### Using Tor

DarkWow supports Tor for network-level anonymity. To use the testnet over
Tor, you'll need to make some modifications to the `dwowd` config
file.

For detailed instructions and configuration options on how to do this,
follow the [Tor Guide](../misc/nodes/tor-guide.md#configure-network-settings).

### Wallet initialization

Now it's time to initialize your wallet. For this we use `dwow_wallet`, a separate
wallet CLI which is created to interface with the smart contract used
for payments and swaps.

First, you need to change the password in the `dwow_wallet` config. Open
your config file in a text editor (the default path is
`~/.config/dwow/dww_config.toml`). Look for the section marked
`[network_config."testnet"]` and change this line:

```toml
# Password for the wallet database
wallet_pass = "changeme"
```

Initialize a wallet and create a keypair. See
[Wallet Architecture](../arch/wallet.md) for details. Use
`-c bin/drk/dww_config.toml -n localnet` for localnet configuration.

### Darkfid

Now that `dwowd` configuration is in place, you can run it again and
`dwowd` will start, create the necessary keys for validation of blocks
and transactions, and begin syncing the blockchain.

```shell
$ ./dwowd

[INFO] Initializing DarkWow node...
[INFO] Node is configured to run with fixed PoW difficulty: 1
[INFO] Initializing a Darkfi daemon...
[INFO] Initializing Validator
[INFO] Initializing Blockchain
[INFO] Deploying native WASM contracts
[INFO] Deploying NativeToken Contract with ContractID DgmXpuU1EcM54E8GuNTAkBUThcCoYzGN5kRCNXA4cPtw
[INFO] Successfully deployed NativeToken Contract
[INFO] Deploying WASM contract with ContractID 21LYoifepcySKhyDA1vzxRDWGHyDizPQ8f11zSqhep7t
...
```

As its syncing, you'll see periodic messages like this:

```shell
...
[INFO] Blocks received: 4020/4763
...
```

This will give you an indication of the current progress. Keep it running,
and you should see a `Blockchain synced!` message after some time.

### Miner

It's not necessary for broadcasting transactions or proceeding with the
rest of the tutorial (`dwowd` and `dwow_wallet` handle this), but if you want
to help secure the network, you can participate in the mining process
by running an `xmrig` mining daemon. In this example we will build
`xmrig` from its respective source code repository. Make sure you are
not in the DarkWow repository folder as we are going to retrieve
external repos.

First, install its [dependencies][1], retrieve its repo and checkout
the latest release tag:

```shell
$ git clone --recursive https://github.com/xmrig/xmrig
$ cd xmrig
$ git checkout $(git describe --tags "$(git rev-list --tags --max-count=1)")
```

Now we can build it:

```shell
$ mkdir build
```

If you have already build `xmrig` above command will fail as folder
already exists, so just continue to next ones:

```shell
$ cd build
$ cmake ..
$ make -j$(nproc)
```

The binary now exists in the current directory. Make sure you enable
the `Stratum` RPC endpoint that will be used by `xmrig` in `dwowd`
config:

```toml
[network_config."darkwow-testnet".stratum_rpc]
rpc_listen = "tcp://127.0.0.1:31347"
```

> Note:
>
> If you are not on the same network as the `dwowd` instance you
> are using, you must configure and use `tcp+tls` for the RPC
> endpoints, so your traffic is not plaintext, as it contains your
> wallet address used for the block rewards.

To mine on DarkWow we need to add a recipient to `xmrig` that specifies
where the mining rewards will be minted to. You now have to configure
`xmrig` to use your wallet address as the rewards recipient, when it
retrieves blocks from `dwowd` to mine. Make sure you have
[initialized](#wallet-initialization) your wallet and grab your default
address:

```shell
./dwow_wallet wallet address

{YOUR_DARKFI_WALLET_ADDRESS}
```

> **Docker pre-configured wallet**: When running a Docker-based node, you can
> skip the manual address extraction. Pass `WALLET_ADDRESS` and
> `WALLET_SECRET_FILE` (bind-mounted file, not env var) to pre-seed the mining
> keypair before the daemon starts. Coinbase rewards flow directly to your
> wallet with no manual steps.
> See the [darkwow-testnet README](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/contrib/docker/darkwow-testnet/README.md#wallet-setup)
> for the one-step setup.

Refer to [xmrig optimizations guide][2] to fully configure your system
for maximum mining performance. Start `dwowd` as usual and then start
`xmrig`, specifying retries setup, how many threads to mine and for
which wallet:

```shell
$ ./xmrig -u x+1 -r 1000 -R 20 -o 127.0.0.1:31347 -t {XMRIG_THREADS} -u {YOUR_WALLET_ADDRESS}
```

> Note: All miners should use the lowest possible resources so other
> people can mine blocks to retrieve `DRKW` for testing.

In `dwowd`, you should see a notification like this:

```shell
...
[INFO] [RPC-STRATUM] Got login from {YOUR_DARKFI_WALLET_ADDRESS} ({AGENT_INFO})
...
```

This means that `dwowd` and `xmr` are connected over the `Stratum`
RPC and `xmrig` can start mining. You will see log messages like these:

```shell
...
[INFO] Created new block template for wallet: address=DZns...rKkf, spend_hook=-, spend_hook=-, user_data=-
[INFO] [RPC-STRATUM] Created new mining job for client 091e...9d71: 26d6...8a3c
[INFO] [RPC-STRATUM] Got solution submission from client 091e...9d71 for job: 26d6...8a3c
[INFO] Appended proposal 6188...e623c
[INFO] Proposing new block to network
...
```

To stop mining you can `^C` `xmrig` anytime to quit it or press `p` to
pause mining.

### Wallet sync

From this point forward in the guide we will use `dwow_wallet` in `interactive`
mode for all our wallet operations. In another terminal, run the
following command:

```shell
$ ./dwow_wallet interactive

dww>
```

In order to receive incoming coins, you'll need to use the `dwow_wallet`
tool to subscribe on `dwowd` so you can receive notifications for
incoming blocks. The blocks have to be scanned for transactions,
and to find coins that are intended for you. In the interactive shell,
run the following command to subscribe to new blocks:

```shell
dww> subscribe

Requested to scan from block number: 0
Last confirmed block reported by dwowd: 1 - da4455f461df6833a68b659d1770f58e44b6bc4abdd934cb22d084c24333255f
Requesting block 0...
Block 0 received! Scanning block...
=======================================
Header {
        Hash: b967812a860e8bf43deb03dd4f7cf69258f7719ddb7f2183d4e4fa3559b9f39d
        Version: 1
        Previous: 86bbac430a4b3a182f125b37a486e9c486bbfa34d84ef4a66b4a23e5f0c625b1
        Height: 0
        Timestamp: 2025-05-12T13:00:24
        Nonce: 0
        Transactions Root: 0x081361c364feba0d28a418e2e20c216ce442d5127036e3491ceaf1996fdb3c3b
        State Root: afc1694dd6b290d8b92c33d3fc746707da9bed857eb9e90f11683d2e243b8047
        Proof of Work data: Darkfi
}
=======================================
[scan_block] Iterating over 1 transactions
[scan_block] Processing transaction: 91525ff00a3755a8df93c626b59f6e36cf021d85ebccecdedc38f3f1890a15fc
Requesting block 1...
Block 1 received! Scanning block...
...
Requested to scan from block number: 2
Last confirmed block reported by dwowd: 1 - da4455f461df6833a68b659d1770f58e44b6bc4abdd934cb22d084c24333255f
Finished scanning blockchain
Subscribing to receive notifications of incoming blocks
Detached subscription to background
All is good. Waiting for block notifications...
```


## Local Development and Custom Testnets

For local (non-testnet) development, see the [Local Devnet Setup](../localnet-dev.md) guide.

For setting up a standalone testnet (DIY public testnet), see
[Bootstrapping a Testnet](bootstrapping.md), which covers phased network
deployment from single-machine Docker to internet-scale multi-node networks.

[1]: https://xmrig.com/docs/miner/build
[2]: https://xmrig.com/docs/miner/randomx-optimization-guide
