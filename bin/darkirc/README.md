# darkirc

DarkWow's peer-to-peer anonymous IRC daemon. Provides fully anonymous chat over
Tor and i2p with no centralized server. Used for project coordination, developer
meetings, and community discussion.

## Building

```shell
# Linux/macOS
make darkirc

# Android (cross-compile)
make darkirc.android64
```

## Quick Start

```shell
# First run spawns config at ~/.config/dwow/darkirc_config.toml
./darkirc

# Connect your IRC client to localhost:6667
weechat irc://localhost:6667
```

The preconfigured defaults autojoin `#dev` (weekly developer meetings — TBA for
time and venue) and other community channels.

## Usage

```shell
./darkirc --help
```

Key flags:
- `--irc-listen` — IRC listener address (default: `127.0.0.1:6667`)
- `--irc-tls-cert` / `--irc-tls-secret` — TLS certificate for IRC connections
- `--datastore` / `--replay-datastore` — Message persistence and replay
- `--gen-chacha-keypair` / `--gen-channel-secret` — Key generation
- `--skip-dag-sync` — Skip event graph DAG sync on startup
- `--list-contacts` — List known contacts
- `--password` / `--encrypt-password` — Password authentication

## Documentation

- [DarkIRC Guide](../../doc/src/misc/darkirc/darkirc.md) — Full setup and usage guide
- [Network Troubleshooting](../../doc/src/misc/network-troubleshooting.md) — Connectivity help
- [Architecture: Event Graph](../../doc/src/arch/legacy/event_graph.md) — DAG messaging substrate

## Android Build

Requires Android NDK, OpenSSL, and SQLcipher compiled for aarch64-android.

### OpenSSL

```shell
git clone https://github.com/openssl/openssl
cd openssl
export ANDROID_NDK_ROOT="/opt/android-ndk"
export PATH="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"
./Configure android-arm64 -D__ANDROID_API__=32
make -j$(nproc)
```

### SQLcipher

```shell
git clone https://github.com/sqlcipher/sqlcipher
cd sqlcipher
sed -e 's/strchrnul//' -i configure
export ANDROID_NDK_ROOT="/opt/android-ndk"
export PATH="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"
CC=aarch64-linux-android32-clang \
  CPPFLAGS="-I$PWD/../openssl/include" \
  LDFLAGS="-L$PWD/../openssl" \
  ./configure \
      --host=aarch64-linux-android32 \
      --disable-shared \
      --enable-static \
      --enable-cross-thread-connections \
      --enable-releasemode \
      --disable-tcl
make -j$(nproc)
./libtool --mode install install libsqlcipher.la $PWD
```

### DarkIRC for Android

```shell
make darkirc.android64
```
