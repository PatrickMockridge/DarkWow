Compiling and Running a Node
=========================

This guide covers setting up and running a DarkWow testnet node. For a quick
start with simple shell scripts, see the [Quick Start](#quick-start) section.
For detailed manual configuration, continue reading below.

> **Conda Users**: If using conda environments, run `conda deactivate` before running DarkWow binaries. Conda's Python and library paths may conflict with DarkWow's native dependencies. See [Using dnet](../learn/dchat/network-tools/using-dnet.md) for venv setup.

## Overview

This tutorial will cover the three DarkWow blockchain components and
their current features. The components covered are:

* `darkfid` is the DarkWow fullnode. It validates blockchain
transactions and stays connected to the p2p network.
* `drk` is a CLI wallet. It provides an interface to smart contracts
such as Money and DAO, manages our keys and coins, and scans the
blockchain to update our balances.
* `xmrig` is the mining daemon used in DarkWow. Connects to `darkfid`
over its `Stratum` RPC, and requests new block headers to mine.

The config files for `darkfid` and `drk` are sectioned into three
parts, each marked `[network_config]`. The sections look like this:

* `[network_config."testnet"]`
* `[network_config."mainnet"]`
* `[network_config."localnet"]`

At the top of each daemon config file, we can modify the network being
used by changing the following line:

```toml
# Blockchain network to use
network = "testnet"
```

This enables us to configure the daemons for different contexts, namely
mainnet, testnet and localnet. Mainnet is not active yet. Localnet can
be setup by following the instructions [here](#local-deployment). The
rest of this tutorial assumes we are setting up a testnet node.

## Quick Start

For a simplified setup, use the provided shell scripts in `contrib/testnet/`:

```shell
# 1. Build darkfid and drk
cargo build --release -p darkfid -p drk

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
| `start.sh` | Start darkfid daemon in background |
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
can compile the `darkfid` node and the `drk` wallet CLI:

```shell
$ make darkfid drk

...
make -C bin/darkfid \
        PREFIX="/home/anon/.cargo" \
        CARGO="cargo" \
        RUST_TARGET="x86_64-unknown-linux-gnu" \
        RUSTFLAGS=""
make[1]: Entering directory '/home/anon/darkfi/bin/darkfid'
RUSTFLAGS="" cargo build --target=x86_64-unknown-linux-gnu --release --package darkfid
...
   Compiling darkfid v0.5.0 (/home/anon/darkfi/bin/darkfid)
    Finished `release` profile [optimized] target(s) in 4m 19s
cp -f ../../target/x86_64-unknown-linux-gnu/release/darkfid darkfid
cp -f ../../target/x86_64-unknown-linux-gnu/release/darkfid ../../darkfid
make[1]: Leaving directory '/home/anon/darkfi/bin/darkfid'
make -C bin/drk \
        PREFIX="/home/anon/.cargo" \
        CARGO="cargo" \
        RUST_TARGET="x86_64-unknown-linux-gnu" \
        RUSTFLAGS=""
make[1]: Entering directory '/home/anon/darkfi/bin/drk'
RUSTFLAGS="" cargo build --target=x86_64-unknown-linux-gnu --release --package drk
...
   Compiling drk v0.5.0 (/home/anon/darkfi/bin/drk)
    Finished `release` profile [optimized] target(s) in 2m 16s
cp -f ../../target/x86_64-unknown-linux-gnu/release/drk drk
cp -f ../../target/x86_64-unknown-linux-gnu/release/drk ../../drk
make[1]: Leaving directory '/home/anon/darkfi/bin/drk'
```

This process will now compile the node and the wallet CLI tool.
When finished, we can begin using the network. Run `darkfid` and `drk`
once so their config files are spawned on your system. These config files
will be used to `darkfid` and `drk`.

Please note that the exact paths may differ depending on your local setup.

```shell
$ ./darkfid

Config file created in "~/.config/darkfi/darkfid_config.toml". Please review it and try again.
```

```shell
$ ./drk interactive

Config file created in "~/.config/darkfi/drk_config.toml". Please review it and try again.
```

## Running

### Using Tor

DarkWow supports Tor for network-level anonymity. To use the testnet over
Tor, you'll need to make some modifications to the `darkfid` config
file.

For detailed instructions and configuration options on how to do this,
follow the [Tor Guide](../misc/nodes/tor-guide.md#configure-network-settings).

### Wallet initialization

Now it's time to initialize your wallet. For this we use `drk`, a separate
wallet CLI which is created to interface with the smart contract used
for payments and swaps.

First, you need to change the password in the `drk` config. Open
your config file in a text editor (the default path is
`~/.config/darkfi/drk_config.toml`). Look for the section marked
`[network_config."testnet"]` and change this line:

```toml
# Password for the wallet database
wallet_pass = "changeme"
```

Once you've changed the default password for your testnet wallet, we
can proceed with the wallet initialization. We simply have to
initialize a wallet, and create a keypair. The wallet address shown in
the outputs is explanatory and will be different from the one you get.

```shell
$ ./drk -c bin/drk/drk_config.toml -n localnet wallet initialize

Initializing Money Merkle tree
Successfully initialized Merkle tree for the Money contract
Generating alias DRKW for Token: 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb
Initializing DAO Merkle trees
Successfully initialized Merkle trees for the DAO contract
```

```shell
$ ./drk -c bin/drk/drk_config.toml -n localnet wallet keygen

Generating a new keypair
New address:
{YOUR_DARKFI_WALLET_ADDRESS}
```

```shell
$ ./drk -c bin/drk/drk_config.toml -n localnet wallet default-address 1
```

The second command will print out your new DarkWow address where you
can receive payments. Take note of it. Alternatively, you can always
retrieve your default address using:

```shell
$ ./drk -c bin/drk/drk_config.toml -n localnet wallet address

{YOUR_DARKFI_WALLET_ADDRESS}
```

### Darkfid

Now that `darkfid` configuration is in place, you can run it again and
`darkfid` will start, create the necessary keys for validation of blocks
and transactions, and begin syncing the blockchain.

```shell
$ ./darkfid

[INFO] Initializing DarkWow node...
[INFO] Node is configured to run with fixed PoW difficulty: 1
[INFO] Initializing a Darkfi daemon...
[INFO] Initializing Validator
[INFO] Initializing Blockchain
[INFO] Deploying native WASM contracts
[INFO] Deploying NativeToken Contract with ContractID DgmXpuU1EcM54E8GuNTAkBUThcCoYzGN5kRCNXA4cPtw
[INFO] Successfully deployed NativeToken Contract
[INFO] Deploying MoneyV2 Contract with ContractID 21LYoifepcySKhyDA1vzxRDWGHyDizPQ8f11zSqhep7t
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
rest of the tutorial (`darkfid` and `drk` handle this), but if you want
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
the `Stratum` RPC endpoint that will be used by `xmrig` in `darkfid`
config:

```toml
[network_config."testnet".stratum_rpc]
rpc_listen = "tcp://127.0.0.1:18347"
```

> Note:
>
> If you are not on the same network as the `darkfid` instance you
> are using, you must configure and use `tcp+tls` for the RPC
> endpoints, so your traffic is not plaintext, as it contains your
> wallet address used for the block rewards.

To mine on DarkWow we need to add a recipient to `xmrig` that specifies
where the mining rewards will be minted to. You now have to configure
`xmrig` to use your wallet address as the rewards recipient, when it
retrieves blocks from `darkfid` to mine. Make sure you have
[initialized](#wallet-initialization) your wallet and grab your default
address:

```shell
./drk wallet address

{YOUR_DARKFI_WALLET_ADDRESS}
```

Refer to [xmrig optimizations guide][2] to fully configure your system
for maximum mining performance. Start `darkfid` as usual and then start
`xmrig`, specifying retries setup, how many threads to mine and for
which wallet:

```shell
$ ./xmrig -u x+1 -r 1000 -R 20 -o 127.0.0.1:18347 -t {XMRIG_THREADS} -u {YOUR_DARKFI_WALLET_ADDRESS}
```

> Note: All miners should use the lowest possible resources so other
> people can mine blocks to retrieve `DRKW` for testing.

In `darkfid`, you should see a notification like this:

```shell
...
[INFO] [RPC-STRATUM] Got login from {YOUR_DARKFI_WALLET_ADDRESS} ({AGENT_INFO})
...
```

This means that `darkfid` and `xmr` are connected over the `Stratum`
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

From this point forward in the guide we will use `drk` in `interactive`
mode for all our wallet operations. In another terminal, run the
following command:

```shell
$ ./drk interactive

drk>
```

In order to receive incoming coins, you'll need to use the `drk`
tool to subscribe on `darkfid` so you can receive notifications for
incoming blocks. The blocks have to be scanned for transactions,
and to find coins that are intended for you. In the interactive shell,
run the following command to subscribe to new blocks:

```shell
drk> subscribe

Requested to scan from block number: 0
Last confirmed block reported by darkfid: 1 - da4455f461df6833a68b659d1770f58e44b6bc4abdd934cb22d084c24333255f
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
Last confirmed block reported by darkfid: 1 - da4455f461df6833a68b659d1770f58e44b6bc4abdd934cb22d084c24333255f
Finished scanning blockchain
Subscribing to receive notifications of incoming blocks
Detached subscription to background
All is good. Waiting for block notifications...
```

## Local Deployment

For local (non-testnet) development we recommend running master, and
use the existing `contrib/localnet/darkfid-single-node` folder, which
provides the corresponding configurations to operate. Some outputs are
emitted since they are identical to previous steps.

First, compile `darkfid` node and the `drk` wallet CLI:

```shell
$ make darkfid drk
```

> Note:
>
> Make sure you have properly setup `xmrig` [miner](#miner) as its
> required.

Enter the localnet folder, and initialize a wallet:

```shell
$ cd contrib/localnet/darkfid-single-node/
$ ./init-wallet.sh
```

Then configure your mining daemon in `tmux_sessions.sh`
script, start the daemons and wait until `darkfid` is initialized:

```shell
$ ./tmux_sessions.sh
```

After some blocks have been generated we
will see some `DRKW` in our test wallet.
On a different shell (or tmux pane in the session),
navigate to `contrib/localnet/darkfid-single-node`
folder again and check wallet balance

```shell
$ ./wallet-balance.sh

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+---------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRKW     | 20
```

Alternatively, use the drk CLI directly:

```shell
$ ./drk -c drk.toml wallet balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+---------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRKW     | 20
```

Don't forget that when using this local node, all operations
should be executed inside the `contrib/localnet/darkfid-single-node`
folder. The `drk.toml` file in that folder contains the correct
configuration for localnet.

## DIY Public Testnet

If the official testnet is unavailable, you can create a standalone public
testnet for yourself or a small group. This section explains how to set up
a self-contained network with its own genesis block, seed node, and mining.

### Overview

A DIY testnet differs from localnet in several ways:
- **No `skip_sync`**: Nodes must sync from peers, not start instantly
- **Seed configuration**: New nodes need seed addresses to discover peers
- **Public ports**: Seed nodes must listen on ports accessible to others
- **Fresh genesis**: Every participant must use identical genesis state

### Network Parameters

Choose network parameters that don't conflict with mainnet or testnet:

| Parameter | Example Value |
|-----------|---------------|
| Network name | `diynet` or `localnet` |
| P2P Port | `18340` (or custom) |
| RPC Port | `18345` |
| Management RPC | `18346` |
| Stratum Mining | `18347` |
| PoW Target | `60` seconds |
| Confirmation Threshold | `3` blocks |

### Step 1: Create Configuration

Create a configuration file for your seed node (`diynet-darkfid.toml`):

```toml
network = "diynet"

[network_config."diynet"]
database = "~/.local/share/darkfi/darkfid/diynet"
threshold = 3
pow_target = 60
pow_fixed_difficulty = 4
skip_sync = false
skip_fees = false

[network_config."diynet".rpc]
rpc_listen = "tcp://0.0.0.0:18345"

[network_config."diynet".management_rpc]
rpc_listen = "tcp://0.0.0.0:18346"

[network_config."diynet".stratum_rpc]
rpc_listen = "tcp://0.0.0.0:18347"

[network_config."diynet".net]
localnet = false
active_profiles = ["tcp+tls"]

[network_config."diynet".net.profiles."tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:18340"]
seeds = []
```

Create a corresponding wallet configuration (`diynet-drk.toml`):

```toml
network = "diynet"

[wallet]
wallet_pass = "your_secure_password"
wallet_db = "~/.local/share/darkfi/drk/diynet"

[network_config."diynet"]
rpc_url = "http://127.0.0.1:18345"
stratum_url = "tcp://127.0.0.1:18347"
```

### Step 2: Initialize Wallet

```shell
./drk -c diynet-drk.toml -n diynet wallet initialize
./drk -c diynet-drk.toml -n diynet wallet keygen
./drk -c diynet-drk.toml -n diynet wallet default-address 1
```

Save your wallet address for the mining step.

### Step 3: Start Seed Node

On the machine that will act as seed node:

```shell
./darkfid -c diynet-darkfid.toml
```

On first startup, the node will generate a genesis block. Share the
configuration and genesis state with other participants.

### Step 4: Connect Mining Nodes

Start xmrig with your wallet address as the recipient:

```shell
./xmrig -u x+1 -o tcp://SEED_IP:18347 -t 4 -u YOUR_WALLET_ADDRESS
```

Or use `drk` for integrated mining:

```shell
./drk -c diynet-drk.toml -n diynet mine
```

### Step 5: Share Configuration

For other participants to join your testnet, they need:
1. Your configuration files
2. The same genesis block (auto-generated on first start if using same config)
3. Your seed node's IP address and port

### Multi-Node Setup

To add more nodes, configure them to connect to your seed node:

```toml
[network_config."diynet".net.profiles."tcp+tls"]
seeds = ["tcp+tls://SEED_IP:18340"]
```

### Troubleshooting

**Node won't start**: Check that ports are available and not in use.

**Mining not working**: Ensure stratum RPC is enabled and reachable.

**Genesis mismatch**: All nodes must use identical configuration and
genesis block. Clear database and restart if genesis differs.

**Wallet locked**: Kill any running `drk` processes and retry.

### Restoring from Snapshot

If you have a database snapshot, place it in the database directory before
starting the node. The node will verify the snapshot matches the expected
state.

## Advanced Usage

To run a node in full debug mode:

```shell
$ LOG_TARGETS='!sled,!rustls,!net' ./darkfid -vv | tee /tmp/darkfid.log
```

The `sled` and `net` targets are very noisy and slow down the node so
we disable those.

We can now view the log, and grep through it.

```shell
$ tail -n +0 -f /tmp/darkfid.log | grep -a --line-buffered -v DEBUG
```

[1]: https://xmrig.com/docs/miner/build
[2]: https://xmrig.com/docs/miner/randomx-optimization-guide
