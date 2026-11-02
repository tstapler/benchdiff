#!/bin/sh
# Dev-only: rasterize SVGs to PNG so a human can review them before a golden is committed.
# Never a build dependency and never part of the wasm.
# Usage: scripts/svg-to-png.sh [-o outdir] [svg-or-dir ...]
#   defaults: outdir target/svg-png, input crates/benchdiff-core/tests/snapshots/svg
# Writes <name>-760.png (display width 760 px) and <name>-320.png (a narrow, mobile-width display)
# per SVG. Rasteriser: resvg, else rsvg-convert, else inkscape.
out=target/svg-png
if [ "$1" = "-o" ]; then
  [ -n "$2" ] || { echo "svg-to-png: -o needs a directory" >&2; exit 2; }
  out=$2; shift 2
fi
[ $# -gt 0 ] || set -- crates/benchdiff-core/tests/snapshots/svg

# render <svg> <width> <png>
if command -v resvg >/dev/null 2>&1; then
  render() { resvg -w "$2" "$1" "$3"; }
elif command -v rsvg-convert >/dev/null 2>&1; then
  render() { rsvg-convert -w "$2" -o "$3" "$1"; }
elif command -v inkscape >/dev/null 2>&1; then
  render() { inkscape "$1" --export-type=png --export-width="$2" --export-filename="$3" >/dev/null; }
else
  echo "svg-to-png: no rasteriser found; install one of resvg, rsvg-convert (librsvg) or inkscape" >&2
  exit 3
fi

mkdir -p "$out" || exit 2
count=0
for input in "$@"; do
  if [ -d "$input" ]; then files=$(find "$input" -name '*.svg' | sort); else files=$input; fi
  for svg in $files; do
    [ -f "$svg" ] || { echo "svg-to-png: $svg not found" >&2; exit 2; }
    name=$(basename "$svg" .svg)
    for width in 760 320; do
      render "$svg" "$width" "$out/$name-$width.png" || { echo "svg-to-png: failed on $svg" >&2; exit 1; }
    done
    count=$((count + 1))
  done
done
[ "$count" -gt 0 ] || { echo "svg-to-png: no .svg files in: $*" >&2; exit 2; }
echo "svg-to-png: wrote $((count * 2)) PNGs to $out"
