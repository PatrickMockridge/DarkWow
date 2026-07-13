-- DarkWow Wallet — Haskell FFI binding
--   import DarkWow
--   main = do
--     w <- openWallet "keys.toml" "wallet-1" "testnet"
--     n <- scanBlock w blockJson
--     putStrLn $ "scanned " ++ show n ++ " outputs, balance=" ++ show (balance w)

{-# LANGUAGE ForeignFunctionInterface #-}

module DarkWow
    ( WalletHandle
    , openWallet, closeWallet
    , scanBlock, capCount, balance, chainHeight
    , version
    ) where

import Foreign.C.String (CString, newCString, peekCString, withCString)
import Foreign.C.Types (CInt(..), CULLong(..))
import Foreign.Ptr (Ptr, nullPtr)
import Control.Exception (bracket)

data WalletHandle
type WalletPtr = Ptr WalletHandle

foreign import ccall "dwow_wallet_version"   c_version :: IO CString
foreign import ccall "dwow_wallet_open"       c_open :: CString -> CString -> CString -> IO WalletPtr
foreign import ccall "dwow_wallet_free"       c_free :: WalletPtr -> IO ()
foreign import ccall "dwow_wallet_scan_block_json" c_scan :: WalletPtr -> CString -> IO CInt
foreign import ccall "dwow_wallet_cap_count"   c_capCount :: WalletPtr -> IO CInt
foreign import ccall "dwow_wallet_balance"    c_balance :: WalletPtr -> IO CULLong
foreign import ccall "dwow_wallet_chain_height" c_chainHeight :: WalletPtr -> IO CULLong

openWallet :: String -> String -> String -> IO WalletPtr
openWallet keysPath section network =
    withCString keysPath $ \kp ->
    withCString section  $ \sec ->
    withCString network  $ \net -> do
        h <- c_open kp sec net
        if h == nullPtr then error "dwow_wallet_open failed" else return h

closeWallet :: WalletPtr -> IO ()
closeWallet = c_free

scanBlock :: WalletPtr -> String -> IO Int
scanBlock h json = withCString json $ \j -> fromIntegral <$> c_scan h j

capCount :: WalletPtr -> IO Int
capCount h = fromIntegral <$> c_capCount h

balance :: WalletPtr -> IO Integer
balance h = fromIntegral <$> c_balance h

chainHeight :: WalletPtr -> IO Integer
chainHeight h = fromIntegral <$> c_chainHeight h

version :: IO String
version = peekCString =<< c_version

-- Recommended usage: bracket (openWallet ...) closeWallet $ \w -> do ...
