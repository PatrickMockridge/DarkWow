// DarkWow Wallet — Go cgo binding.
//
//   import "github.com/darkrenaissance/darkwow/bindings"
//
//   w, err := darkwow.Open("keys.toml", "wallet-1", "testnet")
//   n := w.ScanBlock(blockJson)
//   fmt.Printf("scanned %d outputs, balance=%d, caps=%d\n", n, w.Balance(), w.CapCount())
//   w.Close()
//
// Build:
//   CGO_LDFLAGS="-L/path/to/lib -ldwow_wallet" go build
//
// C ABI design follows seatuya (https://github.com/moebiusV/seatuya):
// write the protocol binding once in C, and every language gets it through FFI.

package darkwow

/*
#cgo LDFLAGS: -ldwow_wallet
#include <stdlib.h>
#include "dwow_wallet.h"
*/
import "C"
import (
	"errors"
	"unsafe"
)

// Wallet is a DarkWow wallet instance.
type Wallet struct {
	handle *C.WalletHandle
}

// Open opens a wallet identity from a keys.toml file.
// Initializes an in-memory SQLite database. No P2P, no networking.
func Open(keysPath, section, network string) (*Wallet, error) {
	kp := C.CString(keysPath)
	sec := C.CString(section)
	net := C.CString(network)
	defer C.free(unsafe.Pointer(kp))
	defer C.free(unsafe.Pointer(sec))
	defer C.free(unsafe.Pointer(net))

	h := C.dwow_wallet_open(kp, sec, net)
	if h == nil {
		return nil, errors.New("dwow_wallet_open failed — check keys_path, section, and network")
	}
	return &Wallet{handle: h}, nil
}

// Close frees the wallet and all associated resources.
func (w *Wallet) Close() {
	if w.handle != nil {
		C.dwow_wallet_free(w.handle)
		w.handle = nil
	}
}

// ScanBlock scans a block (JSON format) and persists discovered outputs.
// Returns the number of native token outputs discovered.
func (w *Wallet) ScanBlock(blockJson string) int {
	json := C.CString(blockJson)
	defer C.free(unsafe.Pointer(json))
	return int(C.dwow_wallet_scan_block_json(w.handle, json))
}

// CapCount returns the total number of active held capabilities.
func (w *Wallet) CapCount() int {
	return int(C.dwow_wallet_cap_count(w.handle))
}

// Balance returns the sum of all unspent native token values (base units).
func (w *Wallet) Balance() uint64 {
	return uint64(C.dwow_wallet_balance(w.handle))
}

// ChainHeight returns the current local chain tip height.
func (w *Wallet) ChainHeight() uint64 {
	return uint64(C.dwow_wallet_chain_height(w.handle))
}

// DefaultAddress returns the wallet's default address.
func (w *Wallet) DefaultAddress() (string, error) {
	buf := make([]byte, 128)
	n := C.dwow_wallet_default_address(w.handle, (*C.char)(unsafe.Pointer(&buf[0])), 128)
	if n < 0 {
		return "", errors.New("dwow_wallet_default_address failed")
	}
	return string(buf[:n]), nil
}

// AEADSelfTest runs the AEAD encrypt/decrypt roundtrip self-test.
func (w *Wallet) AEADSelfTest() bool {
	return C.dwow_wallet_aead_self_test(w.handle) == 0
}

// Version returns the library version string.
func Version() string {
	return C.GoString(C.dwow_wallet_version())
}
