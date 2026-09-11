# genev — Event Graph Daemon and CLI

`genevd` is a standalone **event graph** daemon — a P2P message-passing DAG
seeded with a hardcoded *genesis event* (the name's origin). It is unrelated
to blockchain genesis: it syncs `GenEvent` messages (nickname, title, text)
between peers, exactly like the event graph used by darkirc. `genev`
("Generic Event example CLI") posts and lists those messages through the
daemon's JSON-RPC interface.

See [Event Graph](../../doc/src/arch/legacy/event_graph.md) for the data
structure, sync model, and the sled-overlay quarantine boundary.

## Building

```shell
cargo build --release -p genevd -p genev
```

## Running a daemon

```shell
./target/release/genevd --config genevd/genev_config.toml
```

Defaults (embedded in the binary, mirrored in `genevd/genev_config.toml`):

| Setting | Default |
|---------|---------|
| JSON-RPC listen | `tcp://127.0.0.1:28880` |
| Datastore | `~/.local/share/dwow/genev_db` |
| Replay datastore | `~/.local/share/dwow/replayed_genev_db` (with `--replay-mode`) |
| P2P accept | `tcp+tls://127.0.0.1:28881` (commented out — uncomment to accept peers) |

`--localnet` disables TLS cert verification for local P2P overlays;
`--skip-dag-sync` starts without the DAG sync task.

## Posting and listing events

```shell
./target/release/genev -e tcp://127.0.0.1:28880 add <nick> "<title>" "<text>"
./target/release/genev -e tcp://127.0.0.1:28880 list
```

`-e/--endpoint` defaults to `tcp://127.0.0.1:28880`.

## JSON-RPC surface

| Method | Purpose |
|--------|---------|
| `add` | Post a `GenEvent` (nick, title, text) into the local DAG |
| `list` | List local events |
| `eg_get_info` | Event graph daemon info |
| `dnet_subscribe_events` / `dnet_switch` | Subscribe to / activate-or-deactivate the dnet overlay |
| `deg_subscribe_events` / `deg_switch` | Subscribe to / activate-or-deactivate the darkirc event graph (deg) overlay |

## Multi-node testnet (script/)

`script/tmux_sessions.sh` starts a 4-daemon + 4-CLI tmux session from the
five configs in `script/` (`genevd_seed.toml`, `genevd_a.toml` … `genevd_d.toml`):
a seed daemon, four P2P-connected daemons, one `add` from the first CLI, then
`list` from the others to confirm the event propagated over the DAG.
