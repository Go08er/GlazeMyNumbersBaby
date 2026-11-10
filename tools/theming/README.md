# tools/theming

The screenshot comparison that decides whether GMNB's default look is
"identical" to DGMNB's (`docs/theming.md`, "Acceptance: the base look is
DGMNB's", "How it is measured"). A standalone crate (own `Cargo.lock`,
excluded from the workspace like `tools/bench`), depending only on `png` and
`serde_json`. It needs no display: it compares pictures. Capturing them does
(`goldens.sh`).

    cargo build --release --manifest-path tools/theming/Cargo.toml
    compare D.png G.png [--text D.json] [--exclude x,y,w,h]... [--report out.png]
    compare --batch DIR_D DIR_G [--map FILE] [--d-prefix dgmnb] [--g-prefix gmnb]
            [--skip SUBSTRING]... [--exclude x,y,w,h]... [--report-dir DIR]

`D` is DGMNB's picture, `G` GMNB's. The exit status is 0 on pass, 1 on
failure, 2 on a usage or read error (including pictures of different size).

Batch mode pairs `DIR_D/dgmnb-<case>.png` with `DIR_G/gmnb-<case>.png` and
reads `DIR_D/dgmnb-<case>.json` for the text boxes when it exists. A `--map`
file pairs other names, one pair a line: `dgmnb-standard-d0
gmnb-standard-aurora-d0 [exclude=x,y,w,h]...` (`#` comments). It prints a
table (flat %, edge %, failing text runs / runs, verdict). `--report` writes
the diff picture (batch: one per case in `--report-dir`): D faded, failing
flat pixels red, failing edge pixels yellow, failing text and curve boxes
outlined magenta.

## What is measured, and the thresholds

All from `docs/theming.md`; the constants are at the top of `src/main.rs`.

1. **Flat pixels.** A pixel of D is flat when its 5x5 neighbourhood is one
   exact colour (a pixel within 2 px of the picture's edge is not flat). It
   passes when G's pixel at the same place is within 2/255 per channel. At
   least 99.5% must pass.
2. **Edges.** Every other pixel of D, outside text and curves, needs a pixel
   of G within 1 px (Chebyshev) within 24/255 (largest channel difference).
   At least 99% must pass.
3. **Text.** For each run in D's JSON, all of: the ink bounding box in G has
   each side within 1 px of D's; the mean ink colour is within 6/255; after a
   3x3 box blur of both, the mean absolute difference over D's box is at most
   12/255. Every run must pass.

Excluded always: 16x16 at each window corner. Others by `--exclude` (or a
map line): the drag handle, popups, Settings and About, the caret, text in
a script the bundled fonts do not cover. A run whose centre is excluded is
not compared.

### How the definition was made exact

- *Ink.* The text's background is the commonest colour on the border of D's
  box grown by 4 px (taken separately in D and G). Ink pixels are those
  differing from it by more than 24/255 in some channel; the ink box is
  measured the same way in both pictures within that window, so glyph
  anti-aliasing at the box edge counts equally on both sides. The mean ink
  colour weights each pixel by its difference from the background (over 6),
  so thin anti-aliased text is not diluted by background.
- *Curves.* The hook lists each graph curve's box with its colour
  (`"kind":"curve"`). Ink is what lies within 40/255 of the colour; the
  non-flat pixels within 2 px of such ink are curve, not edges. The three
  text rules then apply to it as to text.
- *Text pixels outside the edge rule* are the pixels inside D's box.
- *Text must pass for every run*, not for a share: the document gives
  per-run rules and no percentage.
- Alpha is ignored; the pictures are compared as RGB.

## DGMNB's boxes

`DGMNB_TEXT_BOXES=path.json` together with `DGMNB_SCREENSHOT` writes
`[{"x":..,"y":..,"w":..,"h":..,"text":"..","kind":"text"|"curve"}]` in
physical pixels (curves also `"color":[r,g,b]`): one entry per drawn run
(the box of its glyphs' ink, from `Text::draw`) and per graph curve (the
stroke's box clipped to the plot). Icons are paths, not text, and are not
listed; the edge rule covers them. Unset, nothing is recorded.

`goldens.sh OUTDIR BINDIR [gmnb|dgmnb|all]` is the isolated capture script
(private weston, D-Bus and XDG); DGMNB's pictures come with their JSON.
`BOXES= goldens.sh ...` leaves the hook unset (used to check the pictures are
byte-identical with and without it).

Tests: `cargo test` and `cargo clippy -- -D warnings` in `tools/theming`
(synthetic images: identical passes, a 3 px shift fails the flat rule, a 1 px
border shift passes, a recoloured text box fails the text rule).
