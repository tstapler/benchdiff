# `make wasm` rebuilds the committed dist/benchdiff.wasm. It calls the rustup toolchain's cargo
# directly because a Homebrew rustc earlier on PATH has no wasm32-wasip1 standard library.
RUSTUP_BIN := $(HOME)/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin

.PHONY: test perf wasm golden ready wasm-size wasm-fresh
test:
	cargo test --workspace

# Perf budgets are #[ignore]d in the debug `make test`; they only mean something in release mode.
perf:
	cargo test --release -p benchdiff-core --test perf --test nway_scale -- --include-ignored --nocapture
	node --disable-warning=ExperimentalWarning --test test/perf.test.js

WASM ?= dist/benchdiff.wasm
# With the rust-src component installed, rustc rewrites std paths from /rustc/<commit>/ to the sysroot; mapping
# them back keeps the bytes identical whether or not the builder has rust-src (it is last, so it wins over the HOME remap).
RUSTC_BIN := $(if $(wildcard $(RUSTUP_BIN)/rustc),$(RUSTUP_BIN)/rustc,rustc)
RUSTC_COMMIT := $(shell $(RUSTC_BIN) -vV | sed -n 's/^commit-hash: //p')
RUSTC_SYSROOT := $(shell $(RUSTC_BIN) --print sysroot)
WASM_RUSTFLAGS := --remap-path-prefix=$(HOME)=/build --remap-path-prefix=$(CURDIR)=/src --remap-path-prefix=$(RUSTC_SYSROOT)/lib/rustlib/src/rust=/rustc/$(RUSTC_COMMIT)

# The remap keeps the committed binary free of the builder's home directory (panic messages embed paths).
wasm:
	@if [ -x "$(RUSTUP_BIN)/cargo" ]; then RUSTFLAGS="$(WASM_RUSTFLAGS)" PATH="$(RUSTUP_BIN):$$PATH" RUSTC="$(RUSTUP_BIN)/rustc" cargo build --release --target wasm32-wasip1 -p benchdiff; \
	else RUSTFLAGS="$(WASM_RUSTFLAGS)" cargo build --release --target wasm32-wasip1 -p benchdiff; fi
	cp target/wasm32-wasip1/release/benchdiff.wasm dist/benchdiff.wasm

golden: wasm
	cargo build --release -p benchdiff
	node --disable-warning=ExperimentalWarning --test "test/*.test.js"

wasm-size:
	sh scripts/check-wasm-size.sh $(WASM)

wasm-fresh:
	sh scripts/check-wasm-fresh.sh

ready:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	$(MAKE) test golden
