# GMNB's built-in themes

Each folder here is one theme, built into the app. The contract they are
written against is [docs/theming.md](../../../docs/theming.md).

| Folder | What it is |
|---|---|
| `aurora/` | The look GMNB had up to 0.2: an aurora behind glass keys, in seven palettes. It uses every part of the API (style, three draw layers, options with drawn previews, its own font). |
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

## Not done yet

`aurora/theme.luau` expects `fonts/Outfit-Variable.ttf` and
`fonts/OFL-Outfit.txt` beside it. They are still in
`apps/gmnb/assets/fonts/` and move here with the theme engine.
