/// DarkWow Wallet — Dart FFI binding (Flutter / standalone).
///
///   import 'darkwow.dart';
///
///   final w = DarkWow('keys.toml', 'wallet-1', 'testnet');
///   final n = w.scanBlock(blockJson);
///   print('scanned $n outputs, balance=${w.balance()}, caps=${w.capCount()}');
///   w.close();
///
/// C ABI design follows seatuya (https://github.com/moebiusV/seatuya):
/// write the protocol binding once in C, and every language gets it through FFI.

import 'dart:ffi';
import 'dart:io' show Platform;

import 'package:ffi/ffi.dart';

// ── Native function typedefs ─────────────────────────────────────

typedef DwowWalletOpenNative = Pointer<Void> Function(
    Pointer<Utf8> keysPath, Pointer<Utf8> section, Pointer<Utf8> network);
typedef DwowWalletOpenDart = Pointer<Void> Function(
    Pointer<Utf8> keysPath, Pointer<Utf8> section, Pointer<Utf8> network);

typedef DwowWalletFreeNative = Void Function(Pointer<Void> handle);
typedef DwowWalletFreeDart = void Function(Pointer<Void> handle);

typedef DwowWalletScanBlockNative = Int32 Function(
    Pointer<Void> handle, Pointer<Utf8> blockJson);
typedef DwowWalletScanBlockDart = int Function(
    Pointer<Void> handle, Pointer<Utf8> blockJson);

typedef DwowWalletCapCountNative = Int32 Function(Pointer<Void> handle);
typedef DwowWalletCapCountDart = int Function(Pointer<Void> handle);

typedef DwowWalletBalanceNative = Uint64 Function(Pointer<Void> handle);
typedef DwowWalletBalanceDart = int Function(Pointer<Void> handle);

typedef DwowWalletChainHeightNative = Uint64 Function(Pointer<Void> handle);
typedef DwowWalletChainHeightDart = int Function(Pointer<Void> handle);

typedef DwowWalletVersionNative = Pointer<Utf8> Function();
typedef DwowWalletVersionDart = Pointer<Utf8> Function();

// ── Library loading ──────────────────────────────────────────────

DynamicLibrary _loadLib() {
    final name = Platform.environment['DARKWOW_LIB'] ??
        (Platform.isLinux
            ? 'libdwow_wallet.so'
            : Platform.isMacOS
                ? 'libdwow_wallet.dylib'
                : 'dwow_wallet.dll');
    return DynamicLibrary.open(name);
}

final _lib = _loadLib();

final _open =
    _lib.lookupFunction<DwowWalletOpenNative, DwowWalletOpenDart>('dwow_wallet_open');
final _free =
    _lib.lookupFunction<DwowWalletFreeNative, DwowWalletFreeDart>('dwow_wallet_free');
final _scanBlock =
    _lib.lookupFunction<DwowWalletScanBlockNative, DwowWalletScanBlockDart>(
        'dwow_wallet_scan_block_json');
final _capCount =
    _lib.lookupFunction<DwowWalletCapCountNative, DwowWalletCapCountDart>(
        'dwow_wallet_cap_count');
final _balance =
    _lib.lookupFunction<DwowWalletBalanceNative, DwowWalletBalanceDart>(
        'dwow_wallet_balance');
final _chainHeight =
    _lib.lookupFunction<DwowWalletChainHeightNative, DwowWalletChainHeightDart>(
        'dwow_wallet_chain_height');
final _version =
    _lib.lookupFunction<DwowWalletVersionNative, DwowWalletVersionDart>(
        'dwow_wallet_version');

// ── Dart wrapper ─────────────────────────────────────────────────

class DarkWow {
    Pointer<Void>? _handle;

    DarkWow(String keysPath, String section, [String network = 'testnet']) {
        final kp = keysPath.toNativeUtf8();
        final sec = section.toNativeUtf8();
        final net = network.toNativeUtf8();
        try {
            _handle = _open(kp, sec, net);
            if (_handle == nullptr) {
                throw Exception(
                    'dwow_wallet_open failed — check keys_path, section, and network');
            }
        } finally {
            calloc.free(kp);
            calloc.free(sec);
            calloc.free(net);
        }
    }

    void close() {
        if (_handle != null) {
            _free(_handle!);
            _handle = null;
        }
    }

    int scanBlock(String blockJson) {
        final json = blockJson.toNativeUtf8();
        try {
            return _scanBlock(_handle!, json);
        } finally {
            calloc.free(json);
        }
    }

    int capCount() => _capCount(_handle!);
    int balance() => _balance(_handle!);
    int chainHeight() => _chainHeight(_handle!);

    static String version() => _version().toDartString();
}
