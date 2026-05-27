# Makefile for DarkFi bridge contract
#
# Usage:
#   make           - Build the contract WASM binary
#   make proof     - Compile the ZK circuits
#   make clean     - Remove build artifacts

CONTRACT := dwow_bridge_contract
ZKAS := $(abspath ../../../zkas)

.PHONY: all proof clean

all: proof
	@echo "Building bridge contract WASM..."
	@cargo build --release --manifest-path Cargo.toml --target wasm32-unknown-unknown
	@mv ../../../target/wasm32-unknown-unknown/release/$(CONTRACT).wasm ./
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
	@echo "Cleaned build artifacts"
