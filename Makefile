.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo

# Compile target for system binaries
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes
# If building natively, this might give you more speed
#RUSTFLAGS = -C target_cpu=native

# List of zkas circuits to compile, used for tests
PROOFS_SRC = \
	$(shell find proof -type f -name '*.zk') \
	$(shell find bin/darkirc/proof -type f -name '*.zk')

PROOFS_BIN = $(PROOFS_SRC:=.bin)

# List of all binaries built
BINS = \
	zkas \
	dwowd \
	dwow_wallet \
	darkirc \
	genev \
	genevd \
	lilith \
	taud \
	explorer \
	fud \
	fu

all: $(BINS)

zkas:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

$(PROOFS_BIN): zkas $(PROOFS_SRC)
	./zkas $(basename $@) -o $@

contracts: zkas
	$(MAKE) -C src/contract/native_token
	$(MAKE) -C src/contract/deployooor
	$(MAKE) -C src/contract/dao_escrow
	$(MAKE) -C src/contract/dex
	$(MAKE) -C src/contract/stablecoin
	$(MAKE) -C src/contract/attestation
	$(MAKE) -C src/contract/auction
	$(MAKE) -C src/contract/baccarat
	$(MAKE) -C src/contract/bearer_bond
	$(MAKE) -C src/contract/betting_stake
	$(MAKE) -C src/contract/bridge
	$(MAKE) -C src/contract/darkbet_exchange
	$(MAKE) -C src/contract/darktoshi_dice
	$(MAKE) -C src/contract/drain_protection
	$(MAKE) -C src/contract/escrow
	$(MAKE) -C src/contract/game_room
	$(MAKE) -C src/contract/identity
	$(MAKE) -C src/contract/insurance_market
	$(MAKE) -C src/contract/labor_market
	$(MAKE) -C src/contract/lottery
	$(MAKE) -C src/contract/oracle
	$(MAKE) -C src/contract/pool_stake
	$(MAKE) -C src/contract/relayer_endowment
	$(MAKE) -C src/contract/roulette
	$(MAKE) -C src/contract/slot
	$(MAKE) -C src/contract/subscription
	$(MAKE) -C src/contract/otc_swap
	$(MAKE) -C src/contract/tender

dwowd: contracts
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

dwow_wallet: contracts
	$(MAKE) -C bin/drk \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

darkirc: zkas
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

genev:
	$(MAKE) -C bin/genev/genev-cli \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

genevd:
	$(MAKE) -C bin/genev/genevd \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

lilith:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

taud:
	$(MAKE) -C bin/tau/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

vanityaddr:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

explorer:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

fud:
	$(MAKE) -C bin/fud/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

fu:
	$(MAKE) -C bin/fud/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

# -- END OF BINS --

fmt:
	$(CARGO) +nightly fmt --all

# cargo install cargo-hack
check: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) hack check --target=$(RUST_TARGET) \
		--release --feature-powerset --workspace

clippy: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --all-features --workspace --tests

fix: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --all-features --workspace --tests --fix --allow-dirty

rustdoc: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) doc --target=$(RUST_TARGET) \
		--release --all-features --workspace --document-private-items --no-deps

test: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) test --target=$(RUST_TARGET) \
		--release --all-features --workspace

bench-zk-from-json: contracts $(PROOFS_BIN)
	rm -f src/contract/test-harness/*.bin
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) bench --target=$(RUST_TARGET) \
		--bench zk_from_json --all-features --workspace \
		-- --save-baseline master

bench: contracts $(PROOFS_BIN)
	rm -f src/contract/test-harness/*.bin
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) bench --target=$(RUST_TARGET) \
		--all-features --workspace \
		-- --save-baseline master

coverage: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) llvm-cov --target=$(RUST_TARGET) \
		--release --all-features --workspace --html

clean:
	$(MAKE) -C src/contract/deployooor clean
	$(MAKE) -C bin/zkas clean
	$(MAKE) -C bin/dwowd clean
	$(MAKE) -C bin/drk clean
	$(MAKE) -C bin/darkirc clean
	$(MAKE) -C bin/genev/genev-cli clean
	$(MAKE) -C bin/genev/genevd clean
	$(MAKE) -C bin/lilith clean
	$(MAKE) -C bin/tau/taud clean
	$(MAKE) -C bin/vanityaddr clean
	$(MAKE) -C bin/explorer clean
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) --release
	rm -f $(PROOFS_BIN)

distclean: clean
	rm -rf target

.PHONY: all $(BINS) fmt check clippy fix rustdoc \
	test bench-zk-from-json bench coverage clean distclean
