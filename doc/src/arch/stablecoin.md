Monero-Collateralized Stablecoin on DarkFi (DRAFT)
===================================================

*This section proposes a privacy-preserving stablecoin collateralized by Monero (XMR),
utilizing DarkFi's native token (DRK) and the universal bridge architecture.*

## Motivation: Why Monero Collateral?

Unlike Ethereum-based CDPs (MakerDAO) that use ETH or WBTC as collateral,
a Monero-collateralized stablecoin offers:

1. **Privacy-native collateral**: Monero already provides transaction privacy.
   Using it as collateral maintains the privacy story end-to-end.
2. **Censorship resistance**: Monero's adaptive block size and privacy features
   make it resistant to censorship compared to transparent blockchains.
3. **Deep liquidity**: XMR has established markets and liquidity pools.

## Two Paths: Native XMR vs Wrapped XMR

**Option A: Native XMR as Collateral**

```
User locks XMR directly → DarkFi CDP Engine holds XMR → Issues stablecoin
```

- Requires bridging XMR to DarkFi (atomic swap or bridge)
- DarkFi must support Monero's privacy model natively
- Complex: Monero uses ring signatures, not simple UTXOs

**Option B: Wrapped XMR (wXMR) as Collateral**

```
Monero → Atomic Swap → DRK/XMR LP token → Use as CDP collateral
```

- More practical in near term
- DRK/XMR pool provides:
  - Liquidity for the swap
  - TWAP price feed (P2P Oracle)
  - LP tokens as collateral itself

## The P2P Oracle: Data Bridging for Price Feeds

*Critical insight: A CDP stablecoin doesn't just need asset bridging—it needs data bridging.*

A collateralized stablecoin requires:

| Data Type | Purpose | Source |
|-----------|---------|--------|
| **Price feed** | Collateral valuation | AMM TWAP (DRK/XMR pool) |
| **TWAP window** | Manipulation resistance | Time-weighted average |
| **Redemption rate** | Stability control | PI Controller |
| **Liquidation threshold** | Safety margin | Contract parameter |

The **P2P Oracle** design uses the DRK/XMR AMM pool itself as the price oracle:

```
NETHER (stablecoin) / DRK (collateral) pool → TWAP → PI Controller → Redemption Rate
```

This is **data bridging**: passing external chain state (AMM reserves, prices)
into DarkFi for computation, without transferring value.

## Architecture: Bridged Stablecoin + Universal Bridge

```
┌─────────────────────────────────────────────────────────────────┐
│                     Universal OCap Bridge                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │ Asset Bridge │    │ Data Bridge  │    │ Compute Bridge│       │
│  │ (value xfer) │    │ (price feeds)│    │ (ZK proofs)  │       │
│  └──────────────┘    └──────────────┘    └──────────────┘       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│              Monero-Collateralized Stablecoin                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │  CDP Engine  │    │ PI Controller │    │  AMM Pool    │       │
│  │ (positions)  │    │ (rate adjust) │    │ (TWAP feed)  │       │
│  └──────────────┘    └──────────────┘    └──────────────┘       │
│                                                                 │
│  Collateral: DRK/XMR LP tokens (via atomic swap or bridge)       │
│  Stablecoin: NETHER (USD-pegged, privacy-preserving)             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Flow: Creating a Monero-Collateralized Position

```
1. ATOMIC SWAP (Asset Bridge)
   User sends XMR → Receives DRK/XMR LP tokens
   (Trustless atomic swap, no intermediary)

2. CDP OPEN (Universal Bridge → CDP Engine)
   User deposits LP tokens as collateral
   Commitment: C = H(secret, collateral, debt, owner)
   ZK Proof: collateral >= minimum, ratio valid

3. DATA BRIDGE (Price Feed)
   AMM pool reports reserves to DarkFi
   TWAP calculated: TWAP(DRK/XMR) over 1 hour window
   This TWAP used to value collateral

4. STABLECOIN MINT
   CDP Engine mints NETHER (stablecoin) to user
   Debt = NETHER minted

5. ONGOING: PI CONTROLLER (Data Bridge)
   If TWAP > $1 (premium):
     Redemption rate increases
     Borrowing more expensive → less minting → push TWAP down
   If TWAP < $1 (discount):
     Redemption rate decreases
     Borrowing cheaper → more minting → push TWAP up
```

## Why Data Bridging is Essential

Traditional CDP systems (MakerDAO) use **oracle networks** (Chainlink) to fetch prices.
This creates:

- **Centralization risk**: Single point of failure
- **Manipulation risk**: Oracles can be attacked
- **Censorship risk**: Oracle operators can refuse to serve

The P2P Oracle approach **eliminates these risks** by using:

1. **AMM TWAP**: The pool's own constant-product formula provides price discovery
2. **Time-averaging**: TWAP naturally smooths manipulation attempts
3. **Data bridging**: Price data flows through the bridge as **information**, not **value**

```
Comparison:

Oracle Model:     External Chain → Oracle Network → Price Feed
                                     ↑
                              (centralized,
                               can be censored)

P2P Oracle Model:  External Chain → AMM Pool → TWAP → Data Bridge → DarkFi
                                     ↑
                              (decentralized,
                               manipulation-resistant)
```

## Relationship to Universal OCap Bridge

The universal bridge architecture supports:

| Bridge Type | Capability | Stablecoin Use |
|------------|------------|----------------|
| **Asset Bridge** | Transfer value between chains | Lock collateral, mint/burn stablecoin |
| **Data Bridge** | Pass state/information across chains | TWAP price feeds, reserve data |
| **Compute Bridge** | Verify proofs computed elsewhere | ZK circuit verification |

The **OCap security model** ensures:

- **No VSS shards to steal**: User's collateral is self-sovereign
- **Self-signed withdrawals**: User alone authorizes via their secret
- **Censorship resistant**: No threshold can block valid operations

## Multi-Chain Collateral Support

The bridge now supports **multiple external chains**, enabling diverse collateral options:

| Chain | Token | Bridge Status | Stablecoin Use |
|-------|-------|---------------|----------------|
| **Ethereum** | ETH | Implemented | Native collateral |
| **Monero** | XMR | Implemented | Privacy-native collateral |
| **Zcash** | ZEC | Implemented | Shielded collateral |
| **Aztec** | ETH/DAI | Implemented | Private DeFi collateral |
| **Litecoin** | LTC | Implemented | Trade pair collateral |

This means users can collateralize the stablecoin with **any bridged asset**, enabling:
- XMR-backed privacy-preserving debt positions
- ETH-backed stablecoin minting
- DAI-backed positions via Aztec (private DAI!)
- LTC-backed positions (the Monero trade pair)

### Bridged DAI as Price Anchor

**Bridged DAI (via Aztec)** serves as a critical price anchor:

```
┌─────────────────────────────────────────────────────────────────┐
│              Bridged DAI Price Anchoring                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  External DAI/USD price → Aztec private pool → DarkFi TWAP     │
│                              ↓                                    │
│  Stablecoin redemption rate adjusted by PI Controller            │
│                              ↓                                    │
│  Keeps NETHER/USD price stable                                    │
│                                                                   │
│  Why DAI?                                                         │
│  - DAI is pegged to USD (softly)                                 │
│  - Aztec provides private DAI transfers                           │
│  - DarkFi can observe DAI/USD without revealing users             │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

The **price signal flow**:
1. External price data (DAI/USD, XMR/USD) enters via bridge data
2. AMM pools on DarkFi provide TWAP price feeds
3. PI Controller adjusts redemption rates based on TWAP deviation
4. Users mint/burn to arbitrage the stablecoin back to peg

This creates **price signals in and out of DarkFi**:
- **Into DarkFi**: External asset prices inform collateral valuation
- **Out of DarkFi**: NETHER price influences external markets via redemption

## Open Questions

## Open Questions

1. **Atomic swap feasibility**: Can trustless XMR ↔ DRK swaps be implemented?
2. **LP token stability**: Are DRK/XMR LP tokens suitable collateral?
3. **Native XMR support**: Should DarkFi eventually support XMR natively?
4. **PI Controller tuning**: What Kp/Ki values produce stable redemption rates?

## References

- [P2P Oracle Design](https://technologytruth.substack.com/p/nether-say-nether-again)
- [DarkFi Stablecoin Contract](../../src/contract/stablecoin/)
- [Anonymous Bridge](./bridge.md)
- [Atomic Swaps](../protocol/atomic-swap.md)