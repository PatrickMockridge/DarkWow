# amm-darkfi Port Matrix

This note captures the first concrete fork-exit target: `amm-darkfi`.

## Goal

Make `amm-darkfi` target official DarkWow plus external shared packages, rather than a
fork-era DarkWow line.

## Current State

As of March 27, 2026:

- `amm-darkfi` already uses local path dependencies, not the old hosted fork remote
- it still imports fork-era AMM interfaces from the local `darkfi` checkout
- its canonical arithmetic remains plain Rust helper logic, not `zkas` template use
- the direct AMM model surface now has an external replacement in
  `darkfi-amm-types`

## Current Arithmetic Surface

The direct AMM core currently computes canonical arithmetic locally:

- `floor_div_u128`
- `floor_sqrt_u128`
- `lp0.min(lp1)`

That means safemath adoption will be a semantic and testing migration first, not a
simple import rewrite. The immediate compatible target is the stock safemath track:
host helpers plus two-`u64`-limb templates.

## Imported Fork-Era Surface

The main direct-pool core imports these DarkWow-side exchange modules:

- `AmmExternalInterfaceContractV1`
- `AmmPoolAddLiquidityTransitionV1`
- `AmmPoolConfigV1`
- `AmmPoolFunctionV1`
- `AmmPoolIndexV1`
- `AmmPoolInitializeTransitionV1`
- `AmmPoolRemoveLiquidityTransitionV1`
- `AmmPoolStateV1`
- `AmmPoolSwapExactInTransitionV1`
- `AmmSettlementModeV1`
- `AMM_EXTERNAL_INTERFACE_VERSION_V1`

Contract and tool paths also consume:

- AMM payload helpers like `encode_amm_pool_*` and `decode_amm_pool_*`
- intent-settlement payload helpers in the narrower solver / settlement paths

## Official Master Gap

The clean official DarkWow master worktree from March 27, 2026 does not currently expose:

- `intent`
- `intent_set`
- `amm_pool`
- `amm_external_interface`
- `transition_payload`

So `amm-darkfi` cannot be retargeted to official master by changing dependency paths
alone.

## Direct AMM Replacement

The direct-pool portion of the missing fork surface now exists externally in
`darkfi-amm-types`.

That crate currently provides:

- `AmmPoolConfigV1`
- `AmmPoolStateV1`
- `AmmPoolIndexV1`
- `AmmPoolFunctionV1`
- `AmmPool{Initialize,AddLiquidity,RemoveLiquidity,SwapExactIn}TransitionV1`
- `AmmExternalInterfaceContractV1`
- `AmmSettlementModeV1`
- `encode_amm_pool_*` / `decode_amm_pool_*` payload helpers

It builds against official `darkfi-sdk`, so the direct AMM path no longer needs those
types to live inside a DarkWow fork.

## Port Actions

1. Keep arithmetic semantics externalized here.
   `darkfi-safemath-zk` is the shared relation catalog.

2. Decide where AMM transition and payload types belong.
   For the direct path, that answer now exists:
   - `darkfi-amm-types` is the external replacement
   For the intent-settlement path, the remaining options are:
   - upstream intent / intent-set modules into DarkWow proper
   - extract them into a second external exchange-types package

3. Replace implicit arithmetic conventions with explicit shared semantics.
   For `amm-darkfi`, the first replacements should be:
   - `floor_div_u128`
   - `floor_sqrt_u128`
   - liquidity-leg ordering / `min` conventions
   The flagship stock-compatible template path for those relations is:
   - `div_floor_u128_by_u64_to_u64_v1`
   - `sqrt_floor_u128_v1`
   - `cross_mul_lte_u64_v1`
   - `cross_mul_gte_u64_v1`
   Helper-only stock templates still exist for limb assertions / limb compares
   and for the current branchy `min_select_u128_2x64_v1` relation.

4. Port tests before porting contract code.
   The quickest confidence path is to bind `amm-darkfi` transition vectors to the
   shared safemath templates and fixture logic first.

5. Only then switch the AMM workspace off the fork-era DarkWow line.
   For the direct path, the target dependency set is now:
   - official `darkfi-sdk`
   - `darkfi-amm-types`
   - `darkfi-safemath-zk`

## Current Harness Blocker

An attempted direct proof harness that pulled:

- the current `amm-darkfi` workspace
- and a clean official DarkWow worktree

into one Cargo graph hit package-collision failures on shared DarkWow path crates like:

- `darkfi`
- `darkfi-derive`

That is itself useful evidence for the exit plan:

- the current AMM workspace still inherits the fork-era local DarkWow tree deeply enough
  that side-by-side verification against official-master cannot be done inside one Cargo
  lockfile yet
- so the first executable compatibility harness lives outside `amm-darkfi`, in this repo,
  using standalone AMM vectors rather than direct crate linking

## Immediate Follow-Up

The next concrete coding task should be:

- retarget the direct-path `amm-darkfi` imports from fork `darkfi_sdk::crypto::*`
  to `darkfi_amm_types::*`
- leave the intent / CoW path on the old surface temporarily
- then bind the direct AMM transition vectors to the stock-compatible safemath track:
  - `div_floor_u128_by_u64_to_u64_v1`
  - `sqrt_floor_u128_v1`
  - `cross_mul_lte_u64_v1`
  - helper templates where the full vector still needs them, such as
    `min_select_u128_2x64_v1`

That yields a real direct-path fork exit before the broader intent-settlement port.
