#!/usr/bin/env python3
"""DarkWow Block Explorer — general-purpose CLI for querying any node.

Queries a DarkWow JSON-RPC endpoint and presents raw block data. Works with
local dockernet, public testnet, and future mainnet.

Usage:
  python3 explorer.py [--host HOST] [--port PORT] <command> [args]

Commands:
  height              Get current block height
  block <H>           Get block at height H
  supply              Cumulative supply audit (Pedersen chain verification)
  scan <FROM> <TO>    Scan blocks FROM..TO, show uncle and supply data
  target              Get current PoW target

Examples:
  python3 explorer.py height
  python3 explorer.py block 5
  python3 explorer.py --host lilith0.dark.fi --port 31345 scan 1 100

Output: plain text tables — no web server, no dependencies beyond Python stdlib.
"""

import json
import socket
import struct
import argparse
import sys
import textwrap
from typing import Any, Dict, List, Optional, Tuple


# ============================================================================
# JSON-RPC Client
# ============================================================================

class DarkWowRPC:
    """Minimal JSON-RPC client over raw TCP sockets. Zero dependencies."""

    def __init__(self, host: str = "127.0.0.1", port: int = 31345, timeout: int = 5):
        self.host = host
        self.port = port
        self.timeout = timeout
        self._id = 0

    def _call(self, method: str, params: List[Any] = None) -> Dict[str, Any]:
        """Make a JSON-RPC call and return the result dict."""
        self._id += 1
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params or [],
            "id": self._id,
        }
        payload = json.dumps(request) + "\n"

        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(self.timeout)
        try:
            sock.connect((self.host, self.port))
            sock.sendall(payload.encode())
            response = b""
            while True:
                try:
                    chunk = sock.recv(4096)
                    if not chunk:
                        break
                    response += chunk
                    if b"\n" in response:
                        break
                except socket.timeout:
                    break
        finally:
            sock.close()

        if not response:
            return {"error": "no response"}

        data = json.loads(response.decode().strip())
        if "error" in data:
            return {"error": data["error"]}
        return data.get("result", {})

    def get_height(self) -> int:
        return self._call("blockchain.get_height", []).get("height", 0)

    def get_target(self) -> int:
        return self._call("blockchain.get_target", []).get("target", 0)

    def get_block(self, height: int) -> Optional[Dict[str, Any]]:
        result = self._call("blockchain.get_block_linear", [float(height)])
        if isinstance(result, str):
            return json.loads(result)
        return None

    def get_cumulative_supply(self) -> Dict[str, Any]:
        return self._call("blockchain.get_cumulative_supply", [])


# ============================================================================
# Emission Schedule (standalone, no sim dependency)
#
# WARNING: These constants MUST match src/sdk/src/blockchain.rs exactly.
# If the emission schedule changes (hard fork, parameter update), update
# both files together. This file has no mechanism to auto-sync with Rust.
# ============================================================================

INITIAL_REWARD = 1_383_764_049
HALF_LIFE_BLOCKS = 1_051_920
TAIL_REWARD = 79_853_981
DECAY_FP = 4_294_967_296  # 2^32


def expected_reward(height: int) -> int:
    """Block reward at height H (integer-only fixed-point)."""
    if height == 0:
        return 0
    if height > HALF_LIFE_BLOCKS:
        return TAIL_REWARD
    h = (height - 1)
    numerator = INITIAL_REWARD - TAIL_REWARD
    decay = (DECAY_FP * h) // HALF_LIFE_BLOCKS
    pre_reward = (numerator * (DECAY_FP - decay)) // DECAY_FP
    return TAIL_REWARD + pre_reward


def expected_cumulative_supply(height: int) -> int:
    """Sum of expected_reward(1..H)."""
    total = 0
    for h in range(1, height + 1):
        total += expected_reward(h)
    return total


# ============================================================================
# Display Utilities
# ============================================================================

def fmt_hash(h: List[int]) -> str:
    """Format a 32-byte hex hash from JSON int array."""
    if all(b == 0 for b in h):
        return "0000000000000000 (zero)"
    bs = bytes(h[:8])
    return bs.hex()


def is_zero_hash(h: List[int]) -> bool:
    return all(b == 0 for b in h)


def border(char: str = "=", width: int = 70) -> str:
    return char * width


def print_block_header(h: Dict[str, Any], rpc: DarkWowRPC):
    """Pretty-print a block header."""
    print(f"  Height:           {h['height']}")
    print(f"  Version:          {h['version']}")
    print(f"  Timestamp:        {h['timestamp']}")
    print(f"  Target:           {h['target']:#x}")
    print(f"  Nonce:            {h['nonce']}")
    print(f"  Previous:         {fmt_hash(h['previous'])}")
    print(f"  Merkle Root:      {fmt_hash(h['merkle_root'])}")

    # Uncle data
    uncle_root = h.get("uncle_merkle_root", [0]*32)
    has_uncles = not is_zero_hash(uncle_root)
    print(f"  Uncle Root:       {fmt_hash(uncle_root)} {'<-- UNCLES PRESENT' if has_uncles else '(no uncles)'}")
    print(f"  Total Reward:     {h.get('total_reward', 'N/A')}")

    # Finality
    print(f"  Commitment Root:  {fmt_hash(h.get('commitment_merkle_root', [0]*32))}")
    print(f"  Nullifier Root:   {fmt_hash(h.get('nullifier_root', [0]*32))}")
    print(f"  Anchor TX:        {fmt_hash(h.get('anchor_tx_id', [0]*32))}")
    print(f"  Anchor Monero H:  {h.get('anchor_monero_height', 0)}")
    print(f"  Finality Flags:   {h.get('finality_flags', 0)}")


# ============================================================================
# Commands
# ============================================================================

def cmd_height(rpc: DarkWowRPC):
    h = rpc.get_height()
    print(f"Current height: {h}")
    if h > 0:
        target = rpc.get_target()
        print(f"Current target: {target:#x}")


def cmd_target(rpc: DarkWowRPC):
    target = rpc.get_target()
    print(f"Current PoW target: {target:#x} ({target})")


def cmd_block(rpc: DarkWowRPC, height: int):
    print(border())
    print(f"Block at height {height}")
    print(border("-"))
    block = rpc.get_block(height)
    if block is None:
        print("  Error: block not found")
        return
    hdr = block.get("header", {})
    print_block_header(hdr, rpc)
    txs = block.get("transactions", [])
    print(f"  Transactions:     {len(txs)}")
    for i, tx in enumerate(txs):
        print(f"    [{i}] coinbase: {tx.get('coinbase') is not None}")


def cmd_supply(rpc: DarkWowRPC):
    print(border())
    print("Cumulative Supply Audit (Pedersen Chain)")
    print(border("-"))
    result = rpc.get_cumulative_supply()
    if "error" in result:
        print(f"  Error: {result['error']}")
        return
    h = result.get("height", 0)
    supply = result.get("total_supply", 0)
    expected = expected_cumulative_supply(h)
    match = "MATCH" if supply == expected else "MISMATCH!"
    print(f"  Height:           {h}")
    print(f"  Total Supply:     {supply}")
    print(f"  Expected:         {expected}")
    print(f"  Verification:     {match}")
    print(f"  Cumulative Commit: {result.get('cumulative_value_commit', 'N/A')[:20]}...")
    print(f"  Cumulative Blind:  {result.get('cumulative_blind', 'N/A')[:20]}...")


def cmd_scan(rpc: DarkWowRPC, from_h: int, to_h: int):
    print(border())
    print(f"Block Scan: {from_h} → {to_h}")
    print(border("-"))
    print(f"{'Height':>6}  {'Uncle Root':>20}  {'Uncles?':>7}  {'Reward':>14}  {'Supply Match?':>13}")
    print(border("-"))

    for h in range(from_h, to_h + 1):
        block = rpc.get_block(h)
        if block is None:
            print(f"{h:>6}  {'(not found)':>20}")
            continue
        hdr = block.get("header", {})
        uncle_root = hdr.get("uncle_merkle_root", [0]*32)
        has_uncles = not is_zero_hash(uncle_root)
        total_reward = hdr.get("total_reward", 0)
        expected_r = expected_reward(h)
        reward_match = "OK" if total_reward == expected_r else f"EXP:{expected_r}"
        print(f"{h:>6}  {fmt_hash(uncle_root):>20}  {'YES' if has_uncles else 'no':>7}  {total_reward:>14}  {reward_match:>13}")

    # Summary
    uncle_blocks = 0
    for h in range(from_h, to_h + 1):
        block = rpc.get_block(h)
        if block:
            hdr = block.get("header", {})
            if not is_zero_hash(hdr.get("uncle_merkle_root", [0]*32)):
                uncle_blocks += 1
    print(border("-"))
    print(f"  Uncle blocks: {uncle_blocks}/{to_h - from_h + 1}")
    print(f"  Note: Zero uncle roots are normal for early blocks (no competing blocks yet).")


# ============================================================================
# Main
# ============================================================================

def main():
    parser = argparse.ArgumentParser(
        description="DarkWow Block Explorer — query any node's JSON-RPC",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=textwrap.dedent("""\
            Examples:
              %(prog)s height
              %(prog)s block 5
              %(prog)s supply
              %(prog)s scan 1 20
              %(prog)s --host lilith0.dark.fi --port 31345 height
        """),
    )
    parser.add_argument("--host", default="127.0.0.1", help="Node host (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=31345, help="RPC port (default: 31345)")
    parser.add_argument("--timeout", type=int, default=5, help="Socket timeout in seconds")

    sub = parser.add_subparsers(dest="command", help="Command")

    sub.add_parser("height", help="Get current block height")
    p = sub.add_parser("target", help="Get current PoW target")
    p = sub.add_parser("block", help="Get block at height")
    p.add_argument("height", type=int, help="Block height")
    p = sub.add_parser("supply", help="Cumulative supply audit")
    p = sub.add_parser("scan", help="Scan block range")
    p.add_argument("from_h", type=int, help="Start height")
    p.add_argument("to_h", type=int, help="End height")

    args = parser.parse_args()
    if not args.command:
        parser.print_help()
        sys.exit(1)

    rpc = DarkWowRPC(host=args.host, port=args.port, timeout=args.timeout)

    if args.command == "height":
        cmd_height(rpc)
    elif args.command == "target":
        cmd_target(rpc)
    elif args.command == "block":
        cmd_block(rpc, args.height)
    elif args.command == "supply":
        cmd_supply(rpc)
    elif args.command == "scan":
        cmd_scan(rpc, args.from_h, args.to_h)


if __name__ == "__main__":
    main()
