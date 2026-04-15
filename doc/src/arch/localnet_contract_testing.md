# Local DarkFi Smart Contract Testing

## Overview

This guide covers local testing of DarkFi WASM contracts using the unified `ContractTestingPipeline`.

## Architecture

The current system uses a modular pipeline:

```
ContractTestingPipeline
    │
    ├─► BinaryChecker (check_wasm, check_zkbins)
    │
    ├─► GenesisRunner (ensure_genesis)
    │       └── GenesisHarness (NativeToken + Deployooor)
    │
    └─► ContractDeployer (deploy)
```

## Key Concepts

### 1. Genesis = NativeToken + Deployooor ONLY

Genesis sets up the mandatory consensus contracts:
- **Native Token**: Block rewards, fees, burns
- **Deployooor**: WASM contract deployment

### 2. All Other Contracts = User Contracts

Contracts like DEX, Stablecoin, Identity, etc. are **user contracts** deployed via Deployooor AFTER genesis. They are optional and not part of consensus.

### 3. ContractTestingPipeline

The unified pipeline handles:
- Binary status checking (missing/stale/current)
- Genesis setup (if needed)
- Contract deployment

## Quick Start

```rust
use darkfi::Result;
use darkfi_sdk::num_traits::One;
use num_bigint::BigUint;

let config = HarnessConfig {
    pow_target: 20,
    pow_fixed_difficulty: Some(BigUint::one()),
    confirmation_threshold: 1,
    max_forks: 8,
    alice_url: "tcp+tls://127.0.0.1:18440".to_string(),
    bob_url: "tcp+tls://127.0.0.1:18441".to_string(),
};

// One-shot: build + genesis + deploy
let mut pipeline = ContractTestingPipeline::new("dex", config, &ex).await?;
let contract_id = pipeline.ensure_ready_and_deploy().await?;

// Step-by-step
let report = pipeline.status_report().await;
if matches!(report.wasm_status, BinaryStatus::Missing) {
    pipeline.build_if_needed().await?;
}
pipeline.ensure_genesis().await?;
let contract_id = pipeline.deploy().await?;
```

## File Locations

| Component | Path |
|-----------|------|
| GenesisHarness | `bin/darkfid/src/tests/genesis.rs` |
| ContractDeployer | `bin/darkfid/src/tests/deployer.rs` |
| ContractTestingPipeline | `bin/darkfid/src/tests/pipeline.rs` |

## See Also

- [Genesis Harness](./genesis_harness.md) - NativeToken + Deployooor baseline
- [Contract Testing Pipeline](./pipeline.md) - Unified testing workflow
- [Test Harness Guide](./test_harness_guide.md) - Full testing overview
