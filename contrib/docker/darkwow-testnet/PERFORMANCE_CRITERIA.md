# Test Pipeline Performance Criteria

Non-negotiable minimum criteria for every node type. Every pipeline run must verify all of these.

## Observer Node

- [ ] Container starts and stays running (no crash loops)
- [ ] P2P port accepts inbound connections
- [ ] Shares hostlist with connected peers
- [ ] Does NOT mine blocks (MINING_ENABLED=false)

## Mining Nodes

- [ ] Container starts and stays running (no crash loops)
- [ ] P2P connects to observer and peer mining nodes
- [ ] Generates or imports mining keypair from declared config
- [ ] Produces blocks (height advances)
- [ ] Coinbase encrypted to miner's own keypair
- [ ] RPC responds to blockchain queries

## Wallet Nodes

- [ ] Container starts and stays running (no crash loops)
- [ ] Secret imported from declared config (not random, not empty)
- [ ] At least one secret loaded (wallet secrets returns non-empty)
- [ ] P2P connects to observer and mining nodes
- [ ] Syncs blocks (local chain height > 0)
- [ ] Scan finds coinbase outputs (wallet scan produces output)
- [ ] Balance shows DRKW (wallet balance returns non-zero)
- [ ] Wallet address matches declared key

## Key Management Critical Path

- [ ] Single source of truth for keys (keys.toml)
- [ ] Mining node keypair matches declared key (not random)
- [ ] Wallet secret matches declared key (same source)
- [ ] Miner public key == wallet public key
- [ ] AccountManager imports declared key on first boot
- [ ] AccountManager persists state across restarts
- [ ] Key selection works: set_default changes miner coinbase key

## Non-Negotiable Guards

1. No node generates random keys when keys are declared
2. No node starts with zero secrets when keys.toml has entries
3. Pipeline FAILS if miner_public_key != wallet_public_key
4. Pipeline FAILS if wallet balance == 0 after scan
5. Pipeline FAILS if any node crash-loops
