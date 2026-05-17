"""Simulation configuration."""

from dataclasses import dataclass, field


@dataclass
class SimConfig:
    """Default simulation parameters matching DarkWow bridge/endowment constants."""

    # Network
    blocks_to_simulate: int = 10_000
    block_time_secs: float = 30.0

    # Bridge contract
    withdrawal_timeout_blocks: int = 100
    min_withdrawal_amount: int = 100_000  # 0.1 DAI equivalent
    max_withdrawal_amount: int = 1_000_000_000  # 1000 DAI

    # Relayer endowment
    min_deploy: int = 1_000_000  # 1 DAI
    bp_precision: int = 10_000
    default_backer_cut_bp: int = 5000  # 50%

    # Relayer node
    relayer_poll_interval_blocks: int = 2
    fee_settlement_interval_blocks: int = 50
    slash_amount: int = 1_000_000  # 1 DAI
    standard_fee_bp: int = 100  # 1%
    guaranteed_premium_bp: int = 500  # 5%
    max_concurrent_withdrawals: int = 10
    coverage_ratio: float = 1.5  # stake must be 1.5x active withdrawals

    # Agents
    num_relayers: int = 3
    num_backers: int = 10
    num_users: int = 50

    # Capital
    initial_relayer_stake: int = 100_000_000  # 100 DAI
    initial_backer_capital: int = 50_000_000  # 50 DAI per backer
    initial_user_balance: int = 10_000_000  # 10 DAI per user

    # External chain
    external_chain_latency_blocks: int = 3  # blocks for external tx to confirm
    external_chain_failure_rate: float = 0.0  # base failure rate

    # Derived
    @property
    def total_blocks(self) -> int:
        return self.blocks_to_simulate


# Overridable per-scenario
def with_overrides(base: SimConfig, **kwargs) -> SimConfig:
    """Create a new config with specific overrides for a scenario."""
    d = {k: v for k, v in base.__dict__.items() if not k.startswith("_")}
    d.update(kwargs)
    return SimConfig(**d)
