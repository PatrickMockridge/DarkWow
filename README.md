# DarkWow

A DarkFi fork rebuilt around **four rejections of upstream**:

1. **No governance DAO** — pure PoW, no token-holder voting
2. **No overlay/diff consensus** — deterministic Uncle Merkle with stateless verification
3. **LessThanOrEqual and BaseDiv op codes proven sound in Lean4** and activated on this fork
4. **No premine** — every coin mined

Zero vendor lock-in. Genesis is two contracts — NativeToken and Deployooor. Hard forks are a feature, not a threat. Extended and entirely voluntary smart contract feature set including but not limited to: Darktoshi Dice, DAO (with Escrow and Drain protection), DEX, Stablecoin, Prediction Market, Betting Stake, Identity, Sealed Bidding/Tendering, Labor Market and more.

<!-- TODO: website placeholder — https://darkwow.org -->

Development occurs on the **`linear-master`** branch.

---

## Four Refutations in Detail

### 1. No Governance DAO — Pure PoW

Upstream DarkFi's DAO can freeze the native token through token-holder voting. If governance can restrict minting, miners may not get paid and PoW consensus becomes attackable by wealthy token holders. DarkWow removes all governance control over the native token. There is no DAO that can freeze consensus-critical functions.

### 2. Uncle Merkle Consensus — No Overlay/Diff

Upstream's overlay/diff consensus uses speculative state (checkpoint → commit → rollback), where `diff()` computation depends on sequence history — the same code produces different results depending on timing. This makes deterministic testing effectively impossible.

DarkWow replaces this entirely with **Uncle Merkle consensus**: the canonical chain with the most accumulated work obligates offering uncle chains a one-time option to form a side chain and share the PoW reward. Stateless verification via pure merkle proof — no overlay needed. Same block always produces the same result.

See [Uncle Merkle Consensus](doc/src/arch/consensus/uncle_merkle.md) for the full specification.

### 3. ZK Opcodes — Formally Verified in Lean4

The `LessThanOrEqual` and `BaseDiv` opcodes deployed in DarkWow's zkVM have been formally proven sound using the Lean4 proof assistant. Unlike upstream's experimental opcodes that relied on empirical testing alone, these opcodes carry machine-checkable proofs of correctness.

See [Opcodes and Formal Verification](doc/src/arch/zk/opcodes.md) for details.

### 4. No Premine — Every Coin Mined

Upstream DarkFi distributed tokens at genesis to early investors, team members, and SAFT participants — creating concentrated whale dominance. DarkWow has zero pre-mined tokens. Every unit of the native token is earned through RandomX Proof-of-Work mining, following a continuous exponential decay schedule with a permanent 1% tail emission, converging asymptotically toward a 21 million supply cap.

See [Blockchain Rewards](src/sdk/src/blockchain.rs) for the reward schedule constants.

---

## Privacy Architecture: O-Cap Authorization

**Traditional blockchain asks**: "WHO has access?" — public keys link transactions to identities.

**DarkWow asks**: "Can you PROVE you have access?" — identity never revealed, only capabilities proven. O-Cap (Object Capability) = authorization without revelation.

```
Alice proves: "I am a verified smart contract auditor"
Verifier learns: ✓ Alice can audit
Verifier DOES NOT learn: Alice's public key, balance, or identity
```

---

## Smart Contracts

### Identity & Authorization
- **Identity**: O-Cap primitives, credentials, DAG-based competency claims

### Finance
- **Native Token**: Consensus-first native token (block rewards, fees, transfers)
- **Stablecoin**: Synthetix-style pooled debt model
- **DEX**: Privacy-preserving decentralized exchange
- **Escrow**: Conditional value escrow
- **DAO-Escrow**: Voluntary DAO-governed endowment (opt-in, ZK predicate-based)
- **Subscription**: Recurring payment streams
- **Bridge**: Cross-chain transfers

### Labor & Tendering
- **Labor Market**: Job posting and acceptance with O-Cap
- **Tender**: Sealed-bid procurement with O-Cap
- **Attestation**: Generalized claims and evidence verification

### Risk & Gaming
- **Insurance Market**: Underwriting and coverage with O-Cap
- **Prediction Market**: Risk probability pricing
- **Darktoshi Dice**, **Baccarat**, **Roulette**, **Slot**: Privacy-preserving casino games
- **Lottery**: Configurable lottery combining BettingStake and Insurance
- **DarkBet Exchange**: Unified betting with order-book and AMM modes

### Infrastructure
- **Oracle**: Push-model oracle with attestation
- **Auction**: Privacy-preserving auctions
- **Atomic Swap**: Cross-chain swaps

---

## Build

```shell
git clone https://codeberg.org/PatrickM123/darkfi-jailbroken
cd darkfi-jailbroken
rustup target add wasm32-unknown-unknown
make
```

Minimum Rust version: **1.87.0**.

### Running a Node

```bash
# Local development (single node)
cargo run -p darkfid -- --network linear-testnet

# DarkWow public testnet
cargo run -p darkfid -- --network darkwow-testnet
```

---

## Documentation

- [Architecture Overview](doc/src/arch/overview.md)
- [Uncle Merkle Consensus](doc/src/arch/consensus/consensus.md)
- [O-Cap Authorization](doc/src/arch/ocap.md)
- [Opcodes & Formal Verification](doc/src/arch/zk/opcodes.md)
- [Contract Development Guide](doc/src/dev/contracts.md)

---

## Security Status

All contracts are **EXPERIMENTAL** and **UNAUDITED**. Known issues are documented in [Security Analysis](doc/src/arch/security-analysis.md).

---

## License

AGPL-3.0-only. See [LICENSE.md](LICENSE.md).

DarkWow is a tool for people and nations to establish sovereignty according to human rights law. See the [UN Declaration on the Rights of Indigenous Peoples](https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf).

---

**Go Dark.**
