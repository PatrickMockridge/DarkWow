.POSIX:

# Cargo binary
CARGO = cargo

# Compile target for system binaries
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

# wasm build target
WASM_TARGET = wasm32-unknown-unknown

# Cargo package name
PKGNAME = $(shell grep '^name = ' Cargo.toml | cut -d' ' -f3 | tr -d '"')
# wasm contract binary
WASM_BIN = $(PKGNAME:=.wasm)

# zkas compiler - absolute path to ensure we find the right binary
ZKAS = $(abspath ../../../target/release/zkas)

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
COMPUTE_SOURCE_HASH = $(shell cat $(ZK_SRC) | sha256sum | cut -d' ' -f1)

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
			echo "WARNING: Source files have changed since last binary build!"; \
			echo "  Old hash: $$OLD_HASH"; \
			echo "  New hash: $$NEW_HASH"; \
			echo "  Run 'make clean && make all' to regenerate binaries."; \
			echo "  Or run 'make update-hash' to accept the new sources."; \
			exit 1; \
		fi; \
	fi

# Update source hash to current sources (accept new sources as correct)
update-hash:
	@echo "$(COMPUTE_SOURCE_HASH)" > $(SOURCE_HASH_FILE)
	@echo "Updated source hash to current sources."

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

.PHONY: all test-integration test clippy clean check-source-hash update-hash check-zkas-version