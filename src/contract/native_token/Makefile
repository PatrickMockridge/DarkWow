.POSIX:

# Cargo binary
CARGO = cargo

# Compile target for system binaries
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)

# Build `all` when invoked with no target. Make's default goal is the *first*
# target in this file, which is `check-zkas-version` — so a bare
# `make` in this directory, and every `$(MAKE) -C src/contract/<name>` in the
# root Makefile's `contracts:` target, ran the zkas version check and then
# stopped: check-source-hash never ran and the WASM was never rebuilt.
.DEFAULT_GOAL := all

# Strip host paths from the built WASM, so the artifact is a function of the
# sources rather than of where this checkout happens to live. rustc embeds the
# absolute path of every crate it compiles into panic locations, and those
# paths are readable straight out of the artifact:
#
#     strings -a dwow_*_contract.wasm | grep "$HOME"
#
# For the genesis contracts this is load-bearing: genesis embeds these WASM
# bytes (see `build_genesis_deployment_txs` in bin/dwowd/src/lib.rs), so a host
# path in here moves the genesis hash, and the pin in
# bin/dwowd/genesis_hash.txt could only ever be satisfied on one machine.
#
# $(HOME) is expanded by make at this point. It cannot be expanded in
# .cargo/config.toml — see the note in that file for the two mechanisms that
# were measured and do not work.
RUSTFLAGS += --remap-path-prefix=$(HOME)/.cargo=/cargo
RUSTFLAGS += --remap-path-prefix=$(HOME)/.rustup=/rustup
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

# wasm build target
WASM_TARGET = wasm32-unknown-unknown

# Cargo package name
PKGNAME = $(shell grep '^name = ' Cargo.toml | cut -d' ' -f3 | tr -d '"')
# wasm contract binary
WASM_BIN = $(PKGNAME:=.wasm)

# zkas compiler - absolute path to ensure we find the right binary
ZKAS = $(abspath ../../../zkas)

# ZKAS version tracking
ZKAS_VERSION_FILE = .zkas_version

# zkas circuits - use wildcard to find all .zk files and derive .zk.bin names
ZK_SRC := $(wildcard proof/*.zk)
ZK_BINS := $(ZK_SRC:.zk=.zk.bin)

# Source hash manifest
SOURCE_HASH_FILE = .source_hash

# wasm source files
WASM_SRC = \
	Cargo.toml \
	../../../Cargo.toml \
	../../../src/sdk/Cargo.toml \
	../../../src/serial/Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../sdk -type f -name '*.rs') \
	$(shell find ../../serial -type f -name '*.rs')

# Compute SHA256 hash of all source files (ZK files only for simplicity)
# Inputs whose contents can change the built WASM: this crate's Rust sources
# and manifest, its zk circuits, and the shared sdk/serial crates it links.
# $(wildcard) drops any that don't exist and sorts what remains.
SOURCE_MANIFEST = $(wildcard $(WASM_SRC) $(ZK_SRC) manifest.toml)

# One "path sha256" line per input, sorted, then hashed.
#
# Including the path makes renames and content swaps visible; hashing bare
# concatenated contents does not. Sorting removes the dependence on glob
# order, which is not stable across filesystems, so a hash recorded on one
# machine could fail on another for no reason.
#
# The previous definition was `cat $(ZK_SRC) | sha256sum`, i.e. proof/*.zk
# only. A change to src/sdk — the change that actually moved the genesis hash
# and broke the pin — was invisible to it.
COMPUTE_SOURCE_HASH = $(shell for f in $(SOURCE_MANIFEST); do echo "$$f $$(sha256sum "$$f" | cut -d' ' -f1)"; done | sort | sha256sum | cut -d' ' -f1)

# Check if zkas version changed and force rebuild if needed
check-zkas-version:
	@current_ver=$$($(ZKAS) --version 2>/dev/null || echo "none"); \
	if [ -f $(ZKAS_VERSION_FILE) ]; then \
		stored_ver=$$(cat $(ZKAS_VERSION_FILE)); \
		if [ "$$stored_ver" != "$$current_ver" ]; then \
			echo "ZKas version changed ($$stored_ver -> $$current_ver), removing old binaries"; \
			rm -f proof/*.zk.bin; \
			echo "$$current_ver" > $(ZKAS_VERSION_FILE); \
		fi; \
	else \
		echo "No version file, recording current zkas version..."; \
		echo "$$current_ver" > $(ZKAS_VERSION_FILE); \
	fi

# Pattern rule for ZK binaries - depends on source AND zkas compiler
# The check-zkas-version target ensures old binaries are removed on version change
proof/%.zk.bin: proof/%.zk $(ZKAS) check-zkas-version
	$(ZKAS) $< -o $@

# Check if source hash changed since last build
check-source-hash:
	@if [ -f $(SOURCE_HASH_FILE) ]; then \
		OLD_HASH=$$(cat $(SOURCE_HASH_FILE)); \
		NEW_HASH=$(COMPUTE_SOURCE_HASH); \
		if [ "$$OLD_HASH" != "$$NEW_HASH" ]; then \
			echo "WARNING: $(WASM_BIN) is stale."; \
			echo "  Either a source it is built from has changed, or it was built"; \
			echo "  before a change to the source-hash recipe itself."; \
			echo "  Rebuild with: make clean && make all"; \
			echo "  Do not hand-edit $(SOURCE_HASH_FILE) to silence this: that file is"; \
			echo "  what attests this artifact was built from these sources."; \
			exit 1; \
		fi; \
	fi

all: check-source-hash $(ZK_BINS) $(WASM_BIN)

# WASM build depends on ZK binaries
$(WASM_BIN): $(ZK_BINS) $(WASM_SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	cp -f ../../../target/$(WASM_TARGET)/release/$@ $@
	@echo "$(COMPUTE_SOURCE_HASH)" > $(SOURCE_HASH_FILE)

test-integration: all
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) test --target=$(RUST_TARGET) \
		--release --package $(PKGNAME) \
		--features=no-entrypoint,client \
		--test integration

clippy: all
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --package $(PKGNAME) \
		--features=no-entrypoint,client --tests

clean:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) \
		--release --package $(PKGNAME)
	rm -f $(ZK_BINS) $(WASM_BIN) $(SOURCE_HASH_FILE) $(ZKAS_VERSION_FILE)

.PHONY: all test-integration test clippy clean check-source-hash check-zkas-version