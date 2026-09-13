.POSIX:

# Cargo binary
CARGO = cargo

# Compile target for system binaries
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)

# Strip host paths from the built WASM, so the artifact is a function of the
# sources rather than of where this checkout happens to live. rustc embeds the
# absolute path of every crate it compiles into panic locations, and those
# paths are readable straight out of the artifact:
#
#     strings -a dwow_*_contract.wasm | grep "$HOME"
#
# $(HOME) is expanded by make at this point. It cannot be expanded in
# .cargo/config.toml — see the note in that file for what was measured.
RUSTFLAGS += --remap-path-prefix=$(HOME)/.cargo=/cargo
RUSTFLAGS += --remap-path-prefix=$(HOME)/.rustup=/rustup
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

# Build `all` when invoked with no target. Make's default goal is the *first*
# target in this file; state it explicitly so reordering cannot silently change
# what a bare `make` does.
.DEFAULT_GOAL := all

# wasm build target
WASM_TARGET = wasm32-unknown-unknown

# Cargo package name
PKGNAME = $(shell grep -m1 '^name = ' Cargo.toml | cut -d' ' -f3 | tr -d '"')
# wasm contract binary
WASM_BIN = $(PKGNAME:=.wasm)

# zkas compiler binary
ZKAS = ../../../zkas

# zkas circuits
PROOFS_SRC = $(shell find proof -type f -name '*.zk')
PROOFS_BIN = $(PROOFS_SRC:=.bin)

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
# and manifest, its zk circuits, and the shared sdk/serial crates it links.
# $(wildcard) drops any that don't exist and sorts what remains.
SOURCE_MANIFEST = $(wildcard $(WASM_SRC) $(PROOFS_SRC) manifest.toml)

# One "path sha256" line per input, sorted, then hashed.
#
# Including the path makes renames and content swaps visible; hashing bare
# concatenated contents does not. Sorting removes the dependence on glob
# order, which is not stable across filesystems, so a hash recorded on one
# machine could fail on another for no reason. This contract previously had no
# guard at all, so a changed dependency could silently leave a stale artifact.
COMPUTE_SOURCE_HASH = $(shell for f in $(SOURCE_MANIFEST); do echo "$$f $$(sha256sum "$$f" | cut -d' ' -f1)"; done | sort | sha256sum | cut -d' ' -f1)

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

$(PROOFS_BIN): $(ZKAS) $(PROOFS_SRC)
	$(ZKAS) $(basename $@) -o $@

$(WASM_BIN): $(WASM_SRC) $(PROOFS_BIN)
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
	rm -f $(PROOFS_BIN) $(WASM_BIN) $(SOURCE_HASH_FILE)

.PHONY: all test-integration clippy clean check-source-hash