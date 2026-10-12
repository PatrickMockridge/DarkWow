# Native token

> **Rewritten 2026-10-12 (`F-603`).** This page was an interactive-shell guide — a `dww>` prompt,
> `alias add`/`alias show`, `token generate-mint`/`token mint`/`token freeze`/`token list`, mint
> authorities, freeze heights and per-token aliases. **None of that exists.** `dwow_wallet` is a
> one-shot command-line binary with no shell, `WalletSubcmd` and `ContractSubcmd` have no `token`
> or `alias` variant between them, and DarkWow has no mint-authority or freeze model at all: there
> are no coins, and a token is a **promissory-note type** whose supply is whatever has been issued
> against it. What follows is the real workflow. See
> [promissory_note.md](../contract/promissory_note.md) for the model and
> [../contract/native_token.md](../contract/native_token.md) for DRKW itself.

## Getting DRKW

`DRKW` is the native token and pays transaction fees. There are two ways to get it: mine a block
that gets confirmed, or be sent some.

Mining is covered in [node.md](node.md). If someone sends you DRKW, ask them to add the
**`--half-split`** flag to their transfer, which splits the output into two notes rather than one —
a wallet with two notes can spend one while the other is still being scanned. That is the whole of
the advice the old page wrapped in an alias table.

## Checking your balance

```shell
dwow_wallet wallet balance
```

The wallet reports what it has **scanned**: a balance is the sum of the unspent notes the wallet
holds for a given asset id, so a wallet that has not scanned the chain reports zero. Run
`dwow_wallet scan` (or `dwow_wallet scan --reset <height>`) first if you expect a balance and see
none.

`--porcelain` prints the same data in a machine-readable form:

```shell
dwow_wallet wallet balance --porcelain
```

## Sending

```shell
dwow_wallet transfer <amount> <asset-id> <recipient> [--spend-hook <id>] [--user-data <hex>] [--half-split]
```

`asset-id` is the field element identifying the token — for DRKW it is the native token's id. There
is no alias layer: the id is the name. `dwow_wallet position` lists the capabilities the wallet
holds, which is the closest thing to a portfolio view.

## Token types

A new token is a **registered promissory-note type**, issued once with `RegisterTypeV1 (0x00)` on
the promissory-note contract and then minted with `IssueV1 (0x02)`. Both go through
`dwow_wallet contract invoke`:

```shell
dwow_wallet contract invoke <promissory-note-contract-id> register_type
dwow_wallet contract invoke <promissory-note-contract-id> issue
```

See [promissory_note.md](../contract/promissory_note.md) for the parameters, the five circuits and
what each entrypoint enforces. There is no freeze: a type's supply grows by issuance and nothing
retires it, and the closest thing to a cap is the discipline of whoever holds the issuing
capability.
