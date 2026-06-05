DarkWow x Monero Merge Mining using p2pool and xmrig
===================================================

> **Note:** For the complete merge mining reference — architecture, protocol, economics, and finality — see [Merge Mining](../arch/merge-mining.md). This document covers setup instructions.

This document provides a way to set up a Monero testnet that is
able to merge-mine DarkWow using `p2pool` and `xmrig`.

## Current Status

The merge mining pipeline (`test_pipeline.sh --mode merge`) runs the full pathway:

```
xmrig → p2pool (sidecar per node) → mm_rpc → dwowd → DarkWow block
         \--[monerod RPC/ZMQ]--> monerod (offline, fixed-difficulty)
```

Each merge-mining node runs its own p2pool and xmrig as sidecars — no standalone
p2pool container. Every node generates its own Monero testnet wallet at startup via
`monero-wallet-cli`. The Python model at `contrib/model/merge_mining_model.py`
confirms the architecture and achieves ALL VERIFIED consensus across 3 nodes.

**Last pipeline result:** 33 PASS, 2 FAIL (Phase 8 log pattern — xmrig IS running,
detection regex needs update). Merge-mined block accepted. `mm_submit_solution`
received. Cryptographic receipt chain verified.

**Monero testnet sync:** First-time public testnet sync takes ~12 hours and downloads
~100GB. The pipeline default is `MONERO_OFFLINE=true` (fixed difficulty 1000, no sync
required). For public testnet merge mining, set `MONERO_OFFLINE=false` and ensure
`~/.cache/dwow_merge_testnet_monero` has synced data. The data survives `--fresh`
rebuilds (host bind mount).

**Test result (2026-05-24):**

| Check | Status |
|---|---|
| dwowd starts, contracts deploy | Pass |
| mm_rpc responds to `merge_mining_get_chain_id` | Pass |
| p2pool starts stratum server | Pass |
| xmrig connects and receives jobs | Pass |
| p2pool calls mm_rpc `get_aux_block` (handshake active) | Pass |
| Merge mined block submitted via `merge_mining_submit_solution` | **Pass** |

Full cycle: p2pool discovered the chain, got block templates, miners found a
RandomX share, and p2pool submitted the solved aux block to dwowd.

### Requirements

**monerod sync.** monerod must be synced from public testnet once (~1M
blocks/hour with `--fast-block-sync=1`). The synced data dir persists at
`$HOME/.cache/dwow_merge_testnet_monero/` and is reused for all subsequent
test runs in offline mode.

**xmrig >= 6.22.2.** xmrig 6.21.1 has a RandomX buffer overflow bug
(glibc `_FORTIFY_SOURCE` abort during dataset init) that prevents it from
running on some Linux systems. The test script auto-downloads 6.22.2 if the
installed version is older.

**JSON-RPC compliance.** p2pool's JSON-RPC parser rejects any response
containing an `"error"` field — even `"error": null`. The successful
response serialization in `src/rpc/jsonrpc.rs` must omit the `"error"`
field entirely (matching JSON-RPC 2.0 spec).

Two scripts are available:

**Full test** (`test_merge_mining_p2pool.sh`) — auto-downloads binaries, validates
prerequisites, smoke-tests xmrig, detailed progress output. Use for CI or first run.

**Minimal reproduce** (`reproduce_merge_mining.sh`) — 133 lines, no checks or
auto-download. Assumes binaries are built and monerod is synced. Quick iteration
for development.

```bash
# Full test (auto-downloads missing binaries, checks prerequisites):
./contrib/docker/darkwow-testnet/test_merge_mining_p2pool.sh --no-build

# Minimal reproduce (binaries must already exist):
./contrib/docker/darkwow-testnet/reproduce_merge_mining.sh
```

The test fails fast with the exact sync command if monerod isn't synced.

**mm_rpc methods implemented** (see [dwowd JSON-RPC](../clients/dwowd_jsonrpc.md#merge-mining-xmr)):
- `merge_mining_get_chain_id` — returns chain ID for aux chain discovery
- `merge_mining_get_aux_block` — returns aux blob, difficulty, hash
- `merge_mining_submit_solution` — accepts solved aux block, verifies PoW, applies block

## Architecture

DarkWow merge mining operates in three layers. Every computer on the network
handshakes via lilith — pool mining and merge mining are overlays on top, not
replacements for the base P2P layer.

| Layer | Component | Role | Mandatory? |
|-------|-----------|------|------------|
| 1 — P2P | lilith + dwowd | Node discovery, block propagation, tx gossip | **Yes** — everyone |
| 2 — Pool | p2pool | Aggregates miner hashrate, PPLNS payouts | No — solo miners skip |
| 3 — Merge | p2pool + monerod | Bridges to Monero, embeds aux data | No — pure DarkWow pools skip |

p2pool connects directly to dwowd's mm_rpc HTTP JSON-RPC endpoint (no adaptor).
p2pool speaks the merge mining protocol natively; dwowd implements the same
protocol. For merge mining (Layer 3), p2pool additionally connects to monerod
via RPC+ZMQ for block templates and real-time notifications.

See [Mining Network Architecture](../arch/mining-tokenomics.md#mining-network-architecture)
for the full topology, ASCII diagrams, and mm_rpc interface description.

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

**Anchoring finality.** Merge-mined and native blocks can include Monero block
references as anchors. Once the anchor has N confirmations on Monero, the
DarkWow block is finalized — protected against reorgs, double-spends, and
re-ordering attacks. This is a modular security overlay that does not replace
PoW fork choice. See [Anchoring Finality Gadget](../arch/mining-tokenomics.md#anchoring-finality-gadget)
for the full design.

### Enabling Monero Finality

To enable Monero finality verification, add these settings to your
`dwowd_config.toml` or pass them as CLI flags:

```toml
[network_config."darkwow-testnet".finality]
monero_enabled = true
monero_min_confirmations = 3
# Optional: full verification against a monerod node
monerod_url = "http://127.0.0.1:18081/json_rpc"
```

CLI equivalents:

```bash
dwowd --network darkwow-testnet --finality-enable-monero \
    --monerod-rpc-url http://127.0.0.1:18081/json_rpc
```

Without `monerod_url`, the daemon falls back to lightweight plausibility checks
(accepts any anchor height up to 5M). With `monerod_url` set, the daemon
queries monerod via JSON-RPC to verify the anchor hash matches the actual
Monero block and that `monero_min_confirmations` have elapsed.

The full economic model is documented in [Mining Tokenomics](../arch/mining-tokenomics.md#merge-mining-competition).
A Python simulation matching the Rust consensus 1:1 is available at
`contrib/docker/darkwow-testnet/merge_mining_model.py` — run it to explore
hashpower ratios, uncle phases, and reward distribution interactively.

> **Conda Users**: If using conda environments, run `conda deactivate` before running DarkWow binaries. Conda's Python and library paths may conflict with DarkWow's native dependencies. Consider using a venv as described in [Using dnet](../learn/dchat/network-tools/using-dnet.md).

## Docker Quick Start

The easiest way to test merge mining is with the darkwow-testnet Docker
setup, which bundles monerod + p2pool + xmrig behind a single flag.

By default, `monerod` runs in offline mode — which **requires a synced data dir**
from a previous online sync (see "Monero setup" below). On first run, set
`MONERO_OFFLINE=false` to sync from the live Monero testnet, or sync monerod
manually before starting the Docker stack.

See the [darkwow-testnet README](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/contrib/docker/darkwow-testnet/README.md)
for the full docker compose commands, merge mining env var reference, and
pipeline modes.

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
testnet configuration already includes mm_rpc on port 31348:

```toml
[network_config."darkwow-testnet".mm_rpc]
rpc_listen = "tcp://127.0.0.1:31348"
```

If using docker compose, mm_rpc port 31348 is already exposed.

Then start `dwowd` as usual:

Stop `p2pool` if it's running, and re-run it with the merge-mining
parameters appended:

```shell
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet {YOUR_MONERO_WALLET_ADDRESS_HERE} --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd --merge-mine 127.0.0.1:31348 {YOUR_DARKFI_WALLET_ADDRESS}
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
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet {YOUR_DAO_MONERO_WALLET_ADDRESS_HERE} --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd --merge-mine 127.0.0.1:31348 {YOUR_DAO_WALLET_ADDRESS_MINING_CONFIGURATION}
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
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet {YOUR_MONERO_WALLET_ADDRESS_HERE} --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd --merge-mine 127.0.0.1:31348 {YOUR_DARKFI_WALLET_ADDRESS}
```

And `xmrig` (requires >= 6.22.2, see [Current Status](#current-status)):

```shell
$ ./xmrig -o 127.0.0.1:3333 -u x -p 20000 -a rx/0 -t 1 --keepalive
```

## Verification

After running the merge mining test or setting up manually, verify the pathway
worked by checking dwowd's log for merge mining activity:

```shell
grep "RPC-MM" /tmp/dwow_merge_test/dwowd.log
```

A successful merge mining cycle shows these events in order:

| Log entry | What it proves |
|---|---|
| `RPC-MM.*merge_mining_get_chain_id` | p2pool discovered the aux chain |
| `RPC-MM.*merge_mining_get_aux_block` | p2pool requested a block template |
| `RPC-MM.*merge_mining_submit_solution` | p2pool submitted a solved block |
| `BLOCK ACCEPTED` | dwowd verified PoW and applied the block |

A `merge_mining_submit_solution` event followed by `BLOCK ACCEPTED` is the
definitive proof that merge mining produced a valid DarkWow block.

To check p2pool's side of the handshake:

```shell
grep -i "merge.mine\|aux.chain\|sidechain block\|found share" /tmp/dwow_merge_test/p2pool.log
```

To check monerod block production:

```shell
curl -s -X POST http://127.0.0.1:28081/json_rpc \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"get_info","id":1}' \
    | python3 -c "import sys,json; print(json.load(sys.stdin)['result']['height'])"
```

## Troubleshooting

### "monerod is not synchronized" in p2pool log

p2pool requires monerod to report `synchronized: true` before starting its
stratum server. In offline mode, this requires a previously synced data dir.
To fix:

1. Sync monerod from public testnet once:
   ```shell
   monerod --testnet --no-igd --data-dir $HOME/.cache/dwow_merge_testnet_monero \
     --log-level 0 --hide-my-port \
     --add-peer 125.229.105.12:28081 --add-peer 37.187.74.171:28089 \
     --fast-block-sync=1 --zmq-pub tcp://127.0.0.1:28083 \
     --rpc-bind-ip 127.0.0.1 --rpc-bind-port 28081 \
     --confirm-external-bind --non-interactive
   ```
2. Wait for `"You are now synchronized with the network"` in the log.
3. Stop monerod. The data dir now has the full chain.
4. Subsequent runs use `--offline --fixed-difficulty 20000` and start instantly.

`--fast-block-sync=1` uses embedded checkpoint hashes to skip full validation
of historical blocks, syncing at ~1M blocks/hour on modern hardware.

### mm_rpc endpoint not responding

```shell
curl -s -X POST http://127.0.0.1:31348 \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","params":[],"id":1}'
```

If this returns nothing, check that `mm_rpc` is configured in `dwowd_config.toml`:
```toml
[network_config."linear-testnet".mm_rpc]
rpc_listen = "http+tcp://127.0.0.1:31348"
```

### xmrig connects but no shares found

- Verify p2pool stratum is running: `grep -i "StratumServer" p2pool.log`
- Check xmrig is receiving jobs: `grep "new job" xmrig.log`
- With `--fixed-difficulty 20000` and 1 thread, shares can take several minutes
- Increase mining threads (`-t 4`) for faster results in testing

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
