WASM = verify_threshold.wasm
TARGET = wasm32-unknown-unknown
RUSTFLAGS =

all: $(WASM)

$(WASM):
	RUSTFLAGS="$(RUSTFLAGS)" cargo build --release --target=$(TARGET)
	cp ../../target/$(TARGET)/release/dwow_threshold_verify.wasm $(WASM)

clean:
	cargo clean
	rm -f $(WASM)

.PHONY: all clean
