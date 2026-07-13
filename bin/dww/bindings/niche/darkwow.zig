// DarkWow Wallet — Zig FFI binding
//   const darkwow = @import("darkwow");
//   var w = try darkwow.Wallet.open("keys.toml", "wallet-1", "testnet");
//   defer w.close();
//   const n = w.scanBlock(block_json);
//   std.debug.print("scanned {d} outputs, balance={d}\n", .{ n, w.balance() });

const std = @import("std");

const WalletHandle = opaque {};

extern "c" fn dwow_wallet_version() [*c]const u8;
extern "c" fn dwow_wallet_open(keys_path: [*c]const u8, section: [*c]const u8, network: [*c]const u8) ?*WalletHandle;
extern "c" fn dwow_wallet_free(handle: ?*WalletHandle) void;
extern "c" fn dwow_wallet_scan_block_json(handle: ?*WalletHandle, block_json: [*c]const u8) c_int;
extern "c" fn dwow_wallet_cap_count(handle: ?*WalletHandle) c_int;
extern "c" fn dwow_wallet_balance(handle: ?*WalletHandle) u64;
extern "c" fn dwow_wallet_chain_height(handle: ?*WalletHandle) u64;

pub const Wallet = struct {
    handle: *WalletHandle,

    pub fn open(keys_path: [:0]const u8, section: [:0]const u8, network: [:0]const u8) !Wallet {
        const h = dwow_wallet_open(keys_path.ptr, section.ptr, network.ptr) orelse
            return error.OpenFailed;
        return Wallet{ .handle = h };
    }

    pub fn close(self: *Wallet) void {
        dwow_wallet_free(self.handle);
    }

    pub fn scanBlock(self: *Wallet, json: [:0]const u8) c_int {
        return dwow_wallet_scan_block_json(self.handle, json.ptr);
    }

    pub fn capCount(self: *Wallet) c_int { return dwow_wallet_cap_count(self.handle); }
    pub fn balance(self: *Wallet) u64 { return dwow_wallet_balance(self.handle); }
    pub fn chainHeight(self: *Wallet) u64 { return dwow_wallet_chain_height(self.handle); }

    pub fn version() [:0]const u8 {
        return std.mem.span(dwow_wallet_version());
    }
};
