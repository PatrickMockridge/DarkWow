# Makefile for the DarkWow bridge contract
#
# Usage:
#   make           - Build the contract WASM binary
#   make proof     - Compile the ZK circuits
#   make clean     - Remove build artifacts
#
# This file does not follow the src/contract/*/Makefile template the other
# contracts use, which is why it had no source-hash guard and passed no
# RUSTFLAGS at all. Both are added here rather than by adopting the template,
# so that the build this file describes stays as it was.

CONTRACT := dwow_bridge_contract
ZKAS := $(abspath ../../../zkas)

WASM_BIN := $(CONTRACT).wasm
SOURCE_HASH_FILE := .source_hash

# Build `all` when invoked with no target. Make's default goal is the *first*
# target in the file; state it so reordering cannot silently change that.
.DEFAULT_GOAL := all

# Strip host paths from the built WASM, so the artifact is a function of the
# sources rather than of where this checkout happens to live. rustc embeds the
# absolute path of every crate it compiles into panic locations, and those
# paths are readable straight out of the artifact:
#
#     strings -a $(WASM_BIN) | grep "$HOME"
#
# $(HOME) is expanded by make at this point. It cannot be expanded in
# .cargo/config.toml — see the note in that file for what was measured.
RUSTFLAGS += --remap-path-prefix=$(HOME)/.cargo=/cargo
RUSTFLAGS += --remap-path-prefix=$(HOME)/.rustup=/rustup

# Inputs whose contents can change the built WASM. $(wildcard) drops any that
# do not exist and sorts what remains.
SOURCE_MANIFEST := $(wildcard Cargo.toml ../../../Cargo.toml \
	../../../src/sdk/Cargo.toml ../../../src/serial/Cargo.toml \
	manifest.toml proof/*.zk) \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../sdk -type f -name '*.rs') \
	$(shell find ../../serial -type f -name '*.rs')

# One "path sha256" line per input, sorted, then hashed.
#
# Including the path makes renames and content swaps visible; hashing bare
# concatenated contents does not. Sorting removes the dependence on glob
# order, which is not stable across filesystems.
COMPUTE_SOURCE_HASH := $(shell for f in $(SOURCE_MANIFEST); do echo "$$f $$(sha256sum "$$f" | cut -d' ' -f1)"; done | sort | sha256sum | cut -d' ' -f1)

.PHONY: all proof clean check-source-hash

# These bytes are embedded into test binaries with include_bytes!, so a stale
# copy means a test silently exercises an artifact that no longer matches the
# sources it claims to be built from.
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

all: check-source-hash proof
	@echo "Building bridge contract WASM..."
	@RUSTFLAGS="$(RUSTFLAGS)" cargo build --release --manifest-path Cargo.toml --target wasm32-unknown-unknown
	@mv ../../../target/wasm32-unknown-unknown/release/$(CONTRACT).wasm ./
	@echo "$(COMPUTE_SOURCE_HASH)" > $(SOURCE_HASH_FILE)
	@echo "Done: $(CONTRACT).wasm"

proof:
	@echo "Compiling ZK circuits..."
	@for zk in proof/*.zk; do \
		$(ZKAS) $$zk -o $${zk%.zk}.zk.bin; \
	done
	@echo "Done compiling circuits"

clean:
	@rm -f *.wasm
	@rm -f proof/*.zk.bin
	@rm -f $(SOURCE_HASH_FILE)
	@echo "Cleaned build artifacts"
