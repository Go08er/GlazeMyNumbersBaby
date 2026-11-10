# GMNB's built-in themes

Each folder here is one theme, built into the app. The contract they are
written against is [docs/theming.md](../../../docs/theming.md).

| Folder | What it is |
|---|---|
| `aurora/` | The look GMNB had up to 0.2, grown since: an aurora behind glass keys, in seven palettes. It uses every part of the API (style, three draw layers, options with drawn previews, its own font), and works its colours out from each palette so that all of them stay readable. |
| `mono/` | The base look with a different accent, corners and typeface, in twenty lines. Start here. |

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
