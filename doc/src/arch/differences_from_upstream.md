# DarkWow: Differences From Upstream

## Philosophy

DarkWow is a simplified fork of the DarkFi protocol. Upstream DarkFi is a
generic multi-network P2P framework designed to host independent services
(darkirc, tau, multiple blockchains) on a shared seed infrastructure (lilith).
DarkWow simplifies this to a single-purpose blockchain node with optional P2P
services, governed by a three-tier feature gate that cleanly separates essential
blockchain infrastructure from optional protocol extensions.

The guiding principle: **opt-in governance, uncensorable native token, no
dependency on seed nodes.**

## Three-Tier Feature Gate

```
net-wallet ⊂ net-node ⊂ net-full
```

### net-wallet — Essential Blockchain Infrastructure

The minimal P2P stack needed for a wallet to join the DarkWow blockchain
network. Contains:

- `ProtocolAddress` — P2P address exchange (PEX gossip) for decentralized
  peer discovery. This is the ONLY address-exchange mechanism needed. No
  separate seed protocol. No seed dependency.
- `OutboundSession` — manages outbound connections to discovered peers.
- `ManualSession` — direct connections to configured peers (Bitcoin's
  `-addnode` equivalent). Wallets bootstrap by connecting to known
  community nodes.
- `InboundSession` — accept inbound connections (optional, for wallets
  that also serve blocks).
- `ProtocolVersion` — magic bytes + semantic version handshake.
  `app_name` is informational only — never used to reject connections.
- `ProtocolPing` — keepalive heartbeat.
- `HostContainer` — address store (tried/new/banned tiers).
- `Channel` — framed binary stream with message dispatching.
  MissingDispatcher never kills a channel — unknown messages are
  logged and ignored. No censorship.

**What net-wallet excludes** (gated out at compile time):

- `ProtocolSeed` — a separate seed protocol duplicating ProtocolAddress.
  Bitcoin, Monero, and Ethereum all use a single address-exchange protocol.
  DarkWow does too.
- `SeedSyncSession` — a separate session type for seed connections.
  Seed connections are just outbound connections. No special session needed.
- `BanPolicy` — a mechanism for banning peers that send unknown message
  types. Anti-P2P. Removed. Unknown messages are always logged and ignored.
- `SESSION_SEED` — a session bitflag only used by ProtocolSeed.
  Dead without it.

### net-node — Mining and Observer Infrastructure

Everything in `net-wallet` plus:

- `RefineSession` — greylist refinery for long-running nodes (miners,
  observer nodes). Periodically probes addresses to verify liveness
  before adding to the whitelist. Not needed by wallets.

### net-full — Full Upstream P2P Stack

Everything in `net-node` plus:

- `ProtocolSeed` + `SeedSyncSession` — upstream seed protocol.
- `BanPolicy` — upstream ban mechanism (opt-in for services that
  need spam filtering, e.g. darkirc).
- `SESSION_SEED` — upstream session bitflag.

**net-full is NOT used by the DarkWow blockchain.** It exists only for
non-blockchain P2P services (darkirc, tau, lilith seed nodes) that
need the full upstream protocol stack. The blockchain track uses
`net-wallet` (wallets) or `net-node` (miners, observer nodes).

## Architecture: No Dependence on Seed Nodes

Upstream requires every node to have a configured seed (lilith) for
bootstrap. This creates a single point of failure and a censorship
vector — if lilith is down or hostile, no new nodes can join.

DarkWow removes this dependency. Nodes bootstrap through:

1. **Peers config** (`peers = [...]` in TOML) — direct connections to
   known community nodes. Bitcoin's `-addnode` equivalent. Wallets,
   miners, and observer nodes all support this. Configured peers are
   just regular full nodes that happen to be well-known.

2. **ProtocolAddress PEX** — once connected to any peer, the node
   discovers the rest of the network through PEX gossip. Addresses
   flow from peer to peer. No seed needed after initial bootstrap.

3. **Observer nodes** — community-run full nodes that serve as
   bootstrap points. They are regular `dwowd` nodes with mining
   disabled. They run the same protocol stack as miners — no protocol
   mismatches, no MissingDispatcher channel deaths.

The seed node (lilith) is an optional convenience, not a requirement.
It still exists for non-blockchain P2P services (darkirc, tau) but
the DarkWow blockchain does not depend on it.

## Protocol Simplifications

| Feature | Upstream | DarkWow |
|---------|----------|---------|
| Address exchange | Two protocols (ProtocolSeed + ProtocolAddress) | One protocol (ProtocolAddress) |
| Bootstrap | Seed dependency (lilith) | Peers config + PEX gossip |
| Ban mechanism | BanPolicy::Strict defaults to banning | Removed. Unknown messages logged, never banned |
| Session types | 6 (INBOUND, OUTBOUND, MANUAL, SEED, REFINE, DIRECT) | 5 (SEED gated out for net-wallet/net-node) |
| Host colors | 5 (Gold, White, Grey, Dark, Black) | 4 (Dark gated out) |
| Message handling | MissingDispatcher kills channel | MissingDispatcher logs and continues |
| Feature model | Monolithic (net-wallet, net-full) | Three-tier (net-wallet ⊂ net-node ⊂ net-full) |
| app_name in handshake | Validated as gate | Informational only |

## Opt-In Governance

The feature gate hierarchy embodies the principle of opt-in governance.
Every node operator chooses their level of protocol complexity:

- Wallet operators compile with `net-wallet` — minimal P2P, no seed
  dependency, no censorship.
- Mining/observer operators compile with `net-node` — adds refinery
  for long-running nodes.
- Service operators (darkirc, tau) compile with `net-full` — full
  upstream stack with all protocol features.

No node is forced to include code it doesn't need. The blockchain
track is cleanly separated from the P2P services track.

## Uncensorable Native Token

The DarkWow blockchain (native token DRKW) operates on the `net-wallet`
and `net-node` tiers. These tiers:

- Have no seed dependency — the network cannot be censored by
  shutting down seed nodes.
- Have no BanPolicy — peers cannot be banned for protocol-level
  message type mismatches.
- Use PEX gossip for decentralized peer discovery — the network
  graph is self-sustaining after initial bootstrap from any
  configured peer.
- Use ProtocolAddress as the SINGLE address-exchange protocol —
  simpler, fewer failure modes, no protocol fragmentation.

The native token is uncensorable because the network that carries it
has no central points of control.
