/* DarkWow Wallet — C example (native interface)
 *
 * Compile:
 *   cc -o wallet-example example.c -L target/release -ldwow_wallet
 *
 * This is the C ABI that all other bindings call. The header at
 * ../include/dwow_wallet.h is the authoritative API reference.
 */

#include <stdio.h>
#include "../dwow_wallet.h"

int main(void) {
    printf("DarkWow wallet version: %s\n", dwow_wallet_version());

    WalletHandle *w = dwow_wallet_open("keys.toml", "wallet-1", "testnet");
    if (!w) { fprintf(stderr, "open failed\n"); return 1; }

    printf("chain height: %lu\n", dwow_wallet_chain_height(w));
    printf("capabilities: %d\n", dwow_wallet_cap_count(w));
    printf("balance: %lu\n", dwow_wallet_balance(w));
    printf("aead self-test: %s\n", dwow_wallet_aead_self_test(w) == 0 ? "PASS" : "FAIL");

    dwow_wallet_free(w);
    return 0;
}
