# `make wasm` rebuilds the committed dist/benchdiff.wasm. It calls the rustup toolchain's cargo
# directly because a Homebrew rustc earlier on PATH has no wasm32-wasip1 standard library.
RUSTUP_BIN := $(HOME)/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin

.PHONY: test wasm golden ready
test:
	cargo test --workspace

wasm:
	@if [ -x "$(RUSTUP_BIN)/cargo" ]; then PATH="$(RUSTUP_BIN):$$PATH" RUSTC="$(RUSTUP_BIN)/rustc" cargo build --release --target wasm32-wasip1 -p benchdiff; \
	else cargo build --release --target wasm32-wasip1 -p benchdiff; fi
	cp target/wasm32-wasip1/release/benchdiff.wasm dist/benchdiff.wasm

golden: wasm
	cargo build --release -p benchdiff
	node --disable-warning=ExperimentalWarning --test "test/*.test.js"

ready:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	$(MAKE) test golden
