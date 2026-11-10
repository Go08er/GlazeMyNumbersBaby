# GMNB's built-in themes

Each folder here is one theme, built into the app. The contract they are
written against is [docs/theming.md](../../../docs/theming.md).

| Folder | What it is |
|---|---|
| `aurora/` | The look GMNB had up to 0.2, grown since: an aurora behind glass keys, in seven palettes. It uses every part of the API (style, three draw layers, options with drawn previews, its own font, sounds), and works its colours out from each palette so that all of them stay readable. Its sounds are glass and breath, synthesised from two recipes, with a chord for each palette. |
| `mono/` | The base look with a different accent, corners and typeface, in twenty lines. Start here. |
| `glazed/` | An 8-bit doughnut shop that takes the app's name at its word: press = and the number comes out glazed, with sprinkles. Where Aurora is computed light, this one is pictures and matter: stills drawn by a script (`make_assets.py`: a tiled wall, the hem of an awning cut out of masks and filled with the flavour's colours), a GIF loop that repeats in space as well as in time (sprinkles falling), two pictures the theme works out pixel by pixel (`gmnb.pixels`: the icing shown through the digits' own shapes, and the doughnut that lives on the readout's shelf), keys moved by `place` (they sink, hop and stack themselves), an overlay (a fountain of sprinkles from the = key), a pixel typeface (Jersey 10) and chip-tune sounds in which every flavour has its own scale. Nothing eases: things step and land, thirty times a second at most. Five flavours. |

Glazed's typeface, Jersey 10, is `fonts/ttf/Jersey10-Regular.ttf` of its
upstream, <https://github.com/scfried/soft-type-jersey>, at commit
`d8446c4c9c2ba14cf408c295be35213c006e19ff`, unchanged (SHA-256
`db9cbd091617048a145d249daa2b815fe7083be6ab66ac26626e21a4e01c3e82`), with
that repository's `OFL.txt` beside it.

Every file in `glazed/images/` is written by `glazed/make_assets.py`, which
needs nothing but Python (`python3 -I make_assets.py --check` says whether
the files are what it would write). A picture added there is added to the
theme's list in `crates/gmnb-theme/src/theme.rs` too: a built-in theme's
files are compiled into the app one by one.

GMNB's base look is not a theme. It is what a theme that returns `{}` looks
like: the same as DGMNB.

## Making your own

1. Copy `mono/` to `~/.local/share/gmnb/themes/mine/` (in the Flatpak:
   `~/.var/app/io.github.Go08er.GlazeMyNumbersBaby/data/gmnb/themes/mine/`).
   Settings has a button that opens this folder.
2. Edit `theme.luau`. Settings → Theme → Reload picks up what you saved.
3. For autocomplete, point luau-lsp at `types/gmnb.d.luau` (the file's
   first lines say how).

## What is drawn so far

The style pipeline is in: a theme's `style` restyles the window, the keys,
segmented controls, the History/Memory switch and the readout, and its
options show in Settings, a choice with a `preview` as tiles the theme
draws. The other elements are accepted (a theme returning them is checked)
but still drawn the old way from the theme's tokens.

The draw layers are in: `background`, `keypad`, `display` and `overlay`
run from the theme's Luau, on the app's frame clock and events. Aurora's
drift, grain, pulses, ripples, pointer light, key cascade and every
effect of its readout are in its `theme.luau`. Pictures from files
(`gmnb.image`) are sized and counted but not drawn yet: decoding them
comes with the media chapter's code.

Sounds are declared and checked (`sounds`, `sound`, `gmnb.synth`), and the
app plays them (`crates/gmnb-sound`; Settings → Sound turns them off and
sets their volume). They can also be rendered to WAV files, measured and
drawn without the app, decided as the app decides them:
[Designing sounds](../../../docs/theming-media.md#designing-sounds-a-few-rules-that-work).
