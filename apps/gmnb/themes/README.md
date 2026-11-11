# GMNB's built-in themes

Each folder here is one theme GMNB comes with. GMNB carries their files
and writes each folder into the user's themes folder the first time it
starts; from then on it is a folder like any other there, to edit or
delete, and Settings → Theme → *Restore built-in themes* brings back one
that was deleted ([The themes folder](../../../docs/theming.md#the-themes-folder)).
The contract they are written against is
[docs/theming.md](../../../docs/theming.md).

| Folder | What it is |
|---|---|
| `aurora/` | The look GMNB had up to 0.2, grown since: an aurora behind glass keys, in seven palettes. It uses every part of the API (style, three draw layers, options with drawn previews, its own font, sounds), and works its colours out from each palette so that all of them stay readable. Its sounds are glass and breath, synthesised from two recipes, with a chord for each palette. |
| `sprinkles/` | An 8-bit doughnut shop that takes the app's name at its word: press = and the number comes out glazed, with sprinkles. Where Aurora is computed light, this one is pictures and matter: stills drawn by a script (`make_assets.py`: a tiled wall, the hem of an awning cut out of masks and filled with the flavour's colours), a GIF loop that repeats in space as well as in time (sprinkles falling), two pictures the theme works out pixel by pixel (`gmnb.pixels`: the icing shown through the digits' own shapes, and the doughnut that lives on the readout's shelf), keys moved by `place` (they sink, hop and stack themselves), an overlay (a fountain of sprinkles from the = key), a pixel typeface (Jersey 10) and chip-tune sounds in which every flavour has its own scale. Nothing eases: things step and land, thirty times a second at most. Five flavours, and two of your own: System, iced in the desktop's accent colour, and Freestyle, in an icing and a second colour you pick; any colour works, and the shop's inks are worked out to read on it. |
| `swirls/` | Flowing paint inspired by ly's ColorMix, with a calculator of dark glass standing in it. The worked example of `draw:field`: see below. |

Sprinkles' typeface, Jersey 10, is `fonts/ttf/Jersey10-Regular.ttf` of its
upstream, <https://github.com/scfried/soft-type-jersey>, at commit
`d8446c4c9c2ba14cf408c295be35213c006e19ff`, unchanged (SHA-256
`db9cbd091617048a145d249daa2b815fe7083be6ab66ac26626e21a4e01c3e82`), with
that repository's `OFL.txt` beside it.

Every file in `sprinkles/images/` is written by `sprinkles/make_assets.py`, which
needs nothing but Python (`python3 -I make_assets.py --check` says whether
the files are what it would write). A picture added there is added to the
theme's list in `crates/gmnb-theme/src/theme.rs` too: a built-in theme's
files are compiled into the app one by one, and what is listed there is
what GMNB writes into the themes folder (`make_assets.py` isn't).

GMNB's base look is not a theme. It is what a theme that returns `{}` looks
like: the same as DGMNB.

## Swirls

The design's background is the ColorMix field of
[ly](https://github.com/fairyglade/ly) (WTFPL): a coordinate bent three
times by sines and cosines of itself, its length pushed through three
bands of colour, gold `#ffcf29`, cyan `#0ed6f0` and near-black `#010609`.
ColorMix works it out in a fragment shader. The theme works out the bent
coordinate in Luau on a grid of about 3,600 points, five times a second,
and the app colours every pixel of it thirty times a second through a
`gmnb.ramp` ([Fields](../../../docs/theming-media.md#fields-swirls-plasma-contours)).
Everything else in the theme is built on that one picture.

| | |
|---|---|
| The keys and panes | Dark glass over the paint, with the design's own cues: its panels (`rgba(1, 6, 9, .91)`, a cyan hairline, radius 12), its hover (`#13252b`, a gold border), its gold focus ring, its text colour at weight 500. A pressed key clears and the paint shows through it, then fogs over again. |
| The paint | Flows at the design's pace. A press stirs it where it lands (a twist that swings back), typing hurries it along, a result sends a ripple out from the readout, an error shudders. All of it by moving where the field is looked up, on the grid the theme already has. |
| The figures | The readout's value, the converter's two and the date page's results are cut out of the same paint: a slice of the swirl through the first band of the same ramp only, from the warm colour to the cool one, so that no figure is ever written in the dark one. |
| The keypad | Its keys come round into their places whenever it comes into view (`place`). Nothing is drawn over the keys. |
| The sounds | Drops. Number keys fall on seven notes of one five-note scale, each key always its own, and a result rings the last three notes that were typed, lowest first. An error goes down the drain. |

Its options: six sets of bands (the design's own first) and two of your
own (System, worked out from the desktop's accent colour, and Freestyle,
from three colours you pick: the dark one is kept near black, as the
glass is made of it, and the bright ones are lightened only as far as
the figures painted from them need), three strengths of glass, the pace from 0 (still) to 4 times the design's, and switches for
the stirring and for the painted figures. The two choices show tiles the
theme draws with small fields. It is one look in light and in dark, as
the page it comes from has one.

**Readable over paint that moves.** White text on the design's gold is 1.4
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

1. Settings → Theme → *Open themes folder* opens the themes folder,
   `~/.local/share/gmnb/themes/` (in the Flatpak:
   `~/.var/app/io.github.Go08er.GlazeMyNumbersBaby/data/gmnb/themes/`).
   The built-in themes are there: copy a folder to `mine/` (any id of lower-case
   letters, digits, `-` and `_`). While Settings is open, a folder dragged
   in shows in its list.
2. Edit `theme.luau`. Settings → Theme → Reload picks up what you saved.
   (You can edit the built-in themes' folders in place too: a newer GMNB
   doesn't write over a folder you changed.)
3. For autocomplete, point luau-lsp at `types/gmnb.d.luau` (the file's
   first lines say how).

## What a theme can do

Everything in [the contract](../../../docs/theming.md) is in the app: a
theme's `style` restyles every element the reference lists (the window,
keys, readout, panels, pages, the toolkit's own dialogs and menus), its
options show in Settings (a choice with a `preview` as tiles the theme
draws), and its `background`, `keypad`, `display` and `overlay` layers
draw from the theme's Luau on the app's frame clock and events.

Pictures from files (`gmnb.image`, `gmnb.animation`) and pictures a theme
computes (`gmnb.pixels`, `draw:field` and `gmnb.ramp`) are drawn: Sprinkles
is made of the first, Swirls of the second.

Sounds (`sounds`, `sound`, `gmnb.synth`, `gmnb.sound`) are played by the
app (`crates/gmnb-sound`; Settings → Sound turns them off and sets their
volume). They can also be rendered to WAV files, measured and drawn
without the app, decided as the app decides them:
[Designing sounds](../../../docs/theming-media.md#designing-sounds-a-few-rules-that-work).
