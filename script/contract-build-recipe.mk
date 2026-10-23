# Shared contract build recipe — included at the end of every src/contract/*/Makefile.
#
# WHY THIS EXISTS. The artifact-purity levers must be identical in every contract build, and the
# inputs that can change an artifact (compiler, cargo, flags, target, toolchain pin, Cargo.lock,
# Makefile, this file) must be inside the source hash. Thirty-two copies of the recipe is the
# split-source-of-truth defect the register names CAUSE-2; this is one copy.
#
# The flags are scoped to the wasm target. `$(RUSTFLAGS)` stays as it was for host tests and
# clippy; a `-Z` flag there would require RUSTC_BOOTSTRAP on every host build for no benefit.

CONTRACT_REMAPS = --remap-path-prefix=$(HOME)/.cargo=/cargo --remap-path-prefix=$(HOME)/.rustup=/rustup
CONTRACT_MAX_MEMORY = -C link-arg=--max-memory=268435456
CONTRACT_PURITY = -C strip=symbols -Zlocation-detail=none
CONTRACT_RUSTC_BOOTSTRAP = 1
WASM_RUSTFLAGS = $(CONTRACT_REMAPS) $(CONTRACT_MAX_MEMORY) $(CONTRACT_PURITY)

# The recipe records HOME as a literal so two hosts with different home paths produce the same
# recipe. The build itself still uses the real $(HOME) through CONTRACT_REMAPS.
CANONICAL_WASM_RUSTFLAGS = $(subst $(HOME),HOME,$(WASM_RUSTFLAGS))
BUILD_RECIPE_FILE = ../../../script/contract_build_recipe.txt
BUILD_RECIPE_STAMP = $(shell { \
  printf 'rustc %s\n' "$$(rustc -Vv | sha256sum | cut -d' ' -f1)"; \
  printf 'cargo %s\n' "$$(cargo -V)"; \
  printf 'target %s\n' "$(WASM_TARGET)"; \
  printf 'bootstrap %s\n' "$(CONTRACT_RUSTC_BOOTSTRAP)"; \
  printf 'rustflags %s\n' "$(CANONICAL_WASM_RUSTFLAGS)"; \
} | sha256sum | cut -d' ' -f1)
BUILD_RECIPE = $(BUILD_RECIPE_STAMP)

# Everything that can change the artifact, including the recipe itself. The Makefile and this
# include are here because OBL-T26's counterexample changed a link argument and the freshness gate
# stayed green; the toolchain pin and Cargo.lock are here for OBL-C210's compiler blind spot.
SOURCE_MANIFEST += Makefile ../../../script/contract-build-recipe.mk $(BUILD_RECIPE_FILE) \
                   ../../../Cargo.lock ../../../rust-toolchain.toml

# `sort`'s collation is locale-dependent: the same tree hashed under en_US.UTF-8
# and under the container's C locale produces two different hashes. That made a
# correct artifact fail check-source-hash inside the image while passing on the
# host. The sort is byte-order only.
COMPUTE_SOURCE_HASH = $(shell { \
  for f in $(SOURCE_MANIFEST); do echo "$$f $$(sha256sum "$$f" | cut -d' ' -f1)"; done; \
  echo "recipe $(BUILD_RECIPE)"; \
} | LC_ALL=C sort | sha256sum | cut -d' ' -f1)

# `make -C src/contract/<name> -s recipe` prints the canonical recipe. The tracked
# script/contract_build_recipe.txt is its output; check-artifact-freshness.sh compares them.
recipe:
	@printf 'rustc %s\n' "$$(rustc -Vv | sha256sum | cut -d' ' -f1)"
	@printf '%s\n' "$$(cargo -V)"
	@printf 'target %s\n' "$(WASM_TARGET)"
	@printf 'bootstrap %s\n' "$(CONTRACT_RUSTC_BOOTSTRAP)"
	@printf 'rustflags %s\n' "$(CANONICAL_WASM_RUSTFLAGS)"
