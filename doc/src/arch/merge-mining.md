# Merge Mining

DarkWow uses Monero's RandomX proof-of-work algorithm for merge mining. Monero miners can secure DarkWow without additional energy cost. This chapter unifies the architecture, economics, setup, and finality aspects into a single reference.

## Quick Reference

| Component | File | Purpose |
|-----------|------|---------|
| `mm_rpc` handler | `bin/dwowd/src/rpc/mm_rpc.rs` | p2pool merge mining JSON-RPC (aux block template + solution submission) |
| Linear `BlockHeader` | `src/linear/src/block.rs` | Anchor fields: `anchor_monero_height`, `anchor_monero_hash`, `finality_flags` |
| `FinalityConfig` | `src/linear/src/finality.rs` | Monero finality config (`monero_enabled`, `monero_min_confirmations`) |
| Stratum handler | `bin/dwowd/src/rpc/stratum.rs` | xmrig stratum login/submit with RandomX PoW verification |
| Monero anchor verify | `src/linear/src/monero/verify.rs` | `verify_monero_anchor()` — dual-mode verification with fallback |

## Architecture & Protocol

DarkWow merge mining uses three separate protocols: mm_rpc JSON-RPC between p2pool and dwowd, RandomX stratum between xmrig and p2pool, and the DarkWow P2P network for block propagation.

### How It Works

1. p2pool connects to dwowd's mm_rpc JSON-RPC handler (port 31348)
2. p2pool calls `merge_mining_get_aux_block` to get a DarkWow block template (227-byte mining blob with nonce=0)
3. p2pool injects the aux data into Monero blocks and miners find RandomX solutions
4. When a Monero block is found with DarkWow aux data, p2pool calls `merge_mining_submit_solution`
5. dwowd reconstructs the block with the found nonce, verifies RandomX PoW, and inserts the block into the linear chain

### Three Pathways

| Pathway | Description | Status |
|---------|-------------|--------|
| mm_rpc merge mining | p2pool to dwowd mm_rpc direct (full merge pathway) | Working |
| Solo stratum | xmrig to dwowd stratum directly | Working |

### Protocol Architecture

Merge mining uses **two completely separate protocols** — p2pool communicates with dwowd via HTTP JSON-RPC, not the DarkWow P2P network.

```
MERGE MINING SIDE                           DARKFI P2P SIDE
══════════════════                         ══════════════════

p2pool                                     dwowd node0 (mm_rpc receiver)
  │                                           │
  │  HTTP JSON-RPC                            │
  ├─ merge_mining_get_chain_id ──────────────►│
  ├─ merge_mining_get_aux_block ─────────────►│
  │                                           │  constructs block template
  │◄────────────── aux_blob, aux_hash ────────┤
  │                                           │
  │  (injects aux data into Monero block)     │
  │  (xmrig mines Monero block)               │
  │                                           │
  ├─ merge_mining_submit_solution ───────────►│
  │                                           │  validates: check_aux_chains,
  │                                           │  RandomX hash, coinbase proof
  │                                           │  registry.submit()
  │                                           │
  │                                           ├─ validator.append_proposal()
  │                                           ├─ p2p.broadcast(ExtendedProposalMessage)
```

The mm_rpc interface on dwowd (raw TCP JSON-RPC, port 31348) provides:
- `merge_mining_get_chain_id` — identify the DarkWow chain
- `merge_mining_get_aux_block` — get the current block template with aux data
- `merge_mining_submit_solution` — submit a found merge-mined block

See [Monero Integration](monero.md) for the broader Monero integration context (bridging, stablecoin collateralization).

## Economics & Tokenomics

Merge mining creates a dual-reward structure. Miners earn both Monero block rewards (XMR) and DarkWow block rewards (DRKW) from the same RandomX computation. This gives DarkWow access to Monero's hashpower without competing for it.

### Mining Competition Model

DarkWow mining operates in three layers:

1. **Solo mining** — Direct stratum connection to dwowd, single miner
2. **P2Pool decentralized pool** — Miners share a p2pool instance, pool operator's dwowd submits blocks. The pool appears as a single miner from the chain's perspective. Participants share the DRKW reward according to contributed shares.
3. **Merge mining bridge** — The p2pool instance bridges to Monero, embedding DarkWow aux data into Monero coinbase transactions. When a block is found, the solution is submitted to both chains.

For the full mining tokenomics (supply schedule, exponential decay, tail emission, reward formula), see [Mining Tokenomics](mining-tokenomics.md).

## Setup Guide

### Prerequisites

- Monero testnet node (synchronized monerod)
- p2pool binary
- xmrig miner
- dwowd node

### Docker Quick Start

The merge mining test at `contrib/docker/darkwow-testnet/test_merge_mining_p2pool.sh` runs the full pathway end-to-end:

```
xmrig --> p2pool --[merge-mine]--> dwowd (mm_rpc)
                 \--[monerod RPC]--> monerod
```

See [Merge Mining Setup](../testnet/merge-mining.md) for the complete step-by-step setup guide, Docker commands, and manual configuration.

## Finality & Consensus

DarkWow uses a **Monero anchoring finality gadget**: each DarkWow block embeds a reference to a confirmed Monero block (height + hash). This provides an additional security layer — to reorganize the DarkWow chain, an attacker must also reorganize the Monero chain back past the anchor point.

The Monero anchor is verified during P2P block propagation (`bin/dwowd/src/proto/linear_broadcast.rs`). Configuration flags: `--finality-enable-monero`, `--monerod-rpc-url`.

### Finality Configuration

```bash
dwowd --network darkwow-testnet --finality-enable-monero \
    --monerod-rpc-url http://127.0.0.1:18081/json_rpc
```

For the Arweave-based finality layer, see [Caribina Finality](caribina.md). For the consensus mechanism that anchors these finality sources, see [Consensus](consensus/consensus.md).

## See Also

- [Mining Tokenomics](mining-tokenomics.md) — Supply schedule and reward economics
- [Monero Integration](monero.md) — Bridging, wrapping, and stablecoin collateral
- [Merge Mining Setup](../testnet/merge-mining.md) — Step-by-step setup guide
- [Caribina Finality](caribina.md) — Arweave-anchored finality layer
- [Consensus](consensus/consensus.md) — Uncle Merkle consensus mechanism
