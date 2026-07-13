-- DarkWow Wallet — LuaJIT FFI binding
--   local darkwow = require("darkwow")
--   local w = darkwow.open("keys.toml", "wallet-1", "testnet")
--   print("balance=" .. w:balance() .. " caps=" .. w:capCount())
--   w:close()

local ffi = require("ffi")

ffi.cdef([[
    typedef struct WalletHandle WalletHandle;
    const char* dwow_wallet_version(void);
    WalletHandle* dwow_wallet_open(const char* keys_path, const char* section, const char* network);
    void dwow_wallet_free(WalletHandle* handle);
    int dwow_wallet_scan_block_json(WalletHandle* handle, const char* block_json);
    int dwow_wallet_cap_count(WalletHandle* handle);
    uint64_t dwow_wallet_balance(WalletHandle* handle);
    uint64_t dwow_wallet_chain_height(WalletHandle* handle);
]])

local lib = ffi.load(os.getenv("DARKWOW_LIB") or "libdwow_wallet")

local DarkWow = {}
DarkWow.__index = DarkWow

function DarkWow.open(keysPath, section, network)
    network = network or "testnet"
    local handle = lib.dwow_wallet_open(keysPath, section, network)
    if handle == nil then error("dwow_wallet_open failed") end
    return setmetatable({ handle = handle }, DarkWow)
end

function DarkWow:close()
    if self.handle then lib.dwow_wallet_free(self.handle); self.handle = nil end
end

function DarkWow:scanBlock(json)
    return lib.dwow_wallet_scan_block_json(self.handle, json)
end

function DarkWow:capCount() return lib.dwow_wallet_cap_count(self.handle) end
function DarkWow:balance() return tonumber(lib.dwow_wallet_balance(self.handle)) end
function DarkWow:chainHeight() return tonumber(lib.dwow_wallet_chain_height(self.handle)) end

DarkWow.version = function() return ffi.string(lib.dwow_wallet_version()) end

return DarkWow
