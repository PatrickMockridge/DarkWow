"""DarkWow Wallet — Python ctypes binding.

    pip install darkwow  # (future)
    # or just copy this file — only depends on ctypes (stdlib)

    from darkwow import DarkWow

    w = DarkWow("keys.toml", "wallet-1", "testnet")
    n = w.scan_block(block_json)
    print(f"scanned {n} outputs, balance={w.balance()}, caps={w.cap_count()}")

C ABI design follows seatuya (https://github.com/moebiusV/seatuya):
write the protocol binding once in C, and every language gets it through FFI.
"""

import ctypes
import os
from ctypes import c_char_p, c_int32, c_uint32, c_uint64, c_void_p, POINTER, byref, create_string_buffer

# ── Library loading ──────────────────────────────────────────────

_lib = ctypes.CDLL(os.environ.get("DARKWOW_LIB", "libdwow_wallet.so"))

# ── Opaque handle types ──────────────────────────────────────────

class _WalletHandle(ctypes.Structure):
    pass

# ── Function signatures ──────────────────────────────────────────

_lib.dwow_wallet_version.restype = c_char_p

_lib.dwow_wallet_open.argtypes = [c_char_p, c_char_p, c_char_p]
_lib.dwow_wallet_open.restype = POINTER(_WalletHandle)

_lib.dwow_wallet_free.argtypes = [POINTER(_WalletHandle)]

_lib.dwow_wallet_scan_block_json.argtypes = [POINTER(_WalletHandle), c_char_p]
_lib.dwow_wallet_scan_block_json.restype = c_int32

_lib.dwow_wallet_cap_count.argtypes = [POINTER(_WalletHandle)]
_lib.dwow_wallet_cap_count.restype = c_int32

_lib.dwow_wallet_balance.argtypes = [POINTER(_WalletHandle)]
_lib.dwow_wallet_balance.restype = c_uint64

_lib.dwow_wallet_chain_height.argtypes = [POINTER(_WalletHandle)]
_lib.dwow_wallet_chain_height.restype = c_uint64

_lib.dwow_wallet_default_address.argtypes = [POINTER(_WalletHandle), c_char_p, c_int32]
_lib.dwow_wallet_default_address.restype = c_int32

_lib.dwow_wallet_aead_self_test.argtypes = [POINTER(_WalletHandle)]
_lib.dwow_wallet_aead_self_test.restype = c_int32

_lib.dwow_wallet_version.restype = c_char_p


class DarkWow:
    """A DarkWow wallet instance.

    Opens a wallet identity from a keys.toml file, initializes an in-memory
    SQLite database, and provides scan/balance/capability access.

    No P2P, no networking — pure wallet engine.
    """

    def __init__(self, keys_path: str, section: str, network: str = "testnet"):
        self._handle = _lib.dwow_wallet_open(
            keys_path.encode(), section.encode(), network.encode()
        )
        if not self._handle:
            raise RuntimeError("dwow_wallet_open failed — check keys_path, section, and network")

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()

    def close(self):
        if self._handle:
            _lib.dwow_wallet_free(self._handle)
            self._handle = None

    def scan_block(self, block_json: str) -> int:
        """Scan a block (JSON format), persist discovered outputs. Returns count."""
        return _lib.dwow_wallet_scan_block_json(self._handle, block_json.encode())

    def cap_count(self) -> int:
        """Total held capabilities (active only)."""
        return _lib.dwow_wallet_cap_count(self._handle)

    def balance(self) -> int:
        """Sum of all unspent native token values (base units)."""
        return _lib.dwow_wallet_balance(self._handle)

    def chain_height(self) -> int:
        """Current local chain tip height."""
        return _lib.dwow_wallet_chain_height(self._handle)

    def default_address(self) -> str:
        """Wallet's default address as a string."""
        buf = create_string_buffer(128)
        n = _lib.dwow_wallet_default_address(self._handle, buf, 128)
        if n < 0:
            raise RuntimeError("dwow_wallet_default_address failed")
        return buf.value.decode()

    def aead_self_test(self) -> bool:
        """Run AEAD encrypt/decrypt roundtrip. Returns True on success."""
        return _lib.dwow_wallet_aead_self_test(self._handle) == 0

    @staticmethod
    def version() -> str:
        """Library version string."""
        return _lib.dwow_wallet_version().decode()
