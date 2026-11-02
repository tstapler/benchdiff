#!/bin/sh
# Fails when the committed wasm differs from a rebuild of the current source with the pinned
# toolchain (byte comparison). dist/benchdiff.wasm is restored afterwards.
# Usage: scripts/check-wasm-fresh.sh   (run from the repo root; needs the wasm32-wasip1 target)
# WASM_COMMITTED=path compares that file instead of dist/benchdiff.wasm (used by the mismatch test).
set -eu
built=dist/benchdiff.wasm
committed=${WASM_COMMITTED:-$built}
[ -f "$built" ] || { echo "check-wasm-fresh: $built not found" >&2; exit 2; }
[ -f "$committed" ] || { echo "check-wasm-fresh: $committed not found" >&2; exit 2; }
saved=$(mktemp)
reference=$(mktemp)
trap 'cp "$saved" "$built"; rm -f "$saved" "$reference"' EXIT
cp "$built" "$saved"
cp "$committed" "$reference"
make wasm >&2
if cmp -s "$reference" "$built"; then
  echo "wasm-fresh: committed wasm is byte-identical to a rebuild"
else
  echo "wasm-fresh: committed wasm ($(wc -c < "$reference") bytes) differs from a rebuild ($(wc -c < "$built") bytes)" >&2
  exit 1
fi
