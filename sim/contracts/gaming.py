"""Gaming contracts — all follow the commit→reveal→settle pattern.

Covers: DarktoshiDice, Roulette, Slot, Baccarat, Lottery, GameRoom, BettingStake.

The commit-reveal-settle pattern is the uniform lifecycle across all games:
    1. Player commits to a bet (hides bet details behind a commitment)
    2. Outcome is revealed (block hash entropy)
    3. Bet is settled (payout or loss)

This shared pattern is what makes the games composable with BettingStake
for capital pooling.
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional

from sim.contract import (
    ANYONE, HOUSE, PLAYER, AuthError, Caller, ConstraintError, Contract,
)
from sim.state import StateMachine


# -- Shared game state machine --

class BetState(Enum):
    COMMITTED = "Committed"
    REVEALED = "Revealed"
    SETTLED_PLAYER = "SettledPlayer"
    SETTLED_HOUSE = "SettledHouse"
    CANCELLED = "Cancelled"


@dataclass
class Bet:
    bet_id: str
    player: str
    amount: int
    state: BetState = BetState.COMMITTED
    metadata: dict = field(default_factory=dict)


class GameBase(Contract):
    """Base class for all gaming contracts with commit-reveal-settle pattern."""

    name = "game_base"

    def __init__(self):
        super().__init__()
        self.bets: Dict[str, Bet] = {}
        self.house_pubkey: str = ""
        self.house_edge_bps: int = 0  # house edge in basis points
        self._bet_counter: int = 0

    def initialize(self, caller: Caller, house_pubkey: str, house_edge_bps: int = 200):
        """Initialize the game table. House only."""
        self.only(caller, HOUSE)
        self.house_pubkey = house_pubkey
        self.house_edge_bps = house_edge_bps

    def commit_bet(self, caller: Caller, amount: int, **bet_meta) -> str:
        """Player commits to a bet. Amount and type hidden behind commitment."""
        self.only(caller, PLAYER)
        if amount <= 0:
            raise ConstraintError("Bet amount must be positive")
        self._bet_counter += 1
        bid = f"bet-{self._bet_counter}"
        self.bets[bid] = Bet(bid, caller.name, amount, BetState.COMMITTED, bet_meta)
        return bid

    def reveal(self, bet_id: str, entropy: int):
        """Reveal outcome. In real contracts this uses block hash entropy."""
        bet = self._get_bet(bet_id)
        if bet.state != BetState.COMMITTED:
            raise ConstraintError(f"Bet is {bet.state.value}, not Committed")
        bet.state = BetState.REVEALED
        bet.metadata["entropy"] = entropy

    def settle(self, bet_id: str, won: bool, payout: int):
        """Settle bet — payout to player or house keeps the stake."""
        bet = self._get_bet(bet_id)
        if bet.state != BetState.REVEALED:
            raise ConstraintError(f"Bet is {bet.state.value}, not Revealed")
        bet.state = BetState.SETTLED_PLAYER if won else BetState.SETTLED_HOUSE
        bet.metadata["payout"] = payout if won else 0

    def house_close(self, caller: Caller, bet_id: str):
        """House closes an abandoned/timed-out bet."""
        self.only(caller, HOUSE)
        bet = self._get_bet(bet_id)
        if bet.state not in (BetState.COMMITTED, BetState.REVEALED):
            raise ConstraintError(f"Cannot close bet in state {bet.state.value}")
        bet.state = BetState.CANCELLED

    def _get_bet(self, bet_id: str) -> Bet:
        if bet_id not in self.bets:
            raise ConstraintError(f"Bet '{bet_id}' not found")
        return self.bets[bet_id]


# -- Specific game implementations --

class DarktoshiDice(GameBase):
    """Satoshi Dice clone — player bets on a roll being above a target."""
    name = "darktoshi_dice"


class Roulette(GameBase):
    """European (37) or American (38) roulette with configurable bet types."""
    name = "roulette"

    def __init__(self):
        super().__init__()
        self.wheel_type: str = "european"  # 37 numbers
        self.bet_types: Dict[str, int] = {  # bet type → payout multiplier
            "straight": 35,
            "split": 17,
            "street": 11,
            "corner": 8,
            "sixline": 5,
            "dozen": 2,
            "column": 2,
            "evenmoney": 1,
        }


class Slot(GameBase):
    """Slot machine — spinning reels with configurable paytables."""
    name = "slot"

    def __init__(self):
        super().__init__()
        self.reels: int = 3
        self.paylines: int = 1
        self.paytable: Dict[str, int] = {}


class Baccarat(GameBase):
    """Punto Banco — bet on Player, Banker, or Tie."""
    name = "baccarat"

    def __init__(self):
        super().__init__()
        self.payouts: Dict[str, float] = {"player": 1.0, "banker": 0.95, "tie": 8.0}


class Lottery(GameBase):
    """Pooled lottery with configurable picks and prize tiers."""
    name = "lottery"

    def __init__(self):
        super().__init__()
        self.ticket_count: int = 0
        self.prize_tiers: List[dict] = []
        self.winning_numbers: List[int] = []
        self.drawn: bool = False

    def draw_winners(self, caller: Caller, numbers: List[int]):
        """House or anyone draws winning numbers."""
        if self.drawn:
            raise ConstraintError("Already drawn")
        self.drawn = True
        self.winning_numbers = numbers


class GameRoom(GameBase):
    """Generalized pot management for poker-style games."""
    name = "game_room"

    def __init__(self):
        super().__init__()
        self.pots: Dict[str, dict] = {}
        self.room_state: str = "Open"

    def create_room(self, caller: Caller, room_config: dict) -> str:
        rid = f"room-{self._bet_counter}"
        self.pots[rid] = {"config": room_config, "players": {}, "state": "Open"}
        return rid

    def close_pot(self, caller: Caller, room_id: str):
        self.only(caller, HOUSE)
        self.pots[room_id]["state"] = "Closed"

    def settle_pot(self, caller: Caller, room_id: str, payouts: Dict[str, int]):
        self.only(caller, HOUSE)
        if self.pots[room_id]["state"] != "Closed":
            raise ConstraintError("Pot must be closed before settling")
        self.pots[room_id]["state"] = "Settled"


# -- BettingStake — capital pooling layer for games --

class BettingStake(Contract):
    """Capital providers stake against betting tables for a share of house edge."""
    name = "betting_stake"

    def __init__(self):
        super().__init__()
        self.tables: Dict[str, dict] = {}  # table_id → config
        self.stakes: Dict[str, dict] = {}  # stake_id → stake info

    def initialize_table(self, caller: Caller, table_id: str, house_edge_bps: int):
        self.only(caller, HOUSE)
        self.tables[table_id] = {"edge_bps": house_edge_bps, "total_staked": 0, "active": True}

    def stake(self, caller: Caller, table_id: str, amount: int) -> str:
        if table_id not in self.tables:
            raise ConstraintError(f"Table '{table_id}' not found")
        if not self.tables[table_id]["active"]:
            raise ConstraintError("Table is inactive")
        sid = f"stake-{caller.name}-{table_id}"
        self.stakes[sid] = {"provider": caller.name, "table": table_id,
                            "amount": amount, "active": True, "earnings": 0}
        self.tables[table_id]["total_staked"] += amount
        return sid

    def unstake(self, caller: Caller, stake_id: str):
        stake = self.stakes.get(stake_id)
        if stake is None or stake["provider"] != caller.name:
            raise AuthError(f"Not your stake: {stake_id}")
        if not stake["active"]:
            raise ConstraintError("Stake already inactive")
        stake["active"] = False

    def claim_earnings(self, caller: Caller, stake_id: str):
        stake = self.stakes.get(stake_id)
        if stake is None or stake["provider"] != caller.name:
            raise AuthError(f"Not your stake: {stake_id}")
        if not stake["active"]:
            raise ConstraintError("Stake is inactive")

    def update_risk(self, caller: Caller, table_id: str, payout: int):
        """Betting contract updates risk after a payout."""
        self.only(caller, HOUSE)
        if table_id not in self.tables:
            raise ConstraintError(f"Table '{table_id}' not found")
