CONTRACT := dwow_entropy_contract
ZKAS := $(abspath ../../../zkas)

.PHONY: all proof clean

all: proof
	@echo "Building entropy contract WASM..."
	@cargo build --release --manifest-path Cargo.toml --target wasm32-unknown-unknown
	@mv ../../../target/wasm32-unknown-unknown/release/$(CONTRACT).wasm ./
	@echo "Done: $(CONTRACT).wasm"

proof:
	@echo "Compiling ZK circuits..."
	@if ls proof/*.zk >/dev/null 2>&1; then \
		for zk in proof/*.zk; do \
			$(ZKAS) $$zk -o $${zk%.zk}.zk.bin; \
		done; \
	fi
	@echo "Done compiling circuits"

clean:
	@rm -f *.wasm
	@rm -f proof/*.zk.bin
	@echo "Cleaned build artifacts"
