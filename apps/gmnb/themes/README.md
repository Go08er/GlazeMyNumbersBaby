# GMNB's built-in themes

Each folder here is one theme, built into the app. The contract they are
written against is [docs/theming.md](../../../docs/theming.md).

| Folder | What it is |
|---|---|
| `aurora/` | The look GMNB had up to 0.2, grown since: an aurora behind glass keys, in seven palettes. It uses every part of the API (style, three draw layers, options with drawn previews, its own font, sounds), and works its colours out from each palette so that all of them stay readable. Its sounds are glass and breath, synthesised from two recipes, with a chord for each palette. |
| `mono/` | The base look with a different accent, corners and typeface, in twenty lines. Start here. |
| `swirls/` | The paint that flows behind s0y b0y, the homepage of GMNB's owner, with a calculator of dark glass standing in it. The worked example of `draw:field`: see below. |

GMNB's base look is not a theme. It is what a theme that returns `{}` looks
like: the same as DGMNB.

## Swirls

The homepage's background is the ColorMix field of
[ly](https://github.com/fairyglade/ly) (WTFPL): a coordinate bent three
times by sines and cosines of itself, its length pushed through three
bands of colour, gold `#ffcf29`, cyan `#0ed6f0` and near-black `#010609`.
The page works it out in a fragment shader. The theme works out the bent
coordinate in Luau on a grid of about 3,600 points, five times a second,
and the app colours every pixel of it thirty times a second through a
`gmnb.ramp` ([Fields](../../../docs/theming-media.md#fields-swirls-plasma-contours)).
Everything else in the theme is built on that one picture.

| | |
|---|---|
| The keys and panes | Dark glass over the paint, with the page's own cues: its panels (`rgba(1, 6, 9, .91)`, a cyan hairline, radius 12), its hover (`#13252b`, a gold border), its gold focus ring, its text colour at weight 500. A pressed key clears and the paint shows through it, then fogs over again. |
| The paint | Flows at the page's pace. A press stirs it where it lands (a twist that swings back), typing hurries it along, a result sends a ripple out from the readout, an error shudders. All of it by moving where the field is looked up, on the grid the theme already has. |
| The figures | The readout's value, the converter's two and the date page's results are cut out of the same paint: a slice of the swirl through the first band of the same ramp only, from the warm colour to the cool one, so that no figure is ever written in the dark one. |
| The keypad | Its keys come round into their places whenever it comes into view (`place`). Nothing is drawn over the keys. |
| The sounds | Drops. Number keys fall on seven notes of one five-note scale, each key always its own, and a result rings the last three notes that were typed, lowest first. An error goes down the drain. |

Its options: six sets of bands (the page's own first), three strengths of
glass, the pace from 0 (still) to 4 times the page's, and switches for
the stirring and for the painted figures. The two choices show tiles the
theme draws with small fields. It is one look in light and in dark, as
the page it comes from has one.

**Readable over paint that moves.** White text on the page's gold is 1.4
to 1, so the glass is never chosen by eye: for each set of bands, at each
window opacity, the theme works out the least strength of glass at which
every ink still reads over every band, and is never thinner than that
("Clear" is that strength, or 0.6 where the text allows it). The tests
then put each of the 768 colours the set's ramp holds under every text of
the readability table in turn
(`swirls_texts_read_over_every_colour_of_the_field`), and hold the layers
to that table: every pixel they draw, stirred or still, is one of those
colours (`crates/gmnb-theme/tests/swirls.rs`).

**What it costs.** A grid of the same number of points, and a picture
of about the same number of pixels, whatever the window's size (a 4K
window gets a coarser grid, not more to work out); the next snapshot
worked out a few rows a frame, so that no frame carries a whole one; and
a clock quantised to thirtieths of a second, so that a frame drawn in
between (an event, a change of colours) asks for the numbers it had and
the app hands back the picture already made. A machine too slow for the
grid gets a coarser one rather than losing the theme to the 8 ms limit.
The figures are in
[the contract](../../../docs/theming.md#how-each-of-swirls-effects-is-made).

The demonstration it grew from is still in
`crates/gmnb-theme/tests/themes/swirls/`: the field alone, for the
engine's tests.

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

Pictures a theme computes are in too (`gmnb.pixels`, `draw:field` and
`gmnb.ramp`): Swirls is made of one.

Sounds are declared and checked (`sounds`, `sound`, `gmnb.synth`), and the
app plays them (`crates/gmnb-sound`; Settings → Sound turns them off and
sets their volume). They can also be rendered to WAV files, measured and
drawn without the app, decided as the app decides them:
[Designing sounds](../../../docs/theming-media.md#designing-sounds-a-few-rules-that-work).
