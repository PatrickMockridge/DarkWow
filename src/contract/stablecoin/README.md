# DarkWow Stablecoin Contract

A privacy-preserving collateralized stablecoin for DarkWow with configurable
models (PooledDebt, Liquity, Fractional, IndividualCdp) and multi-collateral support.

> **Documentation**: See [Stablecoin Contract](../../../doc/src/contract/stablecoin.md)
> for the full architecture, hot/cold circuit separation, and opcode status.

> **Status (2026-10-25): the custody-dependent arms are refused.** `OpenPositionV1` (0x01),
> `AddCollateralV1` (0x02), `MintStableV1` (0x04), `LiquidateV1` (0x06) and
> `RedeemStableV1` (0x0A) return `CustodyModelUnavailable` before any state read: the
> contract does not custody collateral, and the parent cannot check a PN deposit's recipient.
> The market is inert until a parent-checkable custody primitive exists (`OBL-C92`, `C273`;
> decision `OBL-C272`).
