#!/usr/bin/env bash
# modules/build.sh [--check]: builds the WebAssembly modules the built-in
# themes ship (docs/theming.md, "Modules") from their source here, into the
# themes' folders, where they are committed:
#
#   modules/field  ->  apps/gmnb/themes/swirls/paint.wasm  (Swirls' paint)
#
# The same bytes on every rebuild and from any checkout: built in a
# temporary folder from scratch, one codegen unit, no debug info, stripped,
# with the checkout's path remapped, and with RUSTFLAGS of its own (the
# packages' `-C target-cpu=x86-64-v3` means nothing for wasm32): simd128,
# and no relaxed SIMD (the app turns it off). The bytes depend on the
# toolchain's build, not only its version, so run it in the dev shell (the
# flake's pinned rustc, and lld's wasm-ld):
#
#   nix develop -c modules/build.sh            # rebuild the committed files
#   nix develop -c modules/build.sh --check    # rebuild elsewhere and compare
#
# A nixpkgs or Rust update can change the bytes: rebuild the modules in the
# same commit.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
check=0
case "${1:-}" in
  --check) check=1 ;;
  "") ;;
  *) echo "usage: modules/build.sh [--check]" >&2; exit 2 ;;
esac
command -v wasm-ld >/dev/null || { echo "modules/build.sh: no wasm-ld; run it in the dev shell (nix develop)" >&2; exit 2; }
work=$(mktemp -d "${TMPDIR:-/tmp}/gmnb-modules.XXXXXX")
trap 'rm -rf "$work"' EXIT

# build CRATE OUT: modules/CRATE's module, built as described, to OUT.
build() {
  local crate=$1 out=$2
  (
    cd "$root/modules/$crate"
    env -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_RUSTFLAGS \
      -u CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS -u CARGO_BUILD_TARGET \
      CARGO_INCREMENTAL=0 CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER=wasm-ld \
      RUSTFLAGS="-C target-feature=+simd128 --remap-path-prefix=$root=gmnb --remap-path-prefix=$work=build" \
      cargo build --release --locked --offline --quiet --target wasm32-unknown-unknown \
      --target-dir "$work/target-$crate"
  )
  cp "$work/target-$crate/wasm32-unknown-unknown/release/$crate.wasm" "$out"
}

status=0
for pair in "field:apps/gmnb/themes/swirls/paint.wasm"; do
  crate=${pair%%:*}; dest=$root/${pair#*:}
  if [ $check = 1 ]; then
    build "$crate" "$work/$crate.wasm"
    if cmp -s "$work/$crate.wasm" "$dest"; then
      echo "${pair#*:}: the same as modules/$crate builds ($(sha256sum < "$dest" | cut -c1-16)…)"
    else
      echo "::error::${pair#*:} isn't what modules/$crate builds (committed $(sha256sum < "$dest" 2>/dev/null | cut -c1-16)…, built $(sha256sum < "$work/$crate.wasm" | cut -c1-16)…). Rebuild it with 'nix develop -c modules/build.sh' and commit it; a nixpkgs or Rust update must rebuild the modules in the same commit." >&2
      status=1
    fi
  else
    build "$crate" "$dest"
    echo "${pair#*:}: $(wc -c < "$dest") bytes, sha256 $(sha256sum < "$dest" | cut -c1-64)"
  fi
done
exit $status
