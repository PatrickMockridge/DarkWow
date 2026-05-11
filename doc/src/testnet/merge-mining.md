DarkWow x Monero Merge Mining using p2pool and xmrig
===================================================

This document provides a way to set up a Monero testnet that is
able to merge-mine DarkWow using `p2pool` and `xmrig`.

## Merge Mining Economics

Merge mining allows Monero miners to produce valid DarkWow blocks using the
same RandomX proof-of-work. Two reward streams flow independently:

| Reward | Chain | Wallet curve | Delivery mechanism |
|--------|-------|-------------|-------------------|
| XMR coinbase | Monero | Ed25519/Curve25519 | p2pool `--wallet` |
| DRKW block reward | DarkWow | Pallas | p2pool `--merge-mine` address → `NativeToken::PoWRewardV1` |

**Competition.** Merge-mined blocks (`PowData::Monero`) and native DarkWow
blocks (`PowData::DarkFi`) compete under the identical `block_rank()` formula.
Monero's vastly larger hashpower means merge-mined blocks will win nearly all
canonical slots. Native miners rely on **Uncle Merkle rewards** (Phase 2) to
remain economically viable — without them, native mining has no path to
profitability.

The full economic model is documented in [Mining Tokenomics](../arch/mining-tokenomics.md#merge-mining-competition).
A Python simulation matching the Rust consensus 1:1 is available at
`contrib/docker/darkwow-testnet/merge_mining_model.py` — run it to explore
hashpower ratios, uncle phases, and reward distribution interactively.

> **Conda Users**: If using conda environments, run `conda deactivate` before running DarkWow binaries. Conda's Python and library paths may conflict with DarkWow's native dependencies. Consider using a venv as described in [Using dnet](../learn/dchat/network-tools/using-dnet.md).

## Docker Quick Start

The easiest way to test merge mining is with the darkwow-testnet Docker
setup, which bundles monerod + p2pool + xmrig behind a single flag:

```bash
# Start with merge mining (adds monerod, p2pool, and xmrig-merge containers)
MERGE_MINING=true docker compose --profile merge \
    -f contrib/docker/darkwow-testnet/docker-compose.yml up -d

# Check logs
docker logs dwow-p2pool
docker logs dwow-monerod

# Check blockchain status (dwowd uses raw TCP JSON-RPC)
docker exec dwow-node0 bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.last_confirmed_block\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3'

# Tear down
docker compose --profile merge \
    -f contrib/docker/darkwow-testnet/docker-compose.yml down
```

By default, `monerod` runs in offline mode (no Monero testnet sync needed).
To connect to the live Monero testnet, set `MONERO_OFFLINE=false`.

See the [darkwow-testnet README](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/contrib/docker/darkwow-testnet/README.md)
for the full merge mining env var reference.

## Bare-Metal Setup

The sections below cover building and running each component from source.
Use this for production or non-Docker deployments.

Please read the whole document first before executing commands, to
understand all the steps required and how each component operates.
Unless instructed otherwise, each daemon runs on its own shell, so
don't stop a running one to start another.

Each command to execute will be inside a codeblock, on its first line,
marked by the user `$` symbol, followed by the expected output. For
longer command outputs, some lines will be emitted to keep the guide
simple.

## Build binaries from source

We can build Monero and `p2pool` from their respective source code
repositories. Make sure you are not in the DarkWow repository folder as
we are going to retrieve external repos. Refer to
[miner](node.md#miner) section of the guide to build `xmrig` as its
required.

### Monero

First install its [dependencies][1] and then retrieve its repo and
checkout the latest release tag:

```shell
$ git clone --recursive https://github.com/monero-project/monero
$ cd monero
$ git checkout $(git describe --tags "$(git rev-list --tags --max-count=1)")
$ git submodule update --init
```

Now we can build it:

```shell
$ make -j$(nproc)

...
make[1]: Leaving directory '/home/anon/monero/build/Linux/_HEAD_detached_at_v0.18.4.4_/release'
```

Navigate to the directory listed at the end of build command where the
compiled binaries exist:

```shell
$ cd build/Linux/_HEAD_detached_at_v0.18.4.4_/release
$ cd bin
```

The path might look different in your system depending on your OS and
latest tag.

### p2pool

Enter a new shell outside of previously build Monero repo folder,
install `p2pool` [dependencies][2] and then retrieve its repo and
checkout the latest release tag:

```shell
$ git clone --recursive https://github.com/SChernykh/p2pool
$ cd p2pool
$ git checkout $(git describe --tags "$(git rev-list --tags --max-count=1)")
$ git submodule update --init
```

Now we can build it:

```shell
$ mkdir build
```

If you have already build `p2pool` above command will fail as folder
already exists, so just continue to next ones:

```shell
$ cd build
$ cmake ..
$ make -j$(nproc)
```

The binary now exists in the current directory.

## Monero setup

We should first sync the Monero Testnet locally. We can simply do this
by returning back to our Monero shell, starting up `monerod` and
waiting for the sync to finish:

```shell
$ ./monerod --testnet --no-igd --data-dir bitmonero --log-level 0 --hide-my-port --add-peer 125.229.105.12:28081 --add-peer 37.187.74.171:28089 --fast-block-sync=1 --zmq-pub tcp://127.0.0.1:28083

Synced 3601/2754128 (0%, 2750527 left)
Synced 5801/2754128 (0%, 2748327 left)
...
Synced 8101/2754128 (0%, 2746027 left)
Synced 9301/2754128 (0%, 2744827 left)
Synced 9801/2754128 (0%, 2744327 left)
```

After the sync is finished, we should also create a Monero wallet. On
a new shell in the same directory run `monero-wallet-cli` and follow
the wizard to create a wallet:

```shell
$ ./monero-wallet-cli --testnet --trusted-daemon

Generated new wallet: 9zMU...uQA4
View key: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
**********************************************************************
Your wallet has been generated!
```

Now we have our Monero address that we can use with p2pool to receive
mining rewards.

## p2pool setup without merge-mining

First we'll start `p2pool` without merge-mining to make sure everything
works in order. After we get `xmrig` set up, we'll restart `p2pool`
with merge-mining enabled.

`p2pool` connects to `monerod`'s JSONRPC and ZMQ Pub ports in order to
retrieve necessary mining data. It also provides a Stratum mining
endpoint that `xmrig` is able to connect to in order to receive mining
jobs and actually mine the proposed blocks.

We can start `p2pool` with the following command:

```shell
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet {YOUR_MONERO_WALLET_ADDRESS_HERE} --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd
```

Once started, it should connect to `monerod` and retrieve the latest
blockchain info. Now we can proceed with `xmrig` to try and mine some
blocks.

## xmrig setup

`xmrig` is pretty simple. Just start it with a chosen number of threads
and point it to `p2pool` Stratum port. `-t 1` is the number of CPU
threads to use for mining. All miners should use the lowest possible
resources so other people can mine blocks to retrieve `DRKW` for
testing.

```shell
$ ./xmrig -o 127.0.0.1:3333 -t 1
```

Now we should see blocks being mined in p2pool and submitted to our
Monero testnet. To stop mining you can `^C` xmrig anytime to quit it
or press `p` to pause mining.

## p2pool setup with merge-mining

> Note: `p2pool` uses plain `http` connections for RPC calls, as its
> assumed to be running on a localnet. Don't run a `p2pool` instance
> with a `dwowd` instance outside of your network, since someone
> snooping your traffic can see your wallet address used for the block
> rewards.

Now that everything is in order, we can use `p2pool` with merge-mining
enabled in order to merge mine DarkWow. For receiving mining rewards
on DarkWow, we'll need a DarkWow wallet address so make sure you have
[initialized](node.md#wallet-initialization) your wallet and grab your
default address:

```shell
dww> wallet address

{YOUR_DARKFI_WALLET_ADDRESS}
```

We will also need `dwowd` running with mm_rpc enabled. The default
testnet configuration already includes mm_rpc on port 18348:

```toml
[network_config."testnet".mm_rpc]
rpc_listen = "http+tcp://127.0.0.1:18348"
```

If using docker-compose, mm_rpc port 18348 is already exposed.

Then start `dwowd` as usual:

Stop `p2pool` if it's running, and re-run it with the merge-mining
parameters appended:

```shell
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet {YOUR_MONERO_WALLET_ADDRESS_HERE} --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd --merge-mine 127.0.0.1:18348 {YOUR_DARKFI_WALLET_ADDRESS}
```

Now `p2pool` should communicate with both `monerod` and `dwowd` in
order to pick up Monero blocktemplates and inject them with DarkWow data
necessary for merge-mining verification on the DarkWow side. Re-run
`xmrig` and now we should be mining blocks again. Once blocks are
found, they will be submitted to both `monerod` and `dwowd` and
`dwowd` should verify them and release block rewards to the address
provided to `p2pool` merge-mine parameters.

Happy mining!

## Merge-mining for a DAO

To retrieve a DAO merge mining configuration, execute:

```shell
dww> dao mining-config {YOUR_DAO}

DarkWow DAO mining configuration address:
{YOUR_DAO_WALLET_ADDRESS_MINING_CONFIGURATION}
```

Stop `p2pool` if it's running, and re-run it with the merge-mining
parameters appended:

```shell
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet {YOUR_DAO_MONERO_WALLET_ADDRESS_HERE} --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd --merge-mine 127.0.0.1:18348 {YOUR_DAO_WALLET_ADDRESS_MINING_CONFIGURATION}
```

After your miners have successfully mined confirmed blocks, you will
see the DAO `DRKW` balance increasing:

```shell
dww> dao balance {YOUR_DAO}

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+---------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRKW     | 80
```

## Offline merge-mining

For testing purposes its better to merge-mine in offline mode. To start
your Monero node in offline mode with fixed difficulty, execute:

```shell
$ ./monerod --testnet --no-igd --data-dir bitmonero --log-level 1 --hide-my-port --fixed-difficulty 20000 --disable-rpc-ban --offline --zmq-pub tcp://127.0.0.1:28083
```

Now start `p2pool`:

```shell
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet {YOUR_MONERO_WALLET_ADDRESS_HERE} --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd --merge-mine 127.0.0.1:18348 {YOUR_DARKFI_WALLET_ADDRESS}
```

And `xmrig`:

```shell
$ ./xmrig -u x+1 20000 -o 127.0.0.1:3333 -t 1
```

## Localnet merge mining testing

DarkWow's localnet configuration includes `mm_rpc` settings enabled by default,
allowing you to test merge mining without external Monero infrastructure.
The configuration files are located in `contrib/localnet/`:

- `dwowd-single-node/dwowd.toml` - Single node setup
- `dwowd-small/dwowd0.toml` - Small multi-node setup
- `dwowd-five-nodes/dwowd0.toml` - Five node setup

To test merge mining locally:

1. Ensure `mm_rpc` is enabled in your `dwowd.toml`:

```toml
[network_config."localnet".mm_rpc]
rpc_listen = "http+tcp://127.0.0.1:48348"
```

2. Start `dwowd` with localnet configuration:

```shell
$ ./dwowd --network localnet --config contrib/localnet/dwowd-single-node/dwowd.toml
```

3. Verify `mm_rpc` is listening:

```shell
$ curl -X jsonrpc -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","id":1}' \
    http://127.0.0.1:48348
```

4. To integrate with p2pool for full localnet testing, configure p2pool's
   merge-mining to point to this address:

```shell
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 \
    --wallet {YOUR_MONERO_WALLET_ADDRESS_HERE} --stratum 127.0.0.1:3333 \
    --data-dir ./p2pool-data --no-igd \
    --merge-mine 127.0.0.1:48348 {YOUR_DARKFI_WALLET_ADDRESS}
```

For full integration testing of the merge mining RPC implementation, use the
test suite:

```shell
$ cargo test -p dwowd merge_mining
```

[1]: https://github.com/monero-project/monero?tab=readme-ov-file#dependencies
[2]: https://github.com/SChernykh/p2pool?tab=readme-ov-file#prerequisites
