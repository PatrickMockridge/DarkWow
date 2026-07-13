# DarkWow Wallet — Janet FFI binding
#   (import darkwow :prefix "")
#   (def w (dw-open "keys.toml" "wallet-1" "testnet"))
#   (print "balance=" (dw-balance w) " caps=" (dw-cap-count w))
#   (dw-close w)

(import ffi)

(def-ffi dwow_wallet_version :ptr "dwow_wallet_version" [])

(def-ffi dwow_wallet_open :ptr "dwow_wallet_open"
  [:string :string :string])

(def-ffi dwow_wallet_free :void "dwow_wallet_free" [:ptr])

(def-ffi dwow_wallet_scan_block_json :int "dwow_wallet_scan_block_json"
  [:ptr :string])

(def-ffi dwow_wallet_cap_count :int "dwow_wallet_cap_count" [:ptr])

(def-ffi dwow_wallet_balance :u64 "dwow_wallet_balance" [:ptr])

(def-ffi dwow_wallet_chain_height :u64 "dwow_wallet_chain_height" [:ptr])

(defn dw-open [keys-path section network]
  (let [h (dwow_wallet_open keys-path section network)]
    (if (zero? h) (error "dwow_wallet_open failed"))
    h))

(defn dw-close [h] (dwow_wallet_free h))
(defn dw-scan [h json] (dwow_wallet_scan_block_json h json))
(defn dw-cap-count [h] (dwow_wallet_cap_count h))
(defn dw-balance [h] (dwow_wallet_balance h))
(defn dw-chain-height [h] (dwow_wallet_chain_height h))
(defn dw-version [] (dwow_wallet_version))
