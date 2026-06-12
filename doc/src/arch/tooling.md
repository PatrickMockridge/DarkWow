# Tooling

## Universal Full-Node Daemon

`dwowd` is the universal full-node daemon. Every full node — whether it mines
blocks or scans for coins — runs `Dwowd::init_linear()`. The daemon provides
the shared foundation: CChainState (sled-backed blockchain store), P2P network
participation (lilith seed discovery, block sync, transaction broadcast), native
contract deployment, and genesis block creation.

`dwowd` does not have any concept of keys or wallet functionality. It does not
manage keys. Key management belongs to the wallet layer which runs on top of
the universal daemon.

The daemon exposes JSON-RPC and stratum interfaces:

* Get node status and modify settings realtime
* Query the blockchain
* Broadcast transactions to the P2P network
* Get transaction status, query the mempool, interact with contracts

## Wallet

The wallet (`dwow_wallet`) is a full node — it calls `Dwowd::init_linear()`
for the shared daemon foundation, then adds wallet-specific state: SQLite
database (keys, coins, contracts, capabilities) and cache sled database
(wallet-specific SMT indices, scan progress).

The wallet does NOT have its own chain state initialization. CChainState
belongs to the universal daemon. The wallet accesses blockchain data through
the daemon, same as the mining node does.

The wallet manages keys and cryptographic objects. It scans blocks from
CChainState (the local sled database maintained by the daemon), decrypts
AEAD-encrypted notes, and derives capabilities through pure local computation.

The wallet exposes a high-level CLI with subcommands (`keygen`, `balance`,
`transfer`, `scan`, etc.) and will expose a JSON-RPC API corresponding
**exactly** to its commands so that product teams can easily build
applications. The CLI tool serves as an interactive debugging application
and point of reference.

The API should be well documented with all arguments explained.
Likewise for the commands help text.

Command cheatsheets and example sessions are strongly encouraged.
