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

## The whole comparison, one command

From the repo root, inside the nix dev shell, with the private headless
weston running (socket `wincalc-shot`) and release builds of both apps in
`target/release`:

    tools/theming/goldens.sh OUT target/release base

It captures every pair of `base.map` and `least.map`, both apps, each in a
fresh profile (temporary XDG directories, a private D-Bus with no
services, no display but the private weston), then compares them: the
table, every failing run of text, and a diff picture per pair in
`OUT/report`. The exit status is `compare`'s for `base.map`: 0 when every
pair passes.

- `base.map` is the contract's set (docs/theming.md, "How it is
  measured"), light and dark: the seven pages at 760x640, Standard after
  `1234*56=789/3=`, the sidebar, Programmer with the bit grid, the compact
  window, Date with a result and adding days, Graphing with three
  equations, Standard at 360x600. Its only exclusion is the contract's:
  the app's name in the sidebar's header.
- The pictures are taken offline, in a namespace with no network
  (`unshare -rn`): the converter shows the rates it is shipped with, and
  their time, in both apps, so there is no live data to exclude. `NET=online`
  takes them with the network (the currency's rates and their time are
  then whatever each app fetched, and when).
- GMNB is drawn by the toolkit's software renderer (`GSK_RENDERER=cairo`
  unless set), so the pixels don't depend on the GPU.
- `least.map`: Scientific and Programmer at DGMNB's least window,
  300x400. Reported after `base.map`, not in the exit status. Both fit
  the window as DGMNB's do; what still differs: in Scientific, four keys
  whose text is taller than their 15 px key (the glyphs are DGMNB's to
  1/255, but the key's edge under them is on a fraction of a pixel in
  DGMNB and on a whole one in GMNB, which moves the text rule's estimate
  of the background), and the edges of icons drawn by two rasterisers,
  which at this size are more than 1% of the picture's edges; in
  Programmer, `M▾` and `MS`, which DGMNB squeezes into no room at the
  bar's end and shrinks to 10 px, where GMNB's buttons keep their least
  width. The other pages at 300x400 (round 4): Standard, Date and
  Graphing pass; Currency and Length differ by the converter's scroll
  thumb alone, which both apps show, GMNB's 4 px to the right and 3 px
  taller (the page's ring room is inside its view).
- `ONLY=REGEX` captures and compares only the cases it matches
  (`ONLY='date|currency'`), keeping the other pictures in `OUT`.
- `COMPARE=path` uses that `compare` binary (else it is built here).

`goldens.sh OUTDIR BINDIR [gmnb|dgmnb|all|pages]` is the isolated capture script
(private weston, D-Bus and XDG); DGMNB's pictures come with their JSON.
`BOXES= goldens.sh ...` leaves the hook unset (used to check the pictures are
byte-identical with and without it).
`pages` captures both apps, GMNB in its base look with DGMNB's accent, on
the date page (and with a result), Currency, Length and Graphing (empty and
with three equations), light and dark, each in a fresh profile; compare them
with `compare --batch OUT OUT --map pages.map`.

Tests: `cargo test` and `cargo clippy -- -D warnings` in `tools/theming`
(synthetic images: identical passes, a 3 px shift fails the flat rule, a 1 px
border shift passes, a recoloured text box fails the text rule).

## The frame-cost probe

`crates/gmnb-theme/src/cost.rs` charges each call a theme records against a
frame's allowance (1000 points a 760 x 640, about 100 ms of the software
renderer). The probe, `apps/gmnb/src/layers/costprobe.rs`, is how those
weights are checked: an `#[ignore]` test that records a layer of
repeats of one kind of call through the app's own replay
(`replay::replay`), renders the picture with `gsk::CairoRenderer`, and
prints for each case the repeats, the charge in ms, the real ms (median of
several warm frames, this thread's CPU time, above an empty frame's) and
their ratio. It ends with a summary: the dearest ratios (calls the charge
under-states: over 1.5 is a finding), the cheapest (over-charged), and the
worst of each group. Kinds covered: rect and gradients, rounded rects by
radius, circle, fill, stroke, dashes (butt, round, square, wide), path
strain, border, outset and inset shadows over sizes, blurs, radii, spreads,
bars and keys, `push_shadow`, blur, opacity, the four masks, the fifteen
blends, rounded and turned clips, text (Latin, many characters, CJK, Arabic,
emoji, moving sizes and weights), glyph, pictures, tiles.

It needs a display (the private weston) and, as every run of this project,
the limits and isolation in `theming-work/briefs/house-rules.md`:

    export CARGO_INCREMENTAL=0
    env -u DBUS_SESSION_BUS_ADDRESS WAYLAND_DISPLAY=wincalc-shot GMNB_SOUND=null \
        XDG_CONFIG_HOME=$T/c XDG_DATA_HOME=$T/d XDG_CACHE_HOME=$T/k XDG_STATE_HOME=$T/s \
        LIM_JOBS=2 theming-work/tools/lim nix develop -c \
        dbus-run-session --config-file=tools/theming/privbus.conf -- \
        cargo test --release -p gmnb --bin gmnb costprobe -- --ignored --nocapture \
        | grep '^PROBE'

(`$T` a temporary folder.) Always `--release` for numbers: the first build is
long, a full run about 40 s. Settings, all environment variables:

| Variable | Meaning | Default |
|---|---|---|
| `GMNB_PROBE_ONLY` | Comma-separated substrings of `group/case`, to run only those (`shadow/`, `shadow/out_s200,dash/`) | all |
| `GMNB_PROBE_TARGET` | The charge, as a share of the allowance, each case is sized to by its repeats (a larger one makes small real times less noisy) | 0.3 |
| `GMNB_PROBE_FRAMES` | Warm frames timed (the first, a warm-up, is not) | 5 |
| `GMNB_PROBE_SIZE` | The layer's size, `WxH` | 760x640 |
| `GMNB_PROBE_DIR` | Scratch folder (the theme and the font) | a temporary one |

Reading it: a case under 3 ms real is marked noisy (raise the target); a
case that is cheap in absolute terms may have a large ratio just because it
is charged almost nothing (a 4 x 4 shadow of no blur: 8 us real a call, so
at most 4096 calls, 33 ms); look at ratios of cases that fill a good part of
a frame. The timings move with the machine (a fifth, run to run) and with
what else it is doing; the shape (which kinds and parameters are dear) is
what to trust. Translucent paints are used so the toolkit can't skip what is
covered over.
