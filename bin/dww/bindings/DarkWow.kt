/**
 * DarkWow Wallet — Kotlin JNA binding.
 *
 *   // Add to build.gradle.kts:
 *   //   implementation("net.java.dev.jna:jna:5.14.0")
 *
 *   val w = DarkWow("keys.toml", "wallet-1", "testnet")
 *   val n = w.scanBlock(blockJson)
 *   println("scanned $n outputs, balance=${w.balance()}, caps=${w.capCount()}")
 *   w.close()
 *
 * C ABI design follows seatuya (https://github.com/moebiusV/seatuya):
 * write the protocol binding once in C, and every language gets it through FFI.
 */

import com.sun.jna.Library
import com.sun.jna.Native
import com.sun.jna.Pointer
import java.io.File

interface DarkWowLib : Library {
    fun dwow_wallet_version(): String
    fun dwow_wallet_open(keysPath: String, section: String, network: String): Pointer?
    fun dwow_wallet_free(handle: Pointer)
    fun dwow_wallet_scan_block_json(handle: Pointer, blockJson: String): Int
    fun dwow_wallet_cap_count(handle: Pointer): Int
    fun dwow_wallet_balance(handle: Pointer): Long
    fun dwow_wallet_chain_height(handle: Pointer): Long
    fun dwow_wallet_default_address(handle: Pointer, outBuf: ByteArray, bufLen: Int): Int
    fun dwow_wallet_aead_self_test(handle: Pointer): Int
}

class DarkWow(
    keysPath: String,
    section: String,
    network: String = "testnet",
) : AutoCloseable {

    companion object {
        private val LIB: DarkWowLib = Native.load(
            System.getenv("DARKWOW_LIB") ?: "dwow_wallet", DarkWowLib::class.java
        )

        fun version(): String = LIB.dwow_wallet_version()
    }

    private var handle: Pointer? = null

    init {
        handle = LIB.dwow_wallet_open(keysPath, section, network)
            ?: throw RuntimeException("dwow_wallet_open failed — check keys_path, section, and network")
    }

    override fun close() {
        handle?.let { LIB.dwow_wallet_free(it) }
        handle = null
    }

    fun scanBlock(blockJson: String): Int =
        LIB.dwow_wallet_scan_block_json(handle, blockJson)

    fun capCount(): Int =
        LIB.dwow_wallet_cap_count(handle)

    fun balance(): Long =
        LIB.dwow_wallet_balance(handle)

    fun chainHeight(): Long =
        LIB.dwow_wallet_chain_height(handle)

    fun defaultAddress(): String {
        val buf = ByteArray(128)
        val n = LIB.dwow_wallet_default_address(handle, buf, 128)
        if (n < 0) throw RuntimeException("dwow_wallet_default_address failed")
        return String(buf, 0, n)
    }

    fun aeadSelfTest(): Boolean =
        LIB.dwow_wallet_aead_self_test(handle) == 0
}
