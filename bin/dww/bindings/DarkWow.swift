// DarkWow Wallet — Swift binding.
//
//   Just add DarkWow.swift to your Xcode project.
//   Place libdwow_wallet.dylib in your framework search path.
//
//   let w = try DarkWow(keysPath: "keys.toml", section: "wallet-1", network: "testnet")
//   let n = w.scanBlock(json: blockJson)
//   print("scanned \(n) outputs, balance=\(w.balance()), caps=\(w.capCount())")
//   w.close()
//
// C ABI design follows seatuya (https://github.com/moebiusV/seatuya):
// write the protocol binding once in C, and every language gets it through FFI.

import Foundation

// ── C function declarations ──────────────────────────────────────

@_silgen_name("dwow_wallet_version")
func dwow_wallet_version() -> UnsafePointer<CChar>?

@_silgen_name("dwow_wallet_open")
func dwow_wallet_open(
    _ keysPath: UnsafePointer<CChar>?,
    _ section: UnsafePointer<CChar>?,
    _ network: UnsafePointer<CChar>?
) -> OpaquePointer?

@_silgen_name("dwow_wallet_free")
func dwow_wallet_free(_ handle: OpaquePointer?)

@_silgen_name("dwow_wallet_scan_block_json")
func dwow_wallet_scan_block_json(
    _ handle: OpaquePointer?,
    _ blockJson: UnsafePointer<CChar>?
) -> Int32

@_silgen_name("dwow_wallet_cap_count")
func dwow_wallet_cap_count(_ handle: OpaquePointer?) -> Int32

@_silgen_name("dwow_wallet_balance")
func dwow_wallet_balance(_ handle: OpaquePointer?) -> UInt64

@_silgen_name("dwow_wallet_chain_height")
func dwow_wallet_chain_height(_ handle: OpaquePointer?) -> UInt64

@_silgen_name("dwow_wallet_default_address")
func dwow_wallet_default_address(
    _ handle: OpaquePointer?,
    _ outBuf: UnsafeMutablePointer<CChar>?,
    _ bufLen: Int32
) -> Int32

@_silgen_name("dwow_wallet_aead_self_test")
func dwow_wallet_aead_self_test(_ handle: OpaquePointer?) -> Int32

// ── Swift wrapper ────────────────────────────────────────────────

public class DarkWow {
    private var handle: OpaquePointer?

    public init(keysPath: String, section: String, network: String = "testnet") throws {
        guard let h = keysPath.withCString({ kp in
            section.withCString { sec in
                network.withCString { net in
                    dwow_wallet_open(kp, sec, net)
                }
            }
        }) else {
            throw DarkWowError.openFailed
        }
        self.handle = h
    }

    deinit {
        close()
    }

    public func close() {
        if let h = handle {
            dwow_wallet_free(h)
            handle = nil
        }
    }

    public func scanBlock(json: String) -> Int32 {
        return json.withCString { dwow_wallet_scan_block_json(handle, $0) }
    }

    public func capCount() -> Int32 {
        return dwow_wallet_cap_count(handle)
    }

    public func balance() -> UInt64 {
        return dwow_wallet_balance(handle)
    }

    public func chainHeight() -> UInt64 {
        return dwow_wallet_chain_height(handle)
    }

    public func defaultAddress() throws -> String {
        var buf = [CChar](repeating: 0, count: 128)
        let n = dwow_wallet_default_address(handle, &buf, 128)
        if n < 0 { throw DarkWowError.addressFailed }
        return String(cString: buf)
    }

    public func aeadSelfTest() -> Bool {
        return dwow_wallet_aead_self_test(handle) == 0
    }

    public static func version() -> String {
        return String(cString: dwow_wallet_version()!)
    }

    public enum DarkWowError: Error {
        case openFailed
        case addressFailed
    }
}
