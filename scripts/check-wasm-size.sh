#!/bin/sh
# Fails when the wasm exceeds BUDGET bytes. Raising it needs a one-line note in ADR-004.
# Usage: scripts/check-wasm-size.sh [path]   (default dist/benchdiff.wasm)
BUDGET=820000
file=${1:-dist/benchdiff.wasm}
[ -f "$file" ] || { echo "check-wasm-size: $file not found" >&2; exit 2; }
size=$(wc -c < "$file" | tr -d ' ')
echo "$size / $BUDGET bytes"
[ "$size" -le "$BUDGET" ] || { echo "check-wasm-size: $file is over budget by $((size - BUDGET)) bytes" >&2; exit 1; }
