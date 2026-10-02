#!/usr/bin/env bash
# Regenerate DGMNB's embedded font subsets.
#   nix shell --impure --expr 'with (builtins.getFlake "nixpkgs").legacyPackages.x86_64-linux;
#     python3.withPackages (p: [ p.fonttools p.brotli ])' -c tools/fonts/subset.sh
# Sources: Inter 4.1 and Noto (nixpkgs `inter`, `noto-fonts`), both OFL-1.1.
set -euo pipefail
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
cd "$(dirname "$0")/../.."
out=apps/dgmnb/assets/fonts
INTER=${INTER:-$(nix eval --raw nixpkgs#inter.outPath)/share/fonts/truetype/InterVariable.ttf}
NOTO=${NOTO:-$(nix eval --raw nixpkgs#noto-fonts.outPath)/share/fonts/noto}

# UI text: Latin (incl. extended + Vietnamese), Greek, Cyrillic, punctuation,
# super/subscripts, currency signs, letterlike symbols, arrows, maths.
pyftsubset "$INTER" --output-file=$out/Inter.ttf \
  --unicodes="U+0020-007E,U+00A0-024F,U+0300-036F,U+0370-03FF,U+0400-04FF,U+1D00-1DBF,U+1E00-1EFF,U+2000-206F,U+2070-209F,U+20A0-20CF,U+2100-214F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+25A0-25FF,U+2A7D-2A7E" \
  --layout-features='kern,liga,calt,tnum,case,ss01,cv01,locl,mark,mkmk' --no-hinting --desubroutinize

# Fallbacks for the few symbols Inter lacks (maths, a handful of currency
# signs), kept to exactly what the app shows.
pyftsubset "$NOTO/NotoSansMath-Regular.otf" --output-file=$out/NotoSansMath-subset.otf \
  --text="√∛∜∞≤≥≪≫∈∅∪⋅∙∗∕⌈⌉⌊⌋⩽⩾ℝℤℎ𝑥⇔·▾▸◂" --no-hinting
# Arabic currency symbols (د.إ, ر.س, ﷼, ؋, …): their letters, with joining forms.
fonttools varLib.instancer "$NOTO/NotoSansArabic.ttf" wght=400 wdth=100 -q -o "$TMPDIR/ar.ttf"
pyftsubset "$TMPDIR/ar.ttf" --output-file=$out/NotoSansArabic-subset.ttf \
  --text="$(grep -rhoP '[\x{0600}-\x{06FF}\x{FDFC}]' crates/unitconv/src crates/copypaste/src | sort -u | tr -d '\n')." \
  --layout-features='isol,init,medi,fina,rlig,liga,ccmp,locl,mark,mkmk,kern' --no-hinting
pyftsubset "$NOTO/NotoSansArmenian.ttf" --output-file=$out/NotoSansArmenian-subset.ttf --text="֏" --no-hinting
pyftsubset "$NOTO/NotoSansBengali.ttf" --output-file=$out/NotoSansBengali-subset.ttf --text="৳" --no-hinting
pyftsubset "$NOTO/NotoSansKhmer.ttf" --output-file=$out/NotoSansKhmer-subset.ttf --text="៛" --no-hinting
ls -la $out
