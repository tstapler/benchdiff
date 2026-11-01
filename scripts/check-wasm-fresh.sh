#!/bin/sh
# Fails when dist/benchdiff.wasm differs from a rebuild of the current source with the pinned
# toolchain (byte comparison). The committed file is restored afterwards.
# Usage: scripts/check-wasm-fresh.sh   (run from the repo root; needs the wasm32-wasip1 target)
set -eu
committed=dist/benchdiff.wasm
[ -f "$committed" ] || { echo "check-wasm-fresh: $committed not found" >&2; exit 2; }
saved=$(mktemp)
trap 'cp "$saved" "$committed"; rm -f "$saved"' EXIT
cp "$committed" "$saved"
make wasm >&2
if cmp -s "$saved" "$committed"; then
  echo "wasm-fresh: committed wasm is byte-identical to a rebuild"
else
  echo "wasm-fresh: committed wasm ($(wc -c < "$saved") bytes) differs from a rebuild ($(wc -c < "$committed") bytes)" >&2
  exit 1
fi
