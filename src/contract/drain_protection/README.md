# DarkWow DrainProtection Contract

Configurable endowment protection with 8 defense mechanisms: graduated tiers,
exit queue, circuit breaker, guardian pause, observation period, split proposals,
no-loss reserve, and dead man's switch.

> **Documentation**: See [DrainProtection Contract](../../../doc/src/contract/drain_protection.md)
> for the full architecture, deployer/member controls, and security considerations.

> **Status (2026-10-25): the member exit is refused.** `ExitV1` returns
> `CustodyModelUnavailable` before any state read: a parent contract cannot verify a member's
> share over the genesis `Purse` (amounts are off-wire, so `derived_purse_id` is the only
> public field), and `ProtectedFund.members` has no identity or population path. The fund is a
> governance wrapper over externally-held notes, not a custodian (`OBL-C271`).
