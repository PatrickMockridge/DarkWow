.POSIX:

# Build `all` when invoked with no target. Make's default goal is the *first*
# target in this file; here that happens to be `all` already, but stating it
# means reordering this file can never silently change what a bare `make` does.
.DEFAULT_GOAL := all

# Cargo binary
CARGO = cargo

# Compile target for system binaries
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)

# Strip host paths from the built WASM, so the artifact is a function of the
# sources rather than of where this checkout happens to live. rustc embeds the
# absolute path of every crate it compiles into panic locations, and those
# paths are readable straight out of the artifact:
#
#     strings -a dwow_deployooor_contract.wasm | grep "$HOME"
#
# This contract is genesis-critical and its bytes are consensus-relevant:
# genesis embeds them (see `build_genesis_deployment_txs` in
# bin/dwowd/src/lib.rs), so a host path in here moves the genesis hash and the
# pin in bin/dwowd/genesis_hash.txt could only be satisfied on one machine.
#
# $(HOME) is expanded by make at this point. It cannot be expanded in
# .cargo/config.toml — see the note in that file for what was measured.
RUSTFLAGS += --remap-path-prefix=$(HOME)/.cargo=/cargo
RUSTFLAGS += --remap-path-prefix=$(HOME)/.rustup=/rustup
RUSTFLAGS += -C link-arg=--max-memory=268435456
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

# wasm build target
WASM_TARGET = wasm32-unknown-unknown

# Cargo package name
PKGNAME = $(shell grep '^name = ' Cargo.toml | cut -d' ' -f3 | tr -d '"')
# wasm contract binary
WASM_BIN = $(PKGNAME:=.wasm)

# wasm source files
WASM_SRC = \
	Cargo.toml \
	../../../Cargo.toml \
	../../../src/sdk/Cargo.toml \
	../../../src/serial/Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../sdk -type f -name '*.rs') \
	$(shell find ../../serial -type f -name '*.rs')

# Source hash manifest
SOURCE_HASH_FILE = .source_hash

# Inputs whose contents can change the built WASM: this crate's Rust sources
# and manifest, and the shared sdk/serial crates it links. Deployooor has no
# zk circuits of its own, so there is no proof/ term here.
# $(wildcard) drops any that don't exist and sorts what remains.
SOURCE_MANIFEST = $(wildcard $(WASM_SRC) proof/*.zk manifest.toml)

# One "path sha256" line per input, sorted, then hashed. Including the path
# makes renames and content swaps visible; sorting removes the dependence on
# glob order, which is not stable across filesystems.
COMPUTE_SOURCE_HASH = $(shell for f in $(SOURCE_MANIFEST); do echo "$$f $$(sha256sum "$$f" | cut -d' ' -f1)"; done | sort | sha256sum | cut -d' ' -f1)

# This guard is why deployooor broke the genesis pin. Its WASM_SRC includes
# src/sdk, so an edit there made the WASM out of date; with no guard, and with
# `all` as this file's default goal, `make test` rebuilt it silently and the
# genesis merkle root moved. See doc/src/arch/verification-hazop.md (the residue
# table, which carries the genesis/consensus audit's open items) and
# doc/src/arch/genesis.md for the pinning rule.
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

all: check-source-hash $(WASM_BIN)

$(WASM_BIN): $(WASM_SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	cp -f ../../../target/$(WASM_TARGET)/release/$@ $@
	@echo "$(COMPUTE_SOURCE_HASH)" > $(SOURCE_HASH_FILE)

test-integration: all
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) test --target=$(RUST_TARGET) \
		--release --package $(PKGNAME) \
		--features=no-entrypoint,client \
		--test integration

test: test-integration

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
	rm -f $(WASM_BIN) $(SOURCE_HASH_FILE)

.PHONY: all test-integration test clippy clean check-source-hash
