# Theming GMNB

GMNB 0.3 can be restyled by a *theme*: one Luau file that says how the
calculator looks, and can draw behind it, over its keys and in its readout.

This is the contract for theme API **1**. The first half is for people
writing a theme; from [How style reaches the app](#how-style-reaches-the-app)
on it is also the specification the app is built to.

- The matching type definitions are [`types/gmnb.d.luau`](../types/gmnb.d.luau).
- Three themes ship with the app and serve as examples:
  [`aurora`](../apps/gmnb/themes/aurora/theme.luau) (everything a theme
  can compute),
  [`sprinkles`](../apps/gmnb/themes/sprinkles/theme.luau) (pictures, keys that
  move, an overlay, a scale of sounds) and
  [`swirls`](../apps/gmnb/themes/swirls/theme.luau) (a picture the theme
  computes, behind glass).
- Animated pictures, pictures a theme computes, and sound have a chapter
  of their own, [Pictures that move, and sound](theming-media.md). Their
  limits, functions and events are also listed here.

Contents

1. [A theme in twenty lines](#a-theme-in-twenty-lines)
2. [Three layers](#three-layers)
3. [Files and where they go](#files-and-where-they-go)
4. [When a theme runs](#when-a-theme-runs)
5. [`ctx`: what a theme is told](#ctx-what-a-theme-is-told)
6. [Values](#values)
7. [How style reaches the app](#how-style-reaches-the-app)
8. [Reference: tokens](#reference-tokens)
9. [Reference: every element](#reference-every-element)
10. [Drawing](#drawing)
11. [Options](#options)
12. [The `gmnb` library](#the-gmnb-library)
13. [Limits, and what happens when a theme fails](#limits-and-what-happens-when-a-theme-fails)
14. [Versions](#versions)
15. [What is scripted and what is built in](#what-is-scripted-and-what-is-built-in)
16. [Acceptance: the base look is DGMNB's](#acceptance-the-base-look-is-dgmnbs)
17. [Acceptance: Aurora carries 0.2 on](#acceptance-aurora-carries-02-on)
18. [Readability rules](#readability-rules)
19. [For implementers](#for-implementers)

---------------------------------------------------------------------------

## A theme in twenty lines

```lua
-- ~/.local/share/gmnb/themes/typewriter/theme.luau
return {
	api = 1,
	name = "Typewriter",
	description = "Tight corners, a monospaced face, one accent of your choosing.",
	options = {
		{ id = "accent", type = "color", label = "Accent", default = "#e5484d" },
		{ id = "square", type = "toggle", label = "Square corners", default = true },
	},
	style = function(ctx)
		local corner = if ctx.options.square then 2 else 8
		return {
			colors = gmnb.base(ctx.dark, ctx.options.accent),
			fonts = { ui = "monospace", display = "monospace" },
			shape = { radius = corner, row_radius = corner, card_radius = corner * 2 },
			keys = { gap = 2 },
		}
	end,
}
```

What this shows:

- A theme is a table. `api` and `name` are required; everything else is optional.
- `style(ctx)` returns **only what differs from the base look**. A theme
  whose `style` returns `{}` looks exactly like GMNB without a theme.
- A few values, the *tokens* (`colors`, `fonts`, `text`, `shape`), drive most
  of the look. Change the accent and the equals key, the lit switches, the
  operators' glyphs and the focus ring all follow.
- `options` appear in Settings under the theme. Their values arrive in
  `ctx.options`.

Going further, a theme can style single parts and draw:

```lua
local night = gmnb.linear({ angle = 180, colors = { "#10131c", "#05060a" } })

return {
	api = 1,
	name = "Night",
	style = function(ctx)
		local c = ctx.base.colors
		return {
			keys = {
				radius = 14,
				equals = {
					background = gmnb.linear({ angle = 135, colors = { "#ff7ad9", "#8a63ff" } }),
					shadow = { { y = 8, blur = 24, spread = -8, color = gmnb.alpha("#8a63ff", 0.7) } },
				},
				hover = { brightness = 1.1 },
				active = { scale = 0.95 },
				transition = { transform = { ms = 250, ease = "spring" }, filter = 120 },
			},
			display = { value = { color = gmnb.linear({ angle = 180, colors = { c.fg, c.accent_text } }) } },
		}
	end,
	layers = {
		background = {
			draw = function(draw, frame)
				draw:rect(0, 0, frame.width, frame.height, night)
				-- One slow star.
				local x = (frame.time * 12) % frame.width
				draw:circle(x, 40, 1.5, "#ffffff")
				return 1 / 15 -- again in a fifteenth of a second
			end,
		},
	},
}
```

## Three layers

1. **Structure.** The widgets, their order, what they do, what a screen
   reader hears, where the keyboard focus goes. A theme cannot change it.
2. **The base look.** Every colour, size, radius and font has a base value,
   worked out by the app. They are DGMNB's: GMNB without a theme looks like
   DGMNB, plus the window opacity DGMNB doesn't have.
3. **The theme.** Typed values that replace base values, and functions that
   draw.

A theme never writes CSS. It returns colours, numbers, names and lists; the
app checks each one and writes the style itself. That keeps a theme from
reading files through the toolkit's `url()`, and it means a typo is a
message in Settings rather than a broken window.

## Files and where they go

```
mytheme/
  theme.luau        the theme (required)
  fonts/            font files it names in `fonts`
  images/           pictures it loads with gmnb.image
  sounds/           sound files it loads with gmnb.sound
```

- The folder's name is the theme's **id**: lower-case letters, digits, `-`
  and `_`, at most 32 characters, starting with a letter or digit.
- Every theme lives in the **themes folder**, `$XDG_DATA_HOME/gmnb/themes/<id>/`,
  usually `~/.local/share/gmnb/themes/<id>/`: yours, and the ones GMNB
  comes with.
- **Flatpak:** the sandbox sets `XDG_DATA_HOME` itself, so the same folder is
  `~/.var/app/io.github.Go08er.GlazeMyNumbersBaby/data/gmnb/themes/<id>/`.
  No extra permission is needed. Settings → Theme → *Open themes folder*
  opens it either way (see [The themes folder](#the-themes-folder)).
- `theme.luau` is Luau source text, at most 512 KiB. Compiled bytecode is
  refused.
- A theme cannot `require` other files, and has no `loadstring`, `io`,
  `os.execute` or `debug`. The rest of Luau's standard library is there.

### The themes folder

GMNB comes with three themes, `aurora`, `sprinkles` and `swirls`
(from `apps/gmnb/themes/`). It carries their files, and writes each one's
folder into the themes folder the first time it starts for you. From then
on they are folders like any other. This replaces "your copy" (a folder
of yours with a built-in theme's id, which stood in for the theme compiled
into the app): there is no copy any more, only the folder, and it is
yours to edit, delete or replace.

- **Deleted, it stays deleted.** GMNB records what it wrote
  (`built_in_themes` in `settings.json`: each id and a SHA-256 digest of
  the folder), so it never writes a theme it wrote once again of its own
  accord. Settings → Theme → *Restore built-in themes*, shown while one of
  them has no folder, writes each missing one again; it never writes over
  a folder that is there.
- **Changed, it is kept.** A newer GMNB that carries a changed version
  replaces a built-in theme's folder only if the folder is still exactly
  what GMNB wrote (the digests match). A folder you changed is kept,
  without a word. For the new version, delete your folder (or move it
  aside) and restore.
- **A folder of yours with a built-in theme's id**, one GMNB didn't write,
  is yours: GMNB never writes to it. (One that is exactly what GMNB
  carries counts as GMNB's: written by another GMNB, or by one that
  stopped before it saved its settings.)
- **Never over anything, never through a link.** A folder is written into
  a new hidden folder in the themes folder (`.gmnb-…`, a name no theme can
  have, so no list shows it) and renamed into place only if nothing is at
  its name by then (`RENAME_NOREPLACE`: not even an empty folder is
  replaced). Each file is created new, and nothing follows a link where a
  theme's folder or one of its files goes. A hidden folder left behind by a
  GMNB that stopped while writing is cleared away an hour later.
- **If a folder can't be written** (a read-only home, a full disk), GMNB
  uses its own copy of the theme for that run, says so on stderr, and
  tries again the next time it starts. A run that touches no saved state
  (`GMNB_SCREENSHOT` without `GMNB_REAL_STORE`) writes nothing: GMNB's own
  copies stand in for the built-in themes that have no folder.
- **The list.** Settings lists the base look ("GMNB", which is GMNB itself:
  it has no folder, can't be deleted and is always first), then the
  built-in themes' ids in the order above where there is a folder, then
  every other theme folder by id, then `GMNB_THEME_DIR`'s. A theme GMNB
  put in the folder is marked "Comes with GMNB". Besides the built-in
  themes and the one chosen (wherever it falls), at most 64 theme folders
  are listed, the first by id, and
  loading them for the list takes at most about two seconds in all: a theme
  after that is listed as not checked, so a folder of thousands of themes
  can't hold Settings. About → Legal keeps
  every built-in theme's credits whatever their folders hold: their fonts
  and pictures are in GMNB itself.
- **Dragged in, it shows.** While Settings is open it watches the themes
  folder and each theme's folder in it: a theme dragged in, deleted, or
  whose `theme.luau` changes shows in the list a moment later (once the
  folder has been still for 300 ms). Only folders named like a theme's id
  are watched, and no more than are listed. Nothing watches the folder while
  Settings is closed: GMNB wakes for nothing at rest, and a theme deleted
  then is noticed when Settings next opens or GMNB next starts.
- **The chosen theme deleted** (seen at start, while Settings is open, or
  as Settings opens): the base look, and a toast that says the theme isn't
  installed, as for any theme that is missing or fails (while Settings is
  open, every such toast is Settings' own: the window's would be under it,
  and in a narrow window hidden by it). It stays the
  choice, and is loaded again when its folder is back (restored, or
  dragged in, seen the same way).
- ***Open themes folder*** opens the folder in the app your desktop opens
  folders with, to drag themes into. Outside a sandbox that is the default
  handler for folders (`inode/directory`) your `mimeapps.list` names,
  which GMNB asks GIO for (with none named, the first app installed that
  says it opens folders). In the Flatpak it goes through the desktop
  portal: GTK hands the folder to OpenURI's `OpenFile`, and the portal
  opens a folder with the host's default handler for folders without
  asking. If nothing opens it (no app for folders, the portal refuses), a
  toast says why and where the folder is. Its tooltip gives the path too.

### The theme table

What `theme.luau` returns:

| Field | Type | Meaning |
|---|---|---|
| `api` | number, required | The API version the theme is written for: `1`. |
| `name` | string, required | Shown in Settings. |
| `description`, `author`, `version`, `license` | string | Shown in Settings. |
| `fonts` | list of file paths | Font files (TrueType or OpenType) in the theme's folder. They become available to this app only, under the family names inside them. A font whose family name the app already has (the base look's, the desktop's, an earlier theme's) is left out with a note, for a font stays in the app until it quits and would take the place of the one it is named after, whichever theme is chosen next; so is a file that is not a TrueType or OpenType font. |
| `credits` | list of `{ title, file }` | Shown in About → Legal while the theme is installed: a heading, and a text file from the theme's folder. For font licences. |
| `options` | list | What Settings offers. See [Options](#options). |
| `style` | function | `style(ctx)` → the style table. See the [reference](#reference-tokens). |
| `layers` | table | Draw hooks. See [Drawing](#drawing). |
| `sounds` | table | Which sound goes with which event. See [Sound](theming-media.md#sound). |
| `sound` | function | `sound(event, ctx)` → what to play for one event. |

## When a theme runs

| When | What runs | What the app keeps |
|---|---|---|
| The theme is chosen, at start, or on *Reload* | The file, once, top to bottom. Its return value is the theme. | The theme table; fonts registered; the file's locals live until the theme is unloaded. |
| Right after, and whenever `ctx` changes: light/dark, the desktop's accent, an option, the Background opacity, reduced motion, the desktop's font | `style(ctx)` | The checked result and the style generated from it. Regenerated only if the result differs. |
| A layer needs a new picture (see [Drawing](#drawing)) | That layer's `draw` | The picture, until the layer next needs one. |
| Settings shows the theme's options | `visible`, `preview`, an action's `run` | Each tile's picture until `ctx` changes. `visible` and `preview` are asked in passes of a shared budget ([Limits](#limits-and-what-happens-when-a-theme-fails)), never all at once. |
| Something happens that may have a sound, and theme sounds are on | `sound(event, ctx)` | Nothing. |

Rules the app keeps:

- **A theme with no draw hooks costs nothing at rest.** No timers, no
  frames. The same holds for a theme whose hooks return `nil`.
- The Background opacity slider calls `style` at most once per frame while
  it is dragged: the window follows it on the frame clock, at the value it
  has when each frame begins, however many steps the toolkit reported
  since the last one.
- Settings asks a theme nothing while it is closed: no pass of its calls
  waits, and nothing is armed for it.
- Listing themes in Settings runs each theme's file in a throwaway sandbox
  to read its `name`, `description` and `options`, then discards it.
- Without a theme no Luau machine is created at all.

## `ctx`: what a theme is told

| Field | Type | Meaning |
|---|---|---|
| `dark` | boolean | Dark or light, after the app's Light/Dark/System setting. |
| `accent` | `Rgba?` | The desktop's accent colour exactly as its portal reports it. `nil` if the desktop shares none, or if the user turned "Use the desktop's accent colour" off. |
| `opacity` | number | The Background opacity setting, 0.1 to 1. Below 1 the desktop shows through the window's background, which the app fades for you. |
| `options` | table | This theme's options by id: saved values, else their defaults. Always complete. |
| `motion` | boolean | `false` when the desktop asks for reduced motion. |
| `font` | `{ family, size }` | The desktop's interface font; `size` in pixels. |
| `base.colors` | [Colors](#colors) | The base colours for this `dark` and `accent`. All present, all `Rgba`. |
| `base.text` | [Text styles](#text-fonts) | The base text styles. |
| `base.shape` | [Shape](#shape-icons) | The base radii. |
| `base.text_scrim` | `Rgba` | What the base look puts under text on a see-through window (see [See-through windows](#see-through-windows)). Transparent at opacity 1. |

`ctx` is read-only and the same in `style`, in every `frame.ctx` and in the
options' functions.

## Values

All sizes are in logical pixels. Angles are in degrees.

### Color

`"#rrggbb"`, `"#rrggbbaa"`, or an `Rgba` table `{ r, g, b, a }` with each
part 0 to 1 (`a` may be left out). The `gmnb` colour functions take either
and return `Rgba`.

### Paint, Fill

A **Paint** is a colour or a gradient:

```lua
gmnb.linear({ angle = 135, colors = { "#ff7ad9", "#8a63ff" } })
gmnb.linear({ angle = 180, colors = { a, b, c }, stops = { 0, 0.3, 1 } })
gmnb.radial({ at = { 0.5, 0.4 }, radius = "farthest-corner", colors = { a, b } })
```

- `colors`: 2 to 16. `stops`: where each sits, 0 to 1, one per colour, never
  decreasing; left out, they are spread evenly.
- `angle` (linear) follows CSS: 0 points up, 90 right, 180 down.
- `at` (radial, conic) is the centre as fractions of the box being painted.
- In **draw hooks** a gradient can instead be placed in pixels:
  `from` and `to` (linear), `center`, `radius` and `radius_y` (radial),
  `center` and `angle` (conic). Pixel placement is refused in `style`.

A **Fill** is one paint, or up to 4 layered, topmost first:
`{ gradient, "#00000033" }`.

### Shadow, Border, Radius, Spacing

```lua
{ x = 0, y = 6, blur = 18, spread = -12, color = "#00000052" }   -- a drop shadow
{ inset = true, y = 1, color = gmnb.alpha("#ffffff", 0.1) }      -- a highlight along the top edge
```

- **Shadow**: `color` is required, the rest default to 0 / `false`. A
  shadow list has at most 8, topmost first.
- **Border** `{ width, color }` is drawn inside the element's edge. It never
  changes the element's size. `border = false` removes a base border.
- **Radius**: a number, or four (`{ top_left, top_right, bottom_right,
  bottom_left }`). 999 or more is fully round.
- **Spacing**: one number, `{ vertical, horizontal }`, or
  `{ top, right, bottom, left }`.

### Font

```lua
{ family = "Outfit", size = 22, weight = 450, spacing = 0.01,
  italic = false, features = { "tnum" }, variations = { opsz = 24 } }
```

- Every field is optional and fonts merge field by field:
  `font = { size = 22 }` changes only the size.
- `family` is a name or a list of names to fall back through. Font files
  in the theme's `fonts` list are known by the family name inside the file.
- `spacing` is letter spacing in em. `features` are OpenType tags
  (`"tnum"`, or `"ss01=0"` to switch one off). `variations` set a variable
  font's axes.
- **Optical size.** For the bundled Inter the app sets the `opsz` axis to
  the size in pixels, kept between 14 and 32, unless `variations.opsz` says
  otherwise. This is DGMNB's rule; left alone, the toolkit would use the
  size in points instead. A readout the app shrinks to fit its width
  (the calculator's, the converter's, the date page's) is shrunk by as
  much as it is too wide at its largest size, and a smaller optical size
  is a wider cut: set in Inter, a shrunk readout can come out a little
  longer than its width. Naming `variations.opsz` (the largest size's,
  at most 32) keeps one cut at every size, as Swirls' figures do.

### Transition

Which properties ease when an element changes state, and for how long:

```lua
transition = { background = 160, shadow = 220, transform = { ms = 320, ease = "spring" } }
```

Properties: `background`, `color`, `shadow`, `transform`, `filter`,
`opacity`. A duration is milliseconds (0 to 5000), or `{ ms, ease }`. Eases:
`"linear"`, `"ease"`, `"ease-in"`, `"ease-out"` (the default),
`"ease-in-out"`, `"spring"` (overshoots a little:
`cubic-bezier(0.34, 1.56, 0.64, 1)`), or four numbers for a cubic Bézier.
The base look has no transitions. With reduced motion the toolkit skips
them all.

### Text, Surface, Control, State

Most elements are one of three kinds.

| Kind | Properties |
|---|---|
| **Text** | `color`, `font`, `shadow` (a text shadow list), `margin` |
| **Surface** | `background` (Fill), `border`, `shadow`, `radius`, `padding`, `margin` |
| **Control** | all of Surface, plus `color`, `font`, `min_width`, `min_height`, `opacity`, `icon`, `transition`, and the states `hover`, `active`, `checked`, `disabled` |

A **State** says how a control differs while in that state; what it leaves
out stays as at rest:

| Property | Meaning |
|---|---|
| `background` | Replaces the background. |
| `overlay` | A colour laid over whatever the background is. The base look shows hover and press this way. |
| `color` | Text and icon colour. |
| `fade` | Text and icon at this opacity, the background unchanged. The base look shows a disabled key this way. |
| `border`, `shadow` | Replace them. |
| `opacity` | The whole control. |
| `brightness`, `saturation` | Filters; 1 is unchanged. |
| `scale`, `rotate` | Transforms about the centre. They don't move anything else. |
| `font` | Only `weight` is read. |
| `icon` | Only `color` is read. |
| `transition` | The transition into this state (a quick press, a slow release). |

`active` is "being pressed", and also the short flash a key shows when it
is typed on the keyboard. Which states an element has is in the reference;
`checked` means whatever "on" is for it (a toggle that is on, the chosen
tab, the selected row, today's date picked).

When states combine they apply in this order, later ones winning:
rest, `checked`, `hover` or `active`, `disabled`. An `overlay` always lands
on top of the background then in effect.

**Icon** `{ size, stroke, color }`: the app's icons are lines on a 24-unit
grid. `size` is the drawn size and `stroke` the line's width, both in
pixels. Without `color` an icon takes its control's.

**FocusRing** `{ width, color, offset, radius, edge }`: the ring around
what holds the keyboard focus. It shows for keyboard use only; a click
never draws it. `offset` is how far out from the element's edge the
ring's outer edge is (negative: inside); the ring is `width` wide inside
that, so DGMNB's `{ width 2, offset 2 }` touches the element. Its corners
are the element's radius plus `offset`. `edge` (a Border, or `false`) is
a line along both sides of the ring around a keypad's keys (the outer
side only where the ring touches the key): keys lie on the window's
background and whatever the theme draws there, which may be the ring's
own colour. A ring and an edge 9:1 apart read on anything: on any colour,
one of the two reaches 3:1. Elsewhere controls lie on the style's own
surfaces, and the ring is drawn without it. The ring is never hidden: see
[rule 5](#readability-rules).

---------------------------------------------------------------------------

## How style reaches the app

This section is the specification of the style pipeline. Theme authors
can skip to the [reference](#reference-tokens).

### The pieces

| Piece | Where | What it holds |
|---|---|---|
| Structure stylesheet | `apps/gmnb/src/style.css`, static, loaded once | Selectors and layout. Every look property reads a custom property: `background-color: var(--gmnb-keys-number-background-color)`. No colour, size or font is written in it. |
| Base values | `appcore` (colours: `palette::Palette::new(dark, accent)`; measurements: `metrics`; both apps share them) and `crates/gmnb-theme` (`schema.rs`: one table of every path, its kind and how its base derives from the tokens) | The value of every token and element for a `ctx`. |
| Theme engine | `crates/gmnb-theme` (GMNB only; DGMNB never links it) | The Luau machine, the checks, the merge, the generated style. |
| Generated style | two `GtkCssProvider`s: *main*, reloaded when the resolved style changes, and *live* above it, for the few properties a drag changes | `:root { --gmnb-…: …; }`, the toolkit's own named colours, and nothing else. |
| `Style` struct | `gmnb_theme::Style`, handed to widgets that draw themselves | The same resolved values, typed, for the readout, the keypad, the graph, the icons and the layout. |

### Resolving a style

1. `T0` = base tokens for `ctx` (`colors`, `fonts`, `text`, `shape`,
   `icons`, `motion`, `layout`).
2. `T` = the theme's token tables merged over `T0`.
3. `E0` = base elements computed **from `T`**. Each base value in the
   reference is written in terms of tokens for this reason: a theme that
   only changes `colors.accent` changes everything derived from it.
4. `E` = the theme's element tables merged over `E0`.
5. Check and clamp `E`, then generate.

Merging is by key, recursively, with three exceptions that are taken
whole:

- **Lists** (shadows, fill layers, font features, `native`).
- **States** (`hover`, `active`, `checked`, `disabled`, a field's `focus`
  and `error`, `keys.on`, `converter.whimsy`, `graph.row_error`). A
  theme's `hover = { brightness = 1.1 }` means exactly that: the base
  look's hover overlay is gone.
- **Transitions.**

`false` clears a value that can be absent (`border`, `glow`, `marker`,
`divider`). A key the API doesn't know, or a value of the wrong type, is
reported and skipped: the base value stays.

A key class's own `hover`, `active` or `disabled` replaces the shared
`keys.hover`, `keys.active` or `keys.disabled` for that class.

### Names of the custom properties

A style path becomes a property name by joining its parts with `-` and
turning `_` into `-`, after `--gmnb-`:

`keys.number.min_height` → `--gmnb-keys-number-min-height`

A font is named after the element it is the font of: `window.font` gives
`--gmnb-window-font-size`. Values that are tables expand into several
properties:

| Value at path `P` | Properties emitted |
|---|---|
| Fill | `--P-color` (the solid bottom layer, or `transparent`) and `--P-image` (the gradient layers as a `background-image` list, or `none`) |
| Border + shadow list | one `--P-shadow`: the border as `inset 0 0 0 <width> <color>` first, then the shadows; `none` if empty. (`P` is the element; there is no separate border property.) |
| Radius | `--P-radius` |
| Spacing | `--P-padding`, `--P-margin` |
| Font | `--P-font-family`, `--P-font-size`, `--P-font-weight`, `--P-font-style`, `--P-letter-spacing`, `--P-font-features`, `--P-font-variations`, and `--P-line-height` and `--P-text-nudge` (where a line sits: [Text in its place](#text-in-its-place)) |
| Transition | `--P-transition`: a complete `transition` value, or `none` |
| brightness, saturation | `--P-filter`: `brightness(b) saturate(s)`, or `none` |
| scale, rotate | `--P-transform`: `scale(s) rotate(d)`, or `none` |
| Text shadow list | `--P-text-shadow` |
| Icon | `--P-icon-color`; size and stroke go to the icon widget through `Style` |
| FocusRing | `--P-width`, `--P-color`, `--P-offset` |

**States are resolved by the app, not by the stylesheet.** For a control at
`P` and each state combination `S` in `hover`, `active`, `checked`,
`checked-hover`, `checked-active`, `disabled`, `checked-disabled` the app
emits the finished values:

`--P-S-background-color`, `--P-S-background-image`, `--P-S-color`,
`--P-S-icon-color`, `--P-S-shadow`, `--P-S-opacity`, `--P-S-filter`,
`--P-S-transform`, `--P-S-transition`, `--P-S-font-weight`

- An `overlay` becomes the top layer of `--P-S-background-image`, as
  `linear-gradient(<c>, <c>)`.
- `fade` is folded into `--P-S-color` and `--P-S-icon-color`.
- A field has `focus`, `error` and `error-focus` instead.
- Every one of these is always emitted, equal to the resting value where
  the state changes nothing. The stylesheet can therefore name any of them
  without a fallback, and **a property the stylesheet reads that the app
  does not emit is a bug** (there is a test for it).

A full style is about three thousand properties, and reloading that many
costs (measured: 3000 `:root` properties took 80 ms to restyle an 841-node
tree on the software renderer, 220 ms on GL). So:

- **Only the properties the structure stylesheet reads are written.** The
  app parses the stylesheet for `var(--gmnb-…)` names once and writes those
  (and the toolkit's own variables), in one `:root` block of the main
  provider.
- **A drag rewrites only what changes.** While the Background opacity
  slider, an option's slider or the colour chooser is dragged, the
  properties that differ from the main provider's go to the live provider
  (at most 64; more reload main). When the drag rests and the setting is
  saved (0.4 s after its last step), the live provider is folded back
  into main. Theming installs no timer of its own for this.

The properties' names are the app's own business (a theme never sees
them). The structure stylesheet holds no literal colour, size or font,
and every value in it or in the generated style comes from the checked
`Style`. A control's rules there are written by
`tools/theming/control_css.py`.

**Covered is not disabled.** While the sidebar, a sheet or a dialog
covers the page, what is under it is made insensitive so that assistive
technology can't operate it (`inert.rs`, the `.wc-inert` class). It must
keep its look. The structure stylesheet therefore applies the `disabled`
properties only to what the app itself turned off: under `.wc-inert`,
anything not marked `.wc-off` reads its resting properties, at full
opacity and with no filter. (0.2's rules at the end of `style.css`.)

Numbers are written with units by the app (`px`, `ms`, `deg`, `em`; a
zero length too, so that the stylesheet can use it in `calc()`).
Colours are written as `rgb()`/`rgba()`. Font family names are written as
quoted strings with `"` and `\` escaped, after refusing control
characters; OpenType tags, ease names and gradient keywords are matched
against what the API knows and written from that. Nothing a theme returns
is ever copied into the stylesheet as text.

### What the toolkit was found to support

Probed on GTK 4.22.4 / Pango 1.57.1 (the floor is GTK 4.18; custom
properties arrived in 4.16) with a throwaway gtk4-rs program, headless:
for each case one widget styled literally and one styled through `var()`
were rendered off screen and compared pixel by pixel and by size, with a
third, unstyled one to prove the case could tell. 40 of 41 cases were
identical; the 41st is the `none` row below.

| Checked | Result |
|---|---|
| `var()` as the whole value of `background-color`, `color`, `background-image` (linear, radial, two layers in one variable), `background`, `box-shadow` (a three-shadow list), `border-radius` (one and four values), `min-width`, `min-height`, `padding`, `margin`, `font-family` (one name and a list), `font-size`, `font-weight` (700 and 350), `letter-spacing` (px and em), `font-feature-settings`, `font-variation-settings`, `line-height`, `text-shadow` (a list), `outline`, `outline-offset`, `filter`, `transform`, `opacity`, `transition` | identical to the literal |
| `var()` inside a value: gradient stops and angle, `var(), var()` as two background layers, shadow width and colour, `outline` width and colour, `border` width and colour, `brightness(var())`, `scale(var())`, a transition's duration and its `cubic-bezier`, `alpha(var(), 0.5)` | identical to the literal |
| `var(--unset, fallback)`, a variable holding `var()`, a variable overridden on a parent | work |
| Replacing the provider that defines a variable | restyles at once |
| Variables defined on `:root`, read in a second toplevel window | resolve (computed colour checked) |
| `none` as one layer of a multi-layer `background-image` (`none, linear-gradient(…)`), literal or through `var()` | **The gradient beside it was not drawn either**: the widget looked as it does with no `background-image`, with no parse error. The app never emits `none` inside a list: absent layers are left out. |
| Inter's optical size | Pango sets `opsz` by itself, and the widths are consistent with it using the size in **points**: at 52 px a label measured as with `opsz` 32, at 12 and 15 px as with 14, at 21 px (15.75 pt) within a pixel of 14 and far from 32. An explicit `font-variation-settings` wins. DGMNB uses the size in pixels, so the app always writes `"opsz"` explicitly for Inter. |
| The bundled Inter's family name | It is **"Inter Variable"**, not "Inter". `font-family: "Inter"` silently fell back to another face. The base family is `"Inter Variable"`. |

Nothing needed designing around. Transitions were measured mid-flight
(a 4 s change sampled at 0.4 s and 1.5 s matched the literal).

Probed since, on GTK 4.18.6 / libadwaita 1.7.5 and 4.22.4 / 1.9.3 (the
results are the same on both):

- `text-shadow` set on the window reaches every label by inheritance
  (in buttons, toggles, list rows and `entry > text`).
- The converter's marker works as a sized background layer through
  `var()`: `background: var(--g) left bottom / 36px 3px no-repeat`.
- `AdwToggleGroup` is `toggle-group > toggle`, with `separator` nodes
  between the toggles. With the separators' size and margin zeroed, the
  gap between two toggles is **twice** `border-spacing`, so the
  stylesheet writes `calc(var(--gmnb-segmented-gap) / 2)`. Equal widths
  across the full width need `homogeneous`, `hexpand` and `halign: fill`
  on the group (structure, set in Rust from `panel.tabs.stretch`).
- Node names: `scale > trough > highlight | slider`; `switch > slider`;
  `toastoverlay > toast`; `scrollbar > range > trough > slider`;
  `popover > contents`; `dropdown > button.toggle` and
  `dropdown > popover.menu`; `spinbutton > text, button.up, button.down`;
  `windowcontrols > button.minimize|maximize|close`;
  `bottom-sheet > sheet.background`; `entry > text`. A tooltip can't be
  made without a display server that shows it: its names are unverified.

Read from libadwaita 1.9.3's own stylesheet while the rules were written
(not probed on 1.7): it fades a disabled toggle's label itself
(`toggle > label:disabled { filter }`), which the structure stylesheet
undoes for the families the app draws; and `calc(var(--x) / 2)` needs
the variable to carry a unit even when it is zero.

### Text in its place

DGMNB draws its own text, and GTK lays out and draws GMNB's: the base
look meets the [text rule](#how-it-is-measured) only where GMNB puts each
glyph where DGMNB does. What was found, measured against DGMNB's pictures
on the private compositor (GTK 4.22, Pango 1.57), and what the app does
about it:

| What | DGMNB | GTK, by default | GMNB |
|---|---|---|---|
| Outlines | Unhinted, grey levels (`text.rs`) | At scale 1 GTK's automatic font rendering hints them: every glyph moved and stretched to the pixel grid, by up to a pixel | `window.rs` `match_text_rendering`: manual rendering, no hinting (outlines or metrics), grey levels, for the whole app |
| Across | A line at `cx − width / 2`, to a quarter of a pixel | A label rounds its text's place down to a whole pixel | `window.rs` `exact_text`: a centred or end-aligned label is laid out across its whole width (it may be ellipsized), and Pango places the line there to the fraction |
| Down | The baseline at the place's middle plus half the capitals' height, rounded to a pixel (`ui.rs` `draw_line`) | A label centres its line box in its place, rounding down; the baseline is half the leading below the font's ascent. Inter's ascent less its descent is its cap height, so only the rounding differs | Each font's `--P-line-height` and `--P-text-nudge` (`css.rs` `text_line`): a line box and a pixel of padding above it that land the baseline on DGMNB's in any place an even number of pixels high |
| Scripts | 62% of the label's size, raised 0.36 of it or lowered 0.16, at their own optical size | Pango's `<sup>` uses the font's own superscript size and shift (60%, 0.35), and an absolute line height is given to every run, so a raised run makes the line taller | A key's label is not the toolkit's: `widgets/fit.rs` reads the keys' own `<sup>` / `<sub>` (`parse_markup`) and sets each script in the label's font at 62%, risen or dropped by those parts of the size, its baseline rounded with its shift as DGMNB's (`line_layout`, `draw_line`) |
| Cells | Rows of equal cells (`Rect::columns`, `Rect::cell`) fall on fractions of a pixel | Whole pixels, the spare ones to the first cells: a label up to a pixel off, a key up to two at 300×400 | `width_bin.rs` `Cells` (the memory row), `Strip` (the programmer bar) and `bitflip.rs`'s grid place children to the fraction with an allocation transform, and the bits' baselines on DGMNB's pixel. The keypads (`widgets/keypad.rs`) lay their keys out themselves: DGMNB's cells, each edge rounded to the nearest pixel. A key's label is set about the middle of DGMNB's cell (`fit.rs` `set_cell`, `cell_middle`): the keypads and the graph pad |
| Overlap | A narrow window's rows overlap | Children ask for their width, and the window grows | `Strip` asks for none of its own, and takes its slots apart as DGMNB's `take_left` and `take_right` do: each from what is left of the row, the last ones down to none |
| Buttons | A button's label shrunk to its width less its padding, centred (`button`); a unit button as wide as the unit and `UNIT_ARROW` more, "Update rates" the gap after the time's text, both to the fraction | A label at its size, on whole pixels | The labels of the converter's, the date page's, the graph's and the calculator's text buttons are `FitLabel`s; `FitLabel::set_shift` moves a line to where DGMNB's fractional geometry puts it |
| Fields | A field's text at its box's start less its padding, its capitals centred, cut at the box, the caret kept in view (`text_field`) | The toolkit's text field | At rest the text is drawn by a `FitLabel` over the field (`fit.rs` `rest_line`), and the field's own text is hidden (`.wc-rest`); with the focus the field shows its own, and it is always the field that is edited, shows the caret, selection and input method, and is read by assistive technology. A theme that hands the `field` family back to the toolkit keeps the toolkit's text |

The text the app places itself needs none of the toolkit's part of this:
a key's label, a page's line (`widgets/fit.rs` `FitLabel`) and the
readouts (`widgets/display.rs`) are laid out unhinted in grey levels
(`fit.rs` `set_unhinted`, whatever the settings say), shrunk to their
width, their capitals centred with each run's baseline on a whole pixel
and each glyph at the nearest quarter pixel (`fit.rs` `draw_line`), the
quarters and whole pixels being the window's wherever the label is
placed. A key's label and icon ask for no least height, so keys shrink
below what they show, as DGMNB's do; so does the bit grid.

### Toolkit-drawn families: `native`

Some widgets are the toolkit's own: switches, sliders, scrollbars, menus.
The base look restyles them all. A theme can hand families back:

```lua
native = { "switch", "slider", "scrollbar" }
```

Families: `segmented`, `field`, `slider`, `switch`, `scrollbar`, `menu`,
`toast`, `tooltip`, `button`, `card`, `focus`. A native family is drawn by
libadwaita in the [`stock`](#stock-colours) colours; of its style table
only `font` and `radius` still apply. In the stylesheet each family's rules
sit under a window class (`window.gmnb.own-switch switch { … }`) that the
app sets for every family *not* listed.

Aurora uses this to keep the toolkit's switches, sliders and menus
exactly as 0.2 had them. (Not its focus ring any more: libadwaita's is
the accent at half opacity, about 2:1 on the page and on a key, and a
`native` ring is held to [rule 5](#readability-rules) as libadwaita draws
it.)

### See-through windows

The Background opacity setting (0.1 to 1) is the app's, not the theme's.

- The window is always a transparent surface. The backdrop widget paints
  `window.background` and then the `background` layer, both inside one
  group at `ctx.opacity`. Colours keep their balance; the compositor may
  blur what shows through.
- Everything else is drawn at full strength. A theme decides what to
  put under its text: `style(ctx)` sees `ctx.opacity`.
- **A background that thins itself.** Faded as a whole, a background is
  never more there than the window is opaque: at 0.15 whatever it
  paints is all but gone. A `background` layer whose table says
  `fade = false` is left out of that group. `window.background` is
  faded as always, and the layer is drawn over it at full strength;
  what it does with `frame.ctx.opacity` is its own business. (A layer is
  drawn again whenever `ctx` changes: once a frame while the slider is
  dragged.) Aurora's sky is its `window.background`, which the app
  thins to clear air; its layer keeps the aurora's lights, a little
  clearer than they are in the night sky, hanging over the desktop, and
  lets its stars and its grain go with the sky. The key is the
  background layer's alone: the other layers are never faded, and it is
  noted on them. The [readability rules](#readability-rules) hold over
  whatever such a layer shows over a desktop: Aurora's tests take it
  from the layer itself. Sprinkles' wall thins with the window and its
  shop window comes in where it was (a frame and a shine on the glass),
  under an awning that stays: from 0.95 down the header *is* the awning,
  in style, so what its title lies on is the style's. Swirls' paint
  clears its dark band first, and is ribbons of its two bright bands over
  the desktop under the same glass keys; its tests take the paint from
  the ramp its layer draws through, each entry with its own opacity.

The base look's treatment, all of it switched off at opacity 1:

1. **A scrim under text.** `base.text_scrim` is `bg` at the smallest alpha
   `k` for which, over a black desktop and over a white one, with
   `B = over(alpha(bg, k), over(alpha(bg, opacity), desktop))`:
   `contrast(fg, B) ≥ 4.5`, `contrast(over(fg_dim, B), B) ≥ 3` and
   `contrast(accent_text, B) ≥ 3`. It goes under the header, the readout
   and every `plain` container. `k` is found by bisection in `appcore`
   (`palette::text_scrim`) and has a unit test. `ctx.base.text_scrim` is
   worked out from the base colours; the three elements' own base values
   from the theme's colours, so a theme that only changes `colors` gets a
   scrim of its own background.
2. **A halo.** `window.text_halo` is `{ blur 3, bg at 0.9 }, { blur 10, bg
   at 0.75 }`, and the readout gets the same as `display.halo`.
3. Keys, fields, panels, cards and popovers are opaque surfaces in the
   base look and stay so.

## Reference: tokens

### Colors

`colors` — base values from `appcore`'s `palette::Palette::new(ctx.dark, ctx.accent)`.

| Token | Dark | Light | Used for |
|---|---|---|---|
| `bg` | `#1d1d20` | `#f6f6f8` | The window, the sidebar |
| `surface` | `#2c2c31` | `#ffffff` | Number keys, fields, cards, popovers, the graph |
| `surface2` | `#25252a` | `#ececf0` | Function keys, the history panel, filled buttons, chips |
| `border` | `#3a3a40` | `#dcdce2` | Field and card borders, slider tracks, dividers |
| `fg` | `#f2f2f5` | `#1b1b1f` | Text |
| `fg_dim` | `fg` at 0.66 | | Secondary text |
| `fg_faint` | `fg` at 0.40 | | Disabled text, scrollbars |
| `accent` | the desktop accent (or `#3b6fd8`) at saturation ≤ 0.85 and lightness 0.62 dark / 0.48 light, then moved to 4.5:1 against `on_accent` | | The equals key, whatever is switched on |
| `on_accent` | white or `#101014`, whichever contrasts more with `accent` | | Text on `accent` |
| `accent_text` | the desktop accent moved to 4.5:1 against `bg` and `surface` | | Operators, the chosen page, the focus ring |
| `hover` | `fg` at 0.07 | `fg` at 0.05 | Laid over a control under the pointer |
| `press` | `fg` at 0.13 | `fg` at 0.10 | Laid over a control being pressed |
| `danger` | `#ff7b72` | `#c62828` | Errors |
| `series` | six: the accent's hue + 0°, 180°, 60°, 240°, 120°, 300° at saturation 0.70, lightness 0.66 dark / 0.45 light, each moved to 3:1 against `bg` and `surface` | | Graph colours |

`gmnb.base(dark, accent)` returns this table for any pair, so a theme can
re-derive it from its own accent (as [the example above](#a-theme-in-twenty-lines) does).

### Text, fonts

| Token | Base |
|---|---|
| `fonts.ui` | `"Inter Variable"`, then `"Noto Sans Math"`, `"Noto Sans Arabic"`, `"Noto Sans Armenian"`, `"Noto Sans Bengali"`, `"Noto Sans Khmer"`: the files DGMNB bundles, bundled here too. After them, the system's fonts. |
| `fonts.display` | `fonts.ui` |
| `text.body` | 15 px, weight 400 |
| `text.strong` | 15 / 600 |
| `text.small` | 13 / 400 |
| `text.caption` | 12 / 500 |
| `text.key` | 19 / 400 |
| `text.title` | 17 / 600 |

A text style without `family` takes `fonts.ui`.

### Shape, icons

| Token | Base | Used for |
|---|---|---|
| `shape.radius` | 8 | Keys, buttons, date and unit buttons |
| `shape.row_radius` | 6 | List rows, text fields |
| `shape.card_radius` | 12 | Panels, cards, popovers, the graph |
| `icons.stroke` | 1.8 | Line width of every icon on its 24-unit grid |
| `icons.taper` | 0 | At `size` px a line is `stroke × (taper + (1 − taper) × size / 24)` px wide: 1 keeps it the same at every size. 0, the base look's, is DGMNB's line, `stroke × (size / 24)²` (DGMNB's stroke is in the path's units, which the icon's scale scales again: 1.0 px at 18 px). |

An element's own `icon.stroke` (in pixels) overrides the formula.

### Motion

Milliseconds; 0 means "no animation". The base look has every one at 0.

| Token | What it times | Aurora |
|---|---|---|
| `motion.page` | Cross-fade between pages | 220 |
| `motion.mode` | Cross-fade between calculator modes | 180 |
| `motion.panel` | History ⇄ Memory slide | 220 |
| `motion.flip` | Programmer: keypad ⇄ bits | 240 |
| `motion.graph_view` | Graphing, narrow: equations ⇄ graph | 200 |
| `motion.graph_side` | The graph side panel's own pages | 240 |
| `motion.date` | Date: difference ⇄ add/subtract | 260 |
| `motion.history_row` | A new history row sliding in | 280 |
| `motion.equation_row` | An equation row appearing or leaving | 260 |
| `motion.icon_draw` | A navigation icon drawing itself when picked | 700 |
| `motion.graph_draw` | A curve drawing itself | 900 |
| `motion.graph_zoom` | The graph gliding to a new zoom | 240 |

Which kind of movement each one is (fade, slide) is structure and fixed.
All are skipped under reduced motion.

### Layout

The space around and between a page's parts. Feeds the pages' boxes
through `Style` (not CSS).

| Token | Base (DGMNB) | Aurora |
|---|---|---|
| `layout.calculator.margin` | `{ 4, 8, 4, 8 }` | `{ 0, 8, 8, 8 }` |
| `layout.calculator.gap` | 0 | 6 |
| `layout.calculator.panel_width` | 292 | 300 |
| `layout.calculator.panel_margin` | `{ 4, 0, 4, 8 }` (the 8 px between the page and the panel is on the panel's left) | `{ 0, 10, 10, 0 }` |
| `layout.converter.margin` | `{ 6, 12, 6, 12 }` | `{ 0, 12, 10, 12 }` |
| `layout.converter.gap`, `.gap_wide` | 6, 20 (between the values and the keypad: under them, and beside them in a wide window) | 10, 24 |
| `layout.converter.keypad_width` | 300 | 300 |
| `layout.date.margin` | `{ 8, 16, 8, 16 }` | `{ 4, 16, 16, 16 }` |
| `layout.date.gap` | 8 | 14 |
| `layout.date.max_width` | 480 | 480 |
| `layout.graph.margin` | `{ 4, 8, 4, 8 }` | `{ 0, 10, 10, 10 }` |
| `layout.graph.gap` | 8 | 10 |
| `layout.graph.side_width` | 340 | 340 |
| `layout.graph.pad_height` | 214 | natural |

Not themable, and DGMNB's in every theme: a new window is 360×600, the
smallest is 300×400, the calculator goes wide at 620 px, the converter at
700 and graphing at 760.

**DGMNB's source is the authority for the base column** here and in the
tables below. They were transcribed from `apps/dgmnb/src/{ui,app,calc,conv,date,graph}.rs`
at 0.2.0; where a number here and DGMNB's code disagree, the code is right
and this table is corrected. Every place DGMNB names a text style was
matched to an element below; the one known mismatch is the rows of menus
(`row`): DGMNB sets the bit-shift list in its small style and the unit
list in its body style, and here both are `row`. If the comparison shows
it, `row` gains a variant.

### Stock colours

`stock` colours the toolkit's own widgets: the Settings and About dialogs,
menus, dropdown lists, the colour chooser, and every `native` family. They
become libadwaita's named colours and variables (`window_bg_color`,
`--window-bg-color`, and so on), which the app defines in full so the
desktop's own `gtk.css` cannot repaint it.

| Token | Base | libadwaita name |
|---|---|---|
| `stock.window` | `bg` | `window_bg_color`, `overview_bg_color` |
| `stock.view` | `surface` | `view_bg_color` |
| `stock.headerbar` | `bg` | `headerbar_bg_color`, `headerbar_backdrop_color` |
| `stock.sidebar` | `bg` | `sidebar_bg_color`, `secondary_sidebar_bg_color` |
| `stock.dialog` | `bg` | `dialog_bg_color` |
| `stock.popover` | `surface` | `popover_bg_color` |
| `stock.card` | `surface` | `card_bg_color` |
| `stock.fg` | `fg` | every `*_fg_color` above |
| `stock.accent` | `accent_text` | `accent_color` |
| `stock.accent_bg` | `accent` | `accent_bg_color` |
| `stock.accent_fg` | `on_accent` | `accent_fg_color` |
| `stock.destructive` | `danger` | `destructive_color`, `destructive_bg_color` |
| `stock.error` | `danger` | `error_color`, `error_bg_color` |

### Native families

| Token | Base | Meaning |
|---|---|---|
| `native` | `{}` | The widget families left to the toolkit; see [Toolkit-drawn families](#toolkit-drawn-families-native). |

## Reference: every element

How to read the tables: **Path** is where the value sits in what `style`
returns. **Kind** is its type. **Base** is the value without a theme, in
terms of tokens. **Feeds** is the selector in the structure stylesheet
that reads its custom properties, or the Rust code that takes it from
`Style`. Classes marked *(new)* don't exist in 0.2.

For a Control, the base states are these unless the row says otherwise:
`hover = { overlay = colors.hover }`, `active = { overlay = colors.press }`,
`disabled = { color = colors.fg_faint }`, no transition.

### Window

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `window.background` | Fill | `bg` | The backdrop widget (Rust), under everything, faded to `ctx.opacity` |
| `window.color` | Color | `fg` | `window.gmnb` |
| `window.font` | Font | `text.body` | `window.gmnb`; inherited by every widget, popover and dialog in it |
| `window.text_halo` | Shadow list | none; see-through: `{ blur 3, bg 0.9 }, { blur 10, bg 0.75 }` | `text-shadow` on `window.gmnb` (inherited) |
| `window.raised_text_halo` | Shadow list | none | `text-shadow` on keys, buttons, fields, the panel, cards, popovers, the sidebar. Disabled controls get `none` always. |
| `plain.background` | Fill | `base.text_scrim` | `.wc-plain` *(new)*: the memory row, radix list, bit grid, programmer and scientific toolbars, the date column, the converter's value column, the graph's equation column and its analysis. (An empty list's text in the History panel lies on the panel, a surface of the style's.) |
| `plain.radius` | Radius | `shape.card_radius` | same |
| `scrim` | Color | black at 0.30 | The shade over what the sidebar or a sheet covers |
| `focus` | FocusRing | `{ width 2, color accent_text, offset 2 }` (the ring touches the control: DGMNB's `rounded_border` strokes inside the rect it is given); below opacity 1 its colour is moved 9:1 from `bg`, and it has `edge { 1, bg }` (around a key the desktop shows through the backdrop) | `window.gmnb.own-focus :focus:focus-visible { outline }` (the focus itself: `:focus-visible` alone holds for the widgets holding it too) for every control but text fields and sliders, which draw their own; `outline-offset` is `offset − width`; the ring's corners follow the control's radius. `edge`: the keypad (Rust, `widgets/keypad.rs`) draws it around its key that holds the focus |

### Header and window buttons

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `header.background` | Fill | `base.text_scrim` | `window.gmnb headerbar` |
| `header.height` | number | 46 | `headerbar { min-height }` |
| `header.title` | Text | `text.title`, `fg` | `.wc-title` |
| `header.button` | Control | `icon_button`'s base (its row comes first) | `headerbar .wc-icon-button`: menu, History, Keep on top, the graph's toggles. History is a button, not a toggle, as DGMNB's |
| `header.controls` | Control | `icon_button`'s base | `button.wc-window-button` *(new)*: minimise, maximise/restore, close. The app's own buttons with DGMNB's glyphs (`appcore::icons::WINDOW_*`), in its order, named, not focused by a click; the toolkit's are switched off. In compact (keep on top) Close alone stays, and the way back takes the menu's place, as DGMNB's |
| `header.close` | Control | `header.controls` | `button.wc-window-button.wc-close` *(new)* |

### Navigation sidebar

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `nav.background` | Fill | `bg` | `.wc-sidebar` |
| `nav.shadow` | Shadow list | `{ inset, x −1, border }` (a line down the right edge) | `.wc-sidebar` |
| `nav.width` | number | 280 | The split view's sidebar width (Rust); it always leaves 40 px of the window uncovered |
| `nav.title` | Text | `text.title`, `fg` | `.wc-nav-title` *(new)*: the app's name beside the menu button |
| `nav.show_title` | boolean | `true` | Rust: the label's visibility |
| `nav.header` | Text | `text.caption`, `fg_dim`, margin `{ 0, 0, 0, 18 }` (its row is 28 high) | `.wc-nav-header` |
| `nav.uppercase_headers` | boolean | `false` | Rust: a Pango text transform on the label; what a screen reader hears doesn't change |
| `nav.row` | Control | radius `shape.row_radius`; min_height 40; margin `{ 0, 6, 2, 6 }` (the 2 px step is under each row); padding `{ 0, 8 }` (the icon in a 28 px box, the label 44 px in); `text.body`; `fg`; icon `{ size 18 }`; `checked = { background accent at 0.16, color accent_text }` | `list.wc-nav > row.wc-nav-row`; `checked` is `:selected` |
| `nav.settings` | Control | as `nav.row`, margin 6 (its foot is 52 high) | `button.wc-nav-settings` |

### Shared controls

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `icon_button` | Control | 36×36; radius 999; `fg`; icon `{ size 18 }`; `checked = { background accent at 0.18, color accent_text }` | `button.wc-icon-button` |
| `text_button` | Control | min_height 34; padding `{ 0, 6 }`; radius `shape.radius`; `text.caption`; `fg`; no background; `checked = { background accent, color on_accent }` | `button.wc-mem`: MC MR M+ M− MS M▾, DEG, F-E, the programmer bar |
| `text_button_gap` | number | 2 | The rows' spacing (Rust) |
| `flyout_button` | Control | as `text_button`, in `text.small`, min_height 30 (its row is 36 high, the button 3 px in from top and bottom), padding `{ 0, 6 }` | `menubutton.wc-flyout-button > button`: Trigonometry ▾ and Function ▾ (each one label, as DGMNB's; the programmer bar's Bitwise ▾ and Bit shift ▾ are `text_button`s, `menubutton.wc-text-flyout > button`) |
| `button` | Control | background `surface2`; radius `shape.radius`; `text.body`; `fg`; min_height 34; padding `{ 0, 6 }`; `checked = { background accent, color on_accent }` | `window.gmnb.own-button dialog button:not(.flat):not(.circular):not(.image-button)`: the dialogs' buttons (the toolkit has no attribute selectors, so "every button without a `wc-` class" can't be said; the app's own buttons have rules of their own) |
| `segmented.gap` | number | 4 | `toggle-group { border-spacing }` (half of it: see [what the toolkit was found to support](#what-the-toolkit-was-found-to-support)) |
| `segmented.item` | Control | min_height 36; padding `{ 0, 6 }`; radius `shape.radius`; `text.small`; `fg`; background `surface2`; `checked = { background accent, color on_accent }` | `window.gmnb.own-segmented toggle-group > toggle` |
| `field` | Field | background `surface`; `fg`; border `{ 1, border }`; radius `shape.row_radius`; `text.body`; padding `{ 0, 10 }`; min_height 38; placeholder `fg_faint`; selection `accent` at 0.30; caret `fg`; `focus = { border { 2, accent_text } }`; `error = { border { 2, danger } }` | `window.gmnb.own-field entry, spinbutton` |
| `row` | Control | radius `shape.row_radius`; min_height 36; `checked = { background accent at 0.16 }` | Rows of menus, dropdown lists, the bit-shift list |
| `section` | Text | `text.caption`, `fg_dim` | `.wc-section` |
| `slider.track` | `{ height, color, radius }` | 4, `border`, 2 | `window.gmnb.own-slider scale trough` |
| `slider.fill` | Color | `accent` | `scale highlight` |
| `slider.knob` | `{ size, color, halo, halo_size }` | 16, `accent`, `accent` at 0.16, 24 | `scale slider`; the halo shows on hover and press |
| `switch` | `{ width, height, knob_size, off, on, knob, knob_on }` | 46, 24, 18, `border`, `accent`, `surface`, `on_accent` | `window.gmnb.own-switch switch`; the knob is `switch > slider`, centred |
| `scrollbar` | `{ width, color, radius, inset }` | 3, `fg_faint`, 1.5, 5 (from the view's edge to the thumb's outer side) | `.wc-scroll-thumb`: the thumb the app draws over its lists (the sidebar, History, Memory, the converter's values, Graphing's equations and analysis) whenever one is longer than its view, as DGMNB's, and as long; `window.gmnb.own-scrollbar scrollbar slider` for the toolkit's own. A theme that hands `scrollbar` back gets the toolkit's on the lists too |
| `card` | Surface | background `surface`; radius `shape.card_radius`; no border | `window.gmnb.own-card .card, list.boxed-list` (Settings) |
| `card.heading` | Text | `text.strong`, `fg` | The headings over Settings' and About's cards (`preferencesgroup box.header label.heading`) |
| `popover` | Surface | background `surface`; border `{ 1, border }`; radius `shape.card_radius`; padding 6; shadow: three rings, `{ spread 6, black 0.05 }, { spread 3, black 0.07 }, { spread 1, black 0.10 }` | `popover.wc-flyout > contents`, the calendar popover |
| `menu` | Surface | as `popover`, radius 10 | `window.gmnb.own-menu popover:not(.wc-flyout) > contents` |
| `toast` | `{ background, color, radius, font, min_height, padding }` | `fg` at 0.92; `bg`; 19; `text.body`; 38; `{ 0, 16 }` | `window.gmnb.own-toast toast` |
| `tooltip` | `{ background, color, radius, font, padding }` | `fg` at 0.92; `bg`; `shape.row_radius`; `text.small`; `{ 4, 8 }` | `window.gmnb.own-tooltip tooltip` |

### Display (the readout)

Drawn by `widgets/display.rs` from `Style`. The converter's two values and
the date page's results are readouts too (to a theme's
[`display` layer](#the-display-layer): `role` `"converter"` and `"date"`),
but lines of their own to the style: their fonts are `converter.value` and
`date.result`, their colour the window's, and none of the rows below
applies to them.

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `display.background` | Fill | `base.text_scrim` | `calcdisplay:not(.wc-line)` (CSS): the calculator's readout, not a page's line |
| `display.radius` | Radius | `shape.card_radius` | `calcdisplay:not(.wc-line)` (CSS): the calculator's readout, not a page's line |
| `display.padding_x` | number | 8 | Rust: space left and right of the text |
| `display.heights` | per mode: number or `"auto"` | standard 116, scientific 104, programmer 84, compact 86 | Rust: the widget's height. `"auto"`: 1.6 × the expression's size + 1.28 × the mode's size + 12 |
| `display.sizes` | per mode | 52, 52, 52, compact 40 | Rust: the value's largest size; it shrinks to fit the width |
| `display.min_size` | number | 16 | Rust: where shrinking stops |
| `display.value.font` | Font | `fonts.display`, weight 400, `{ "tnum" }` | Rust (size comes from `sizes`) |
| `display.value.color` | Paint | `fg` | Rust: a gradient is spread over the value's band |
| `display.value.error_color` | Paint | `fg` | Rust |
| `display.value.error_scale` | number | 1 | Rust: an error's size as a fraction of the mode's |
| `display.value.glow` | `{ color, blur }` or `false` | `false` | Rust: a blurred copy under the value; `blur` is a fraction of the font size |
| `display.expression` | Text | `text.small`, `fg_dim` | Rust; the line is 30 px high (24 compact) |
| `display.expression_min_size` | number | 9 | Rust: a long expression shrinks to this before it is cut at its start |
| `display.halo` | Shadow list | none; see-through: as `window.text_halo` | Rust: shadows around the whole readout |

A theme with a [`display` layer](#the-display-layer) paints the value
itself; `value.color`, `error_color` and `glow` are then what a failed
hook falls back to.

### Keys

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `keys.gap`, `keys.gap_compact` | number | 4, 3 | Rust: the keypad grid's spacing |
| `keys.radius` | Radius | `shape.radius` | `button.wc-key`; also given to draw hooks as `key.radius` |
| `keys.min_width`, `keys.min_height` | number | 0, 0 | `button.wc-key` |
| `keys.padding` | Spacing | 0 | `button.wc-key` |
| `keys.transition` | Transition | none | `button.wc-key` |
| `keys.icon` | Icon | `{ size 20 }` | Rust: the keys that carry an icon (backspace, …) |
| `keys.hover`, `keys.active`, `keys.disabled` | State | `{ overlay hover }`, `{ overlay press }`, `{ fade 0.35 }` | Every key class that doesn't give its own |
| `keys.number` | Control | background `surface`; `fg`; `text.key` at size + 2, weight 500 | `button.wc-key.wc-num` |
| `keys.func` | Control | background `surface2`; `fg`; `text.key` | `button.wc-key.wc-fn` |
| `keys.operator` | Control | background `surface2`; `accent_text`; `text.key` | `button.wc-key.wc-op` |
| `keys.equals` | Control | background `accent`; `on_accent`; `text.key` | `button.wc-key.wc-eq` |
| `keys.on` | State | `{ background accent, color on_accent }` | `button.wc-key.wc-on`, `button.wc-fn:checked` (2nd, hyp, … switched on) |
| `keys.pad` | `{ gap, radius, min_height, font }` | 3, `shape.radius`, 0, `keys.func`'s font | `.wc-graph-pad button.wc-pad-key` and its grid (the graphing page's keypad). The keys are set in their classes' own fonts (`keys.number`, `keys.func`) unless `font` differs from `keys.func`'s: then it is every pad key's (Rust). Its labels ask for no least height, as a keypad's do. |
| `keys.short.below` | number | 0 (never) | Rust: a keypad whose keys would be less than this high, laid out `keys.gap` apart (`keys.gap_compact` in the compact window), is *short*: it carries `wc-short` and takes the rest of `keys.short` |
| `keys.short.gap` | number | `keys.gap` | Rust: a short keypad's spacing |
| `keys.short.icon` | Icon | `keys.icon` | Rust: a short keypad's keys that carry an icon |
| `keys.short.number`, `.func`, `.operator`, `.equals` | Control | that class as finished | `keypad.wc-short button.wc-key.wc-num` (and `.wc-fn`, `.wc-op`, `.wc-eq`): every rule of the class again, which outranks the class's own for each state; the labels' fonts (Rust) |
| `keys.short.on` | State | `keys.on` | A short keypad's toggle that is on |

The key classes are Controls but have no `checked`; a toggle that is on
uses `keys.on`. A class takes `keys`' shared values (`radius`, `padding`,
`min_width`, `min_height`, `transition`, `icon`, and the three states)
as the theme set them, unless it gives its own. Superscripts and subscripts in key labels are 62% of the
label's size, raised 0.36 of it or lowered 0.16 (fixed, DGMNB's: [Text in its place](#text-in-its-place)).

A key's label is drawn by the app (`widgets/keypad.rs`, a `FitLabel`), in
its class's `font`: its colour is the stylesheet's (the class's `color`
in each state), its weight a state's `font_weight` when the key is in
that state (`checked`, then `hover` or `active`, then `disabled`, as the
stylesheet orders them), and it is shrunk to its key's width less 8 with
no least size. Its halo is `window.raised_text_halo`, and none while the
key is disabled.

Keys shrink with the window, down to its least, 300×400, where
Scientific's and Programmer's are some fifteen pixels high in the base
look. A look made for keys of forty pixels (a lip,
a glint, a large sign) has no room there: `keys.short` is a second look
for a keypad whose keys are less than `keys.short.below` high. Only the
height the keypad is given decides it, never anything the short keys
ask for, so it can't flicker at the threshold; it is decided once the
toolkit has laid the window out, and a frame later the keys are drawn
the other way. A key class's own `min_height` keeps a window from
shrinking at all, and the least window is the contract's on every page:
a theme lets its keys shrink and makes them short instead.

### History and memory panel

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `panel` | Surface | background `surface2`; radius `shape.card_radius`; padding 8; no border, no shadow | `.wc-side-panel` (the wide window's panel) |
| `panel.sheet` | Surface | background `surface`; border `{ 1, border }`; radius 14; the popover's three-ring shadow; padding 8; margin `{ 0, 6, 6, 6 }` | `bottom-sheet > sheet` (background, shadow, radius, margin) and `.wc-sheet` (padding), the narrow window's: a BottomSheet, its height DGMNB's share of the page (62%, at least 260) |
| `panel.tabs.stretch` | boolean | `true` | Rust: the switch fills the panel's width in equal halves, or hugs its labels at the start |
| `panel.tabs.gap` | number | 0 | the switch's spacing |
| `panel.tabs.item` | Control | min_height 34; padding `{ 0, 6 }`; radius `shape.radius`; `text.strong`; `fg`; no background; `checked = { background accent, color on_accent }` | `.wc-toggle-group.wc-panel-tabs > toggle` *(new class)* |
| `panel.title` | Text | none: accepted and not drawn | Programmer has memory only, and shows its Memory tab alone across the panel, lit, as DGMNB's (0.2 showed a title) |
| `panel.row` | Control | radius `shape.row_radius`; min_height 64 (a memory row, with its three buttons under the value: 70); padding `{ 6, 10 }`; margin `{ 0, 0, 2, 0 }` (the step to the next) | `list.wc-panel-list > row.wc-panel-row`: its hover and press cover the whole row; the padding is its content's (`.wc-row-content`); the expression line is 20 high and leaves 24 px for the ×, the result (or the value) 32. No slide-in unless `motion.history_row` |
| `panel.expression` | Text | `text.small`, `fg_dim` | `.wc-hist-expr` |
| `panel.result` | Text | 22 / 500 with `{ "tnum" }`, `fg` | `.wc-hist-result` (history results and memory values) |
| `panel.delete` | Control | 22×22; radius 999; `fg`; the icon as DGMNB draws it | `button.wc-hist-delete` *(new)*: the × on each history row |
| `panel.delete_reveal` | `"always"` or `"hover"` | `"always"` | `"hover"`: opacity 0 until the row is hovered or holds the keyboard focus. The button is always there for the keyboard and for screen readers. |
| `panel.memory_button` | Control | background `surface2`; radius `shape.radius`; `text.caption`; `fg`; min_height 26; padding `{ 0, 6 }` | `button.wc-mem-small`: MC, M+, M− on a memory row, 126 px between them, 4 apart |
| `panel.memory_reveal` | `"always"` or `"hover"` | `"always"` | `.wc-mem-actions`, as `delete_reveal` |
| `panel.empty` | Text | `text.small`, `fg_dim` | `.wc-empty` |

The trash button under the list is an `icon_button`.

### Programmer

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `programmer.radix.row` | Control | radius `shape.row_radius`; min_height 26; padding `{ 0, 8 }`; `checked = { background accent at 0.16 over bg }` (DGMNB's `row`, selected; opaque, so it reads on a see-through window) | `button.wc-radix-row`; `checked` is `.wc-radix-active`; the name 52 px wide |
| `programmer.radix.name` | Text | `text.strong`, `fg_dim` | `.wc-radix-name` |
| `programmer.radix.name_checked` | Color | `accent_text` | `.wc-radix-active .wc-radix-name` |
| `programmer.radix.value` | Text | 14 / 400 with `{ "tnum" }`, `fg` | `.wc-radix-value` |
| `programmer.bit` | Control | radius `shape.radius`; 16 / 400; `fg`; no background; `checked = { background accent, color on_accent, font { weight 700 } }` | `button.wc-bit`, on DGMNB's fractional grid (`bitflip.rs`) |
| `programmer.bit_index` | Text | `text.caption`, `fg_faint`; see-through: deepened toward `fg_dim` until it reaches 2.45 on the scrim over both desktops (`appcore::palette::faint_text`) | `.wc-bit-index` |

Word size, Bitwise and Bit shift are `text_button`s (DGMNB sets them in
its caption style, unlike Scientific's two flyout buttons); their flyouts
are `popover`s.

### Date page

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `date.label` | Text | `text.caption`, `fg_dim` | `.wc-section` on this page |
| `date.field` | Control | min_height 38; radius `shape.radius`; background `surface2`; `text.body`; `fg`; padding `{ 0, 6 }`; `checked = { background accent, color on_accent }` (while its calendar is open) | `menubutton.wc-field > button.toggle`: "Tuesday, November 10, 2026  ▾", centred, at most 320 wide |
| `date.result_panel` | Surface | nothing: no background, no padding | `.wc-result-panel`: the box around a result's caption, the result and the line under it |
| `date.result`, `date.add_result` | `{ size, font }` | 28, weight 400; the same | The page's results (Rust, `widgets/display.rs`: a readout, `role = "date"`), shrunk to fit down to 14; out of range, the error colour |
| `date.subresult` | Text | `text.body`, `fg_dim` | `.wc-subresult` |
| `date.calendar.title` | Text | `text.strong`, `fg` | `.wc-days-title` |
| `date.calendar.weekday` | Text | `text.caption`, `fg_dim` | `.wc-days-weekday` |
| `date.calendar.day` | Control | fills its cell (about 38×37: seven columns and six rows in the 300×316 popover, padding 10, gap 2); radius 999; `text.small` (DGMNB's days are not tabular); `fg`; `checked = { background accent, color on_accent }` | `button.wc-day`; `checked` is `.wc-day-selected` |
| `date.calendar.other_month` | Color | `fg_faint` | `.wc-day-other` |
| `date.calendar.today` | Border or `false` | `{ 1, accent_text }` | `.wc-day-today` |
| `date.calendar.focus` | FocusRing | `focus` | `button.wc-day:focus` (shown on plain focus: a popover isn't told of keyboard use) |

The mode switch is a `segmented`; the day-count boxes are `field`s; the
calendar sits in a `popover`; its arrows are `icon_button`s.

### Converter

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `converter.field` | Control | no background; radius 0; padding 0; `checked` changes nothing | `.wc-conv-field`; `checked` is `.wc-active` (the field being typed into) |
| `converter.marker` | `{ color, width, height }` or `false` | `{ accent, 36, 3 }` | A bar at the bottom-left of the active field: a sized background layer on the field's inner box, `.wc-conv-field.wc-active > .wc-conv-marker` (so it never shares a layer list with the field's own background). `false` writes it as transparent and 0×0. |
| `converter.value` | `{ size, font, checked_weight, focus }` | 38; `fonts.display` weight 300 with `{ "tnum" }`; 500; `focus` | The two values (Rust, `widgets/display.rs`: readouts, `role = "converter"`), shrunk to fit down to 14; the currency's symbol before a value in `converter.symbol`, 8 px from it (the app's, whoever paints the value); the ring is `.wc-conv-field calcdisplay.wc-conv-value:focus-visible` |
| `converter.symbol` | Text | 26 / 400, `fg_dim` | `.wc-currency-symbol` |
| `converter.unit` | Control | min_height 34; radius `shape.radius`; background `surface2`; `text.body`; `fg`; `checked = { background accent, color on_accent }` | `dropdown.wc-unit-dropdown > button`; `checked` while its list is open. The list is a `menu`. |
| `converter.swap` | Control | `icon_button` | `button.wc-swap` |
| `converter.caption` | Text | `text.caption`, `fg_dim` | "About equal to" |
| `converter.whimsy` | State | changes nothing | `.wc-chip-whimsy` (the playful comparison): it is the chip's `checked` |
| `converter.chip` | Control | min_height 28; radius 14; background `surface2`; `text.small`; `fg`; padding `{ 0, 10 }`; nothing under the pointer or pressed; `checked` is `converter.whimsy` | `.wc-chip`; `checked` is `.wc-chip-whimsy` |
| `converter.whimsy_icon` | boolean | `false` | Rust: whether that chip shows its sparkle |
| `converter.ratio` | Text | `text.small`, `fg` | `.wc-ratio` |
| `converter.timestamp` | Text | `text.small`, `fg_dim` | `.wc-timestamp` |
| `converter.link` | Control | `text.small`; `fg`; no background; padding `{ 0, 6 }` (DGMNB's "Update rates" is a plain button) | `button.wc-link`, 116 × 26 |

### Graphing

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `graph.side` | Surface | nothing: no background, radius 0, padding 0 | `.wc-graph-side` |
| `graph.divider` | Color or `false` | `border` | A 1 px line above the keypad |
| `graph.row` | Surface | no background; radius 0; padding `{ 2, 0 }` (rows 40 high) | `.wc-eq-row` |
| `graph.row_error` | State | changes nothing | `.wc-eq-row.wc-has-error` |
| `graph.entry` | Field | `field` with min_height 36 | `entry.wc-eq-entry`; `error` while the equation doesn't parse |
| `graph.row_button` | Control | `icon_button` at 32×32 | The row's style, analyse and remove buttons |
| `graph.error` | Text | `text.caption`, `danger` | `.wc-eq-error` |
| `graph.swatch.size` | number | 18 | `button.wc-swatch` |
| `graph.swatch.hidden_opacity` | number | 0.3 | A hidden equation's swatch |
| `graph.swatch.glow` | `{ blur, spread }` or `false` | `false` | A shadow in the equation's own colour (the app writes each swatch's colour) |
| `graph.swatch.active_scale` | number | 1 | While pressed |
| `graph.swatch.transition` | Transition | none | |
| `graph.add` | Control | min_height 36; `text.body`; `fg`; no background; radius `shape.radius` | `button.wc-add-eq` |
| `graph.variables` | Text | `text.strong`, `fg` | "Variables" (`.wc-var-header`) |
| `graph.variable` | Text | 15 / 600, `fg` | `.wc-var-name`; its slider is a `slider`, its box a `field` |
| `graph.analysis.title` | Text | 18 / 600, `fg` | `.wc-analysis-title` |
| `graph.analysis.card` | Surface | no background; border `{ 1, border }`; radius 8; padding `{ 8, 12 }` | `.wc-kgf` |
| `graph.analysis.card_title` | Text | `text.caption`, `fg_dim` | `.wc-kgf-title` |
| `graph.analysis.value` | Text | `text.body`, `fg` | `.wc-kgf-value` |
| `graph.analysis.text` | Text | `text.small`, `fg` | `.wc-kgf-text` |
| `graph.analysis.note` | Text | `text.caption`, `fg_dim` | `.wc-kgf-note` |
| `graph.toolbar` | Surface | background `surface2` at 0.94; radius 10; padding `{ 4, 2, 6, 2 }`, and 2 px between its buttons | `.wc-graph-tools` |
| `graph.toolbar_button` | Control | `icon_button` at 34×34 | its buttons |

The canvas is drawn by `widgets/graph_view.rs` from `Style`, on screen and
again off screen for *Copy image*:

| Path | Kind | Base |
|---|---|---|
| `graph.canvas.background` | Paint | `surface` |
| `graph.canvas.radius` | number | `shape.card_radius` |
| `graph.canvas.border` | Border or `false` | `{ 1, border }` |
| `graph.canvas.grid_minor` | Color | `fg` at 0.05 dark / 0.06 light |
| `graph.canvas.grid_major` | Color | `fg` at 0.11 dark / 0.13 light |
| `graph.canvas.axis` | `{ color, width }` | `fg` at 0.55, 1.3 |
| `graph.canvas.label` | Text | 11 / 400, `fg_dim` |
| `graph.canvas.fill_opacity` | number | 0.18 (shaded inequalities) |
| `graph.canvas.glow` | `{ blur, width, opacity }` or `false` | `false`. Under each curve, a copy `width` × as thick, blurred by `blur` px, at `opacity`. |
| `graph.canvas.trace.hair_opacity` | number | 0.45: the cross-hair, in the curve's colour |
| `graph.canvas.trace.hair_dash` | `{ dash, gap }` or `false` | `false` (solid) |
| `graph.canvas.trace.halo` | `{ grow, color, opacity }` or `false` | `{ grow 1.5, color surface }`: a disc behind the dot. `color = "series"` uses the curve's colour at `opacity`. |
| `graph.canvas.trace.ring` | Border or `false` | `false` |
| `graph.canvas.trace.bubble` | `{ background, color, radius, font, border, shadow }` | `surface`; `fg`; 8; `text.small`; `{ 1, border }`; the popover's three rings |

Curve colours are `colors.series`; line width and dashes are the user's
graph settings. A proven hole in a curve is an open circle; its inside
shows the canvas.

---------------------------------------------------------------------------

## Drawing

A theme can draw in four places, its **layers**:

| Layer | Where | Coordinates | Notes |
|---|---|---|---|
| `background` | The whole window, under every widget, over `window.background` | The window | Faded to `ctx.opacity` together with `window.background`: draw it opaque. Or say `fade = false` and thin it yourself ([See-through windows](#see-through-windows)). |
| `keypad` | Each keypad, over its keys | That keypad | Called once per visible keypad. Can also move its keys (`place`). |
| `display` | Each readout | That readout | Paints the value **instead of** the app. |
| `overlay` | The whole window, over every widget of the page | The window | Never takes clicks. The header, the sidebar and a page's own sheets are under it. Settings and About are over it, so that no theme can cover the place it is switched off in; popovers and menus are separate surfaces and are not covered either. |

```lua
layers = {
	background = {
		draw = function(draw, frame) … return nil end,
		events = { press = 1.15, result = 1.15 },  -- which events, and how long each is kept
		pointer = false,                           -- true: frame.pointer, and a redraw as it moves
		fade = true,                               -- background only. false: not faded by the app on a see-through window
	},
}
```

### The frame

| Field | Meaning |
|---|---|
| `width`, `height` | The layer's size. |
| `time` | Seconds this layer has spent animating. It stands still whenever the layer isn't being animated. |
| `dt` | Seconds since this layer's previous frame, **if that frame asked for another** (returned a number) and the clock has run since; else 0. It doesn't matter what caused this frame: an event or a pointer move in the middle of an animation still gets the time since the last frame. Never more than 0.25. Add it up to run your own clock. |
| `motion` | `false`: the desktop asked for reduced motion. Draw a still picture. There are no events, `dt` is 0 and what you return is ignored. |
| `settled` | `true`: nothing has happened for 45 seconds and the app stopped the clock. Requests for frames wait until something happens. |
| `active` | The window is the one in use. |
| `pointer` | `{ x, y }` while the pointer is over the layer, for layers with `pointer = true`. |
| `events` | What happened recently, newest last. |
| `ctx` | The same `ctx` as `style` gets. |
| `keys` | Keypad layer: every cell of this keypad. |
| `value` | Display layer: the value to paint. |

A frame and everything in it is the app's: read-only, and refilled in
place for the next frame. Copy what you want to keep.

### Asking for the next frame

What `draw` returns decides when it runs again:

| Return | Meaning |
|---|---|
| `nil` | This picture stands until something changes. |
| `0` | Again on the next frame. |
| a number of seconds | Again after that long. Below 1/60 is treated as 0. |

The app owns the clock:

- A layer is redrawn when its size changes, when `ctx` changes, when an
  event it listens for arrives, when the pointer moves over it (if it
  asked for the pointer), when a key's state changes (keypad), when the
  value changes (display), and when its own request comes due. Nothing
  else. One layer animating does not redraw the others.
- A layer has at most one request pending. Each frame's return value
  replaces it, whatever caused the frame.
- Requests are honoured only while the window is mapped and active,
  motion is allowed, and the app hasn't settled. Otherwise they wait; when
  the wait ends the layer is drawn once with `dt = 0` and carries on.
- "Next frame" is the display's refresh, at most 120 a second. A wait of
  1/30 s or more uses a plain timer, so a slow drift doesn't keep the
  toolkit's frame clock running. A request never comes due sooner than
  asked, to the millisecond, on the layer's own clock (`frame.time`); it
  may come a few milliseconds later.
- A plain timer is not in step with the screen. A layer that returns
  exactly `1 / 30` on a 60 Hz screen is drawn on every second frame when
  its timer comes due just before one, and on every third when it comes
  due just after: 20 pictures a second instead of 30, by chance (measured
  with Swirls). For steady motion at such a rate return a little less
  than `1 / 30`: the app then watches the screen's own frames and draws
  on the first one due. A frame may come up to half a millisecond before
  it was asked for.
- The app settles after 45 seconds without a key press, a pointer move
  over the window, an event or the window becoming active. Any of those
  wakes it.

### Events

Events are data: a hook reads them from `frame.events` each time it draws.
An event stays for as many seconds as the layer's `events` table says
(at most 5), with its `age` growing. At most 32 are kept per layer.

| `kind` | When | Extra fields |
|---|---|---|
| `"press"` | A control was used. | `source`, `via`, `key` |
| `"result"` | A calculation produced a value in a readout. | `cause`, `via` |
| `"error"` | It produced an error. | `cause`, `via` |
| `"show"` | A keypad came into view (first shown, or the mode changed). | |
| `"option"` | The user changed one of this theme's options. | `id`, `value` |
| `"mode"` | The user changed page or calculator mode (not sent for the page the app starts on). | |

Every event has `age`, `page` (`"calculator"`, `"converter"`, `"date"`,
`"graphing"`, `"settings"`), `rect` (the control, readout or keypad it
came from) and `x`, `y`: the exact point pressed for a pointer press on a
key, else the middle of `rect`. All in the layer's own coordinates.

- A keypad layer is told of its own keypad's keys and its own `show`,
  never of another keypad's. Everything else reaches it like any layer.
- An `option` event comes from nowhere in particular: its `rect` is the
  layer's own area.
- A layer that is not on screen is told nothing.
- A readout that is not on screen sends no `result` or `error` (another
  page's value changing behind the one shown; the date page's other
  result).

For `press`:

| `source` | What | `key` |
|---|---|---|
| `"key"` | A keypad key (also the graphing keypad's) | `{ id, kind }`: `kind` is `"number"`, `"operator"`, `"func"`, `"equals"`, `"toggle"` or `"other"`; `id` is a number that is the same for the same key every time. Use it for variety, not meaning. |
| `"button"` | The memory row, F-E and their like | |
| `"swap"` | The converter's swap button | |
| `"date"` | A date was picked | |

`via` is `"pointer"` or `"keyboard"` (a key typed on the keyboard presses
the matching on-screen key), or `"assistive"` when the control was
operated through the accessibility interface.

A press also has `action` when the control does one of three things:
`"clear"` (C, CE), `"backspace"` or `"memory"` (any memory button).

A `result` or an `error` has `cause`, what brought it about: the press
just before it. `"equals"` is = (or Enter), `"operator"` the running total
+ − × ÷ show (`1 +` shows 1), `"function"` any other function key, and
`"digit"`, `"clear"`, `"backspace"`, `"memory"` or `"button"` the rest
(the converter's value follows each digit). It has that press's `via`
too. With no press just before it (a paste, a unit chosen, the rates
arriving), it has neither.

### The draw API

Every call maps onto one GtkSnapshot call or a fixed few. Coordinates are
the layer's; paints may be colours or gradients everywhere.

| Call | GtkSnapshot / GSK |
|---|---|
| `draw:rect(x, y, w, h, paint)` | `append_color`, `append_linear_gradient`, `append_radial_gradient` or `append_conic_gradient` |
| `draw:rounded_rect(x, y, w, h, radius, paint)` | `push_rounded_clip` + the above + `pop` |
| `draw:circle(cx, cy, r, paint)` | `append_fill` of a circle path (gradient: `push_fill` + gradient + `pop`) |
| `draw:fill(path, paint, rule)` | `append_fill`, or `push_fill` + gradient + `pop`; `rule` is `"winding"` (default) or `"even-odd"` |
| `draw:stroke(path, stroke, paint)` | `append_stroke`, or `push_stroke` + gradient + `pop` |
| `draw:border(x, y, w, h, radius, width, color)` | `append_border` |
| `draw:shadow(x, y, w, h, radius, shadow)` | `append_outset_shadow` or `append_inset_shadow` |
| `draw:text(text, x, y, font, paint)` | `append_layout` (gradient: `push_mask` + `append_layout` + `pop` + gradient + `pop`); `x, y` is the top-left |
| `draw:text_size(text, font)` → `w, h` | Pango layout pixel size; draws nothing |
| `draw:glyph(glyph, paint)` | `append_layout` of one laid-out character of the display's value, at its place |
| `draw:image(image, x, y, w, h, options?)` | `append_scaled_texture`; `options.filter` is `"trilinear"` (the default for stills), `"linear"` or `"nearest"`. An [animation](theming-media.md#animated-pictures) draws its current frame. On a screen of more pixels than the window's (scale 2) a `"nearest"` picture is set in the screen's own pixels: the toolkit's renderers would scale it to the window's first and then smoothly up, and its hard pixels would be soft. |
| `draw:field(x, y, w, h, spec)` | A grid of numbers coloured per pixel by the app through a ramp, then `append_scaled_texture`. See [Fields](theming-media.md#fields-swirls-plasma-contours). |
| `draw:tile(image, x, y, w, h, options?)` | `push_repeat` + `append_texture` + `pop`: the image repeated at its own size. Of `options` only `filter` is read, and only on a screen of more pixels than the window's: `"nearest"` keeps its pixels hard there, as `draw:image`'s does (the default, `"linear"`, is smooth) |
| `draw:push_clip(x, y, w, h, radius?)` | `push_clip` / `push_rounded_clip` |
| `draw:push_opacity(opacity)` | `push_opacity` |
| `draw:push_blur(radius)` | `push_blur` |
| `draw:push_shadow(shadows)` | `push_shadow` |
| `draw:push_mask(mode)` | `push_mask`: draw the mask, `pop`, draw what shows through it, `pop`. Modes: `"alpha"` (default), `"inverted-alpha"`, `"luminance"`, `"inverted-luminance"` |
| `draw:push_blend(mode)` | `push_blend`: draw what is underneath, `pop`, draw what is blended onto it, `pop` |
| `draw:pop()` | `pop` |
| `draw:save()`, `draw:restore()` | `save`, `restore` |
| `draw:translate(dx, dy)`, `draw:scale(sx, sy?)`, `draw:rotate(degrees)` | `translate`, `scale`, `rotate` |

A **stroke** is a width, or `{ width, cap, join, dash, dash_offset }`
(`GskStroke`). A **path** comes from `gmnb.path()`, a builder
(`GskPathBuilder`) with `move_to`, `line_to`, `quad_to`, `curve_to`,
`close`, `rect`, `rounded_rect`, `circle`, each returning the path so
calls chain; or from `gmnb.svg_path("M6 12.5h12")` (`gsk_path_parse`).
A circle's radius, and a rounded rectangle's corners in a path, are taken
up to 10^6 (as is `draw:circle`'s), and SVG data may reach no further than
10^6 from the origin, every relative step counted, nor have an arc of a
larger radius (else `gmnb.svg_path` is an error; an arc's radii too small
for its two ends are grown to reach them, as SVG has it, and it is the grown
radius that counts): the toolkit's circles,
corners and arcs cost it more to draw the larger they are in the path's
own units, and the further from the origin (it works them out in single
precision, to half a unit). Measured on its software renderer, a circle
of radius 10^7 took a tenth of a second, one of 10^6 about (10^6, 10^6)
half that, and an arc of radius 10^10 hung it. So a circle or a rounded
rectangle (`draw:circle` too) whose points reach past 10^5 is drawn as
the cubic curves it is made of, which cost no more out there.

Pictures: `gmnb.image("images/stars.png")` loads a PNG, JPEG, WebP or GIF
from the theme's folder (decoded by the app itself, not by the toolkit's
loaders: see [Decoding stills](theming-media.md#decoding-stills)). `gmnb.noise({ size, seed, color, opacity })`
makes a square of speckles: per pixel, in row order, a 32-bit xorshift
(`s ^= s << 13; s ^= s >> 17; s ^= s << 5`, starting from `seed`, default
`0x9e3779b9`) gives `n = (s & 0xff) / 255`, and the pixel is `color` at
alpha `n × opacity` (times the colour's own), premultiplied, each part cut
down to 8 bits (`GdkMemoryTexture`, RGBA8). Both are
remembered by their arguments: calling them every frame costs nothing
after the first.

`gmnb.animation` (GIF, APNG, animated WebP), `gmnb.pixels` (a picture
filled from a Luau `buffer`) and `gmnb.ramp` are described in
[Pictures](theming-media.md#pictures). A layer that draws an animation is
drawn again when the animation's next frame is due, as if `draw` had
returned that delay; all the rules under
[Asking for the next frame](#asking-for-the-next-frame) apply to it.

Pushes must be popped and saves restored by the time `draw` returns, in
the order they were made (a `save` made inside a push is restored before
that push is popped). The app closes what was left open and reports it
once. `push_mask` and `push_blend` each count as two pushes.

What a call does with what it is given:

- **A wrong argument** (a colour that isn't one, a path that isn't a
  path, a missing paint) is an error at that line, like any error in
  Luau. Unless the hook catches it, the frame fails.
- **Going over a cap** of the [limits](#limits-and-what-happens-when-a-theme-fails)
  is an error too, and the frame is dropped whatever the hook does after,
  catching the error included.
- **A number that isn't finite** is refused. Opacities, colour parts and
  gradient stops out of range are brought into range without a word;
  negative sizes and radii count as 0. Coordinates are taken up to
  10^7 either way, and so is a scale; a `draw:scale` that with the scales
  already in force would make sizes more than 10^14 times larger is
  refused too (an error at that line), since one inside another they
  multiply. (Only those that enlarge are counted: one that shrinks takes
  nothing back.) (Six of 10^7 are past a float's range, and the toolkit's
  software renderer, given that and a blur, stopped drawing the whole
  window.) A `restore`, or the `pop` of a push, gives back the scales
  made since. And whatever the scales, a call that would draw more than
  10^8 pixels from the layer's origin is refused (an error at that line):
  a text by its position and its bytes at its font's size, a path by its
  furthest point, a shadow with its offset and blur. (Given a picture
  10^11 pixels across to shadow, a scale of 10^4 and a rectangle of 10^7,
  the software renderer crashed.)
- `draw:text`'s font is laid onto `text.body` of the style; `nil` is that
  font as it is. A font that can't be read is noted and the rest of it
  used.
- A picture that didn't load (see the limits) has no size and draws
  nothing.

A path holds what its builder calls put in it and can be kept between
frames and added to; it is read when it is drawn. A glyph of
`frame.value.previous` carries `previous = true`.

### The keypad layer

`frame.keys` lists every cell of the keypad, keys and the few other
widgets that sit in its grid:

| Field | Meaning |
|---|---|
| `rect`, `row`, `col` | Where it is, in pixels and in the grid (from 0). |
| `kind`, `id` | As in events. `id` is missing on cells that aren't calculator keys. |
| `radius` | Its corner radius, from the style. |
| `enabled`, `hovered`, `pressed`, `checked` | Its state. |

`place(key, frame)` is called for each cell before it is drawn and may
return `{ dx, dy, scale, opacity }`: the cell is scaled about its centre,
moved and faded. `nil` leaves it alone; `opacity = 0` skips it. It changes
the picture only: clicks, focus and screen readers see the key where it
really is. It is not called under reduced motion.

```lua
-- Keys rise into place when the keypad appears.
place = function(key, frame)
	local show = frame.events[#frame.events]   -- this layer listens for "show" only
	if not show then return nil end
	local p = math.clamp((show.age - (key.row + key.col) * 0.02) / 0.4, 0, 1)
	if p >= 1 then return nil end
	return { dy = (1 - p) * 16, opacity = p }
end
```

`draw` must return `0` while `place` is still moving keys; the app does
not guess. If `place` fails, or its calls together run past their limit,
no key is moved that frame.

- The cells listed are the ones on screen, by row and column.
- `pressed` is also true for the 110 ms a key typed on the keyboard is
  shown pressed; `checked` is a toggle that is on.
- The graphing page's small pad is not a keypad to themes: its keys send
  `press` events, and nothing is drawn over it.

### The display layer

The app lays the value out: it picks the size that fits, shapes the text
in `display.value.font` (a converter value in `converter.value`'s font, a
date result in `date.result`'s), and positions each character. The hook
paints. A currency's symbol before a converter value is not part of the
value: the app paints it.

`frame.value`:

| Field | Meaning |
|---|---|
| `text` | The value as shown. |
| `glyphs` | One per character: `{ index, x, y, w, h }`, `index` from 1. Pass them to `draw:glyph`. |
| `size` | The fitted font size. |
| `rect` | The box around the whole value. |
| `band` | The strip the value sits in, as wide as the readout: `rect` grown by 4 px above and below. Spread vertical gradients over this, so digits don't change colour as the number grows. |
| `error` | The readout shows an error. |
| `align` | `"end"` (the calculator) or `"start"` (converter, date). |
| `role` | `"calculator"`, `"converter"` or `"date"`. |
| `change` | While a change is being shown: `{ kind, age, from }`. `kind` is `"typing"`, `"result"`, `"replace"` or `"error"`; `from` is the first glyph that differs from the old value. |
| `previous` | The old value's `{ glyphs, size, rect }`, for as long as `change` is there. |

A change starts when the value changes. It ends when `draw` returns `nil`
(or after 3 seconds): the app then drops `change` and `previous` and calls
`draw` once more for the picture at rest. Under reduced motion `change`
is never set.

The expression line and the see-through halo are the app's
(`display.expression`, `display.halo`), and so is everything a screen
reader hears. If the hook fails, the app paints the value the base way:
**a number is never missing.**

### Previews

A `choice` option may have `preview(draw, frame, choice)`. It draws one
tile for one choice with the same draw API; `frame` is
`{ width = 96, height = 56, ctx }`. Tiles are still pictures, redrawn
when `ctx` changes. Settings draws them in passes of a shared budget
([Limits](#limits-and-what-happens-when-a-theme-fails)), after whether
each option is shown: until its picture comes, a tile shows a faint
blank of its size, and after a change of `ctx` each keeps the picture it
had until its new one is drawn.

## Options

```lua
options = {
	{ id = "palette", type = "choice", group = "Palette", label = "Palette",
	  description = "Colours for the aurora, keys and graphs", default = "aurora",
	  choices = { { id = "aurora", label = "Aurora" }, { id = "ember", label = "Ember" } },
	  preview = function(draw, frame, choice) … end },
	{ id = "primary", type = "color", label = "Primary colour", default = "#7c4dff",
	  visible = function(options, ctx) return options.palette == "custom" end },
	{ id = "living", type = "toggle", group = "Motion", label = "Living background", default = true },
	{ id = "grain", type = "slider", label = "Grain", min = 0, max = 1, step = 0.05,
	  default = 0.5, format = "percent" },
}
```

| `type` | Value | Settings shows |
|---|---|---|
| `"choice"` | one of the `choices`' ids (a string) | With `preview`: a grid of 96×56 tiles, each captioned and named for screen readers by its label, one selected. Without: a dropdown row. |
| `"color"` | `"#rrggbb"` | A row with a colour button that opens the desktop's colour chooser (no opacity). |
| `"toggle"` | boolean | A switch row. |
| `"slider"` | a number from `min` to `max` in steps of `step` | A row with a slider and the value beside it, as a number or (`format = "percent"`) a percentage. |
| `"note"` | none | The `label` as small dimmed text. For explanations. |

Common fields: `id` (letters, digits, `_`; unique in the theme), `label`,
`description` (the row's second line), `group` (options with the same
`group` sit under that heading, in the order given), `default`.

- **`visible(options, ctx)`** returns whether the option is shown now. It
  is asked again after every change. Only `false` hides the option: if
  the function fails, runs out of its 10 ms or returns anything else, the
  option is shown.
- **`actions`** are small buttons at the end of an option's row. An
  action's `run(options, ctx)` returns the options to change:

  ```lua
  actions = { { label = "Complement",
                tooltip = "Use the colour opposite the primary on the colour wheel",
                run = function(o) return { secondary = gmnb.to_hex(gmnb.complement(o.primary)) } end } }
  ```

  The returned values are checked like any other and saved, all together: `style` runs once for them all, not once for each.

Changing an option saves it, calls `style` again, and sends an `option`
event to layers that listen for it.

Limits: 24 options, 16 choices each, labels up to 80 characters,
descriptions and notes up to 300.

### Where Settings puts things

Settings → Appearance, top to bottom:

1. **Mode**: Light / Dark / System (as now), and "Use the desktop's accent
   colour" (new; on by default; off makes `ctx.accent` `nil`).
2. **Theme**: a list with "GMNB" (the base look) first, then the themes
   folder's, the built-in themes' ids first (marked "Comes with GMNB"),
   then the rest; each with its `description`. Below it *Reload*, *Open
   themes folder*, and *Restore built-in themes* while a built-in theme has
   no folder ([The themes folder](#the-themes-folder)). A theme that failed
   shows its error here; so do its warnings (the latest 20).
3. **The chosen theme's options**, one group per `group`.
4. **Sound**: *Theme sounds* (a switch, on by default) and Volume (50% by
   default). See [Settings](theming-media.md#settings).
5. **Window**: Background opacity, Vulkan acceleration (as now).

### How options are saved

In `settings.json`, beside the existing keys:

```json
{
  "theme": "dark",
  "active_theme": "aurora",
  "theme_options": {
    "aurora": { "palette": "ember", "primary": "#7c4dff", "secondary": "#ff6fb5", "living": true }
  },
  "system_accent": true
}
```

- `theme` keeps its meaning: `"system"`, `"light"` or `"dark"`.
- `active_theme` is a theme id; `""` or missing is the base look. An id
  that isn't installed falls back to the base look and is kept, so the
  theme returns when its folder does.
- `theme_options[id]` holds only what the user changed. A saved value that
  no longer fits its option (wrong type, a choice that is gone, out of
  range) is ignored and the default used. Options the theme no longer has
  are kept, not deleted.
- `sound` (boolean, default `true`) and `sound_volume` (0 to 1, default
  0.5) are the app's, not a theme's.
- Each of these loads by itself: one mistyped entry costs only that entry,
  as with the existing settings.

**Coming from 0.2.** People start on the base look, and Aurora is as they
left it. On loading a file with no `theme_options.aurora`:

| 0.2 key | Becomes |
|---|---|
| `palette` | `theme_options.aurora.palette` (if it is one of the seven) |
| `custom_primary`, `custom_secondary` | `theme_options.aurora.primary`, `.secondary` (if valid hex) |
| `animated_background` | `theme_options.aurora.living` |

`active_theme` is left unset. The four old keys are kept and written
back unchanged, so going back to 0.2 still finds the palette it had;
0.3 never reads them again once `theme_options.aurora` exists (the
migration always makes it, with `living` at least).
The existing test becomes: `{"theme":"dark","palette":"ember"}` loads with
`theme == "dark"`, `active_theme == ""`,
`theme_options.aurora.palette == "ember"`, opacity 1 and Vulkan on.

## The `gmnb` library

Colour maths is `appcore::color`, the same code both apps use, done in
32-bit floats. Functions take any Color and return `Rgba`.

| Function | Returns |
|---|---|
| `gmnb.hex("#7c4dff")` | The colour; an error if the text isn't `#rrggbb` or `#rrggbbaa`. |
| `gmnb.color(c)` | Any Color as an `Rgba`. |
| `gmnb.rgb(r, g, b, a?)`, `gmnb.hsl(h, s, l, a?)` | A colour from parts (hue in degrees, the rest 0 to 1). |
| `gmnb.to_hsl(c)` → `h, s, l`; `gmnb.to_hex(c)` | The parts; `"#rrggbb"`. |
| `gmnb.mix(a, b, t)` | `a` moved the fraction `t` of the way to `b`. |
| `gmnb.alpha(c, a)` | `c` with its opacity multiplied by `a`. |
| `gmnb.over(top, bottom)` | `top` composited onto `bottom`. |
| `gmnb.complement(c)` | The hue opposite, same saturation and lightness. |
| `gmnb.luminance(c)`, `gmnb.contrast(a, b)` | WCAG 2 relative luminance; contrast ratio, 1 to 21. |
| `gmnb.worst_contrast(c, { … })` | The lowest contrast of `c` against any of the list. |
| `gmnb.with_contrast(c, against, target)` | `c` with its lightness moved as little as possible to reach `target` against `against`. |
| `gmnb.gradient_for_text(text, a, b, target)` → `a, b` | The two stops adjusted until `text` reaches `target` at both ends and the middle. |
| `gmnb.readable_gradient(a, b, light, dark, target)` → `text, a, b` | Picks `light` or `dark` text, whichever reads better, then adjusts the stops. |
| `gmnb.base(dark, accent?)` | The base [colours](#colors) for that mode and accent. |
| `gmnb.white`, `gmnb.black`, `gmnb.transparent` | Constants. |
| `gmnb.TEXT_CONTRAST`, `gmnb.UI_CONTRAST` | 4.5 and 3: WCAG's minimum for body text, and for large text and meaningful lines. |

| Function | Returns |
|---|---|
| `gmnb.linear(spec)`, `gmnb.radial(spec)`, `gmnb.conic(spec)` | A gradient ([Paint](#paint-fill)). |
| `gmnb.path()`, `gmnb.svg_path(d)` | A path. |
| `gmnb.image(file)`, `gmnb.noise(spec)` | A picture: `{ width, height }` plus what the app needs. |
| `gmnb.ease.linear`, `.in_quad`, `.out_quad`, `.in_out_quad`, `.in_cubic`, `.out_cubic`, `.in_out_cubic`, `.out_quart`, `.out_back` | `p` (0 to 1) eased. `out_back` overshoots: `1 + 2.70158 (p − 1)³ + 1.70158 (p − 1)²`. |
| `gmnb.warn(message)` | Nothing; the message appears under the theme in Settings and on stderr. |
| `gmnb.api` | The API version the app speaks: 1. |

`print` goes the same way as `gmnb.warn`.

Media and sound ([their chapter](theming-media.md)):

| Function | Returns |
|---|---|
| `gmnb.animation(file)` | An animated picture (GIF, APNG, animated WebP): `{ width, height, frames, duration }`. |
| `gmnb.pixels(width, height)` | A picture the theme fills: `:set(buffer)` copies RGBA bytes into it. |
| `gmnb.ramp(spec)` | A colour ramp for `draw:field`. |
| `gmnb.synth(recipe)` | A sound rendered from a recipe: oscillators, envelopes, filters, effects. |
| `gmnb.sound(file, { gain }?)` | A sound from a WAV or Ogg Vorbis file. |
| `gmnb.note(name)` | A note name's frequency in hertz: `gmnb.note("A4")` is 440. |

## Limits, and what happens when a theme fails

A theme is someone else's code running inside the calculator. It gets a
sandbox with hard limits, and it can only ever affect how GMNB looks and
sounds.

| Limit | Value |
|---|---|
| Memory of the theme's Luau machine | 8 MiB of the theme's own; the tables the app builds for a call (`ctx`, a frame) are counted apart, up to 1 MiB more |
| Time: loading the file | 250 ms |
| Time: `style` | 50 ms |
| Time: one `draw` call; all `place` calls of one keypad frame | 8 ms; 2 ms |
| Time: one `preview`, `visible` or action | 10 ms |
| Time: Settings' `visible` and `preview` calls together | 10 ms a pass. Once a pass's calls have used it, no more start that pass (the one running keeps its own 10 ms, so a pass is never much over 20 ms); the rest wait for the next pass, an idle callback once the window has drawn and taken its input, while Settings is open. Whether options are shown is asked first, then the tiles, in order. Opening Settings, and each change of `ctx` while it is open, runs one pass at once. |
| One library call's own work, which the time limit can't stop midway: a plain search (`string.find` with `plain` or a pattern of no magic characters, `string.split`); a pattern search (`find`, `match`, `gmatch`, `gsub`); `table.move` | 2^30 bytes (string × pattern); 2^26 (string bytes × the pattern's longest `[set]`); 2^20 elements. Past them the call is an error. |
| Draw calls per `draw` | 4096 |
| What a frame costs to draw | 1000 points for each 760 × 640 of what the layer is charged over (a keypad's or a readout's, the window too), and never fewer than 1000 ([Drawing per frame](#drawing-per-frame)) |
| Open pushes / saves at once | 32 each |
| Path segments per frame | 16 384 |
| Text per `draw:text` call; text calls per frame | 256 bytes; 64 |
| Blur radius; shadow blur | 100 px (a `push_shadow`'s, of the layer's pixels with the scales in force) |
| Images: file; decoded size; count; total decoded | 4 MiB; 2048×2048; 16; 32 MiB |
| Noise: size; count | 512×512; 8 |
| Fonts: file; count; total | 16 MiB; 8; 32 MiB |
| Any string the theme returns or passes to the app | 4 KiB (longer: refused); a line of `print` or `gmnb.warn` is cut short at 16 KiB instead, and the log keeps no more than its last 200 lines and 16 KiB |
| A `credits` text file; entries | 64 KiB; 8 |
| Options: per theme; actions on one; family names in one font | 24; 4; 8 |
| Events kept per layer; longest kept | 32; 5 s |
| Images: formats | PNG, JPEG, WebP, GIF |
| Animations: file; frame size; frames; count | 8 MiB; 1024×1024; 2048; 4 |
| Animations: counted against "Images: total decoded" | By what playing one holds: a GIF 4¼ of its frames (4 and its largest frame at a byte a pixel, and an eighth more, if that is larger: a GIF's frames may be larger than its canvas), a WebP 5, an animated PNG 6 (a frame is width × height × 4 bytes); a file of one frame, that frame |
| Animations: frames shown; decoding | 30 a second; a quarter of one core, all animations together |
| Pixel pictures: size; count; changes | 512×512; 4; 30 a second each |
| Fields: points; per `draw`; computed pixels per call; passes | 16 384, each side 2 to 256; 2; 262 144; 30 a second per layer |
| Ramps: colours; held at once | 2 to 16; 8 (a theme may make any number: one let go is made again, the same, when it is drawn again) |
| Sounds: count; length; all together | 32; 4 s; 8 MiB (43 s) |
| Sound files: size; kinds | 1 MiB; WAV (8, 16, 24 or 32-bit, or float), Ogg Vorbis; 1 or 2 channels; 8 to 96 kHz |
| Sound recipes: layers; effects; layer-seconds over the whole theme | 8; 4; 64 |
| Voices at once; plays per event; the same event again | 12; 4; not within 30 ms |
| A `sounds` entry's list | 8 sounds |
| A play: `volume`; `rate`; `pan`; `delay` | 0 to 1; 0.5 to 2; −1 to 1; 0 to 1 s |
| Time: one `sound` call | 2 ms |

Every entry into the theme's code runs under its time limit, the first
load included. **A time limit is of processor time**: what the theme's
code has run, not what the clock on the wall says. A machine busy with
other things doesn't fail a theme that did nothing slow. Reading back
what a hook returned counts against its call (a table near the caps
takes some 10 ms to read). Files (`fonts`, `credits`, `gmnb.image`) are relative
paths inside the theme's own folder; `..`, absolute paths and links that
lead outside are refused. The same holds for `gmnb.animation` and
`gmnb.sound`. A picture is never decoded inside a theme's call (a still
when it is first drawn, after the hook has returned; an animation's
frames on a thread of their own); reading a file inside a call counts
against it like any other work, and is small (a tenth of a millisecond
for a picture of 2 MiB).

| What goes wrong | What the user sees |
|---|---|
| The file doesn't load: a syntax error, an error while running, the time or memory limit, no table returned, `api` missing | The base look, and a toast: "Theme ‘Name’ couldn't be loaded". Settings shows the message and the line. |
| `api` is newer than the app | The same, with "needs a newer GMNB". |
| `style` fails | The last style that worked, or the base look. A toast, once. |
| A value is of the wrong type or a key is unknown | That value's base stays. A note under the theme in Settings; no toast. |
| A number is out of range | It is clamped. A note. |
| A `draw` fails, runs out of time or exceeds a cap | Nothing is drawn for that layer that frame (the display paints the base way). After 3 failures the hook is switched off until the theme is reloaded, and a toast says so, once. |
| A `draw` takes more than four times its limit | Switched off at once (a library call the app couldn't interrupt). |
| `place` fails or runs out of time | The keys stay where they are that frame; `draw` still runs. After 3 failures `place` is switched off. |
| The memory limit is hit | The theme is unloaded: the base look, and a toast. |
| A font or image can't be read or is too big | The theme loads without it; a note. |
| An animation or sound can't be read, is too big, or isn't what its first bytes promise | The same: drawing it draws nothing, playing it plays nothing; a note. |
| A picture's file has a sound header and can't be decoded | It has its size and draws nothing; a note, when it is first drawn. |
| An animation is too heavy to decode in time | It plays slower; a note, once. |
| `sound` fails or runs out of time | The `sounds` table decides for that event. After 3 failures the function is switched off; a toast, once. |
| There is no sound system | Silence. Settings → Sound says so. No toast. |

### Drawing per frame

The caps on what a frame holds still let one frame take the toolkit tens
of seconds or gigabytes: 1365 blurs of the whole window took the software
renderer over ten seconds a frame, and 2000 circles over it held 4 GB on
the GL renderer, which makes a window-sized mask for each. So each call is
also charged what drawing it costs: its kind's points below, for each
760 × 640 of the layer it covers (its box, through the transforms in
force, cut to the clips open and to what can be seen, or brought into
view by a blur or a shadow open around it). An effect (`push_opacity`,
`push_mask`, `push_blur`, `push_shadow`) is charged for the box around
what was drawn inside it (a blur's grown by three times its radius, a
shadow's by its offset and three times its blur) when it is popped, and a
`push_blend` for all that can be seen (the software renderer blends whole
layers, whatever is in them); what is drawn inside is charged as well. A
picture drawn at all is also charged its own pixels, however small it is
drawn. A frame has 1000 points for each 760 × 640 of what its layer is
charged over (its own area; a keypad's or a readout's, the window's too,
as below), and never fewer than 1000 (a small layer, or one not yet sized,
spends little anyway: what it draws is charged only where it can be
seen); a call that takes it past them is an error, and the frame is
dropped, as past any cap (that layer draws nothing that frame; the rest
of the window is drawn). A keypad's or a readout's layer is charged for
what it draws wherever the window shows it, beyond itself too.

The points are what each kind was measured to cost over the whole of a
760 × 640 window, on the software renderer (processor time) and on GL
(memory held while the frame is drawn, on llvmpipe): 1000 are 100 ms of
the first or 256 MB of the second, whichever the call costs more of.
Measured at the cap, 49 kinds each as many times as a 760 × 640
background may draw it: the software renderer took 25 to 125 ms a
frame (the measurement's own spread is a fifth or so), and GL held at
most 243 MB more; a frame mixing them near the cap, effects inside
effects, 110 ms. The cap is a layer's: a window drawn again draws every
layer's picture, so a theme whose background, overlay and keypad are all
at their caps costs about three times that a frame (290 ms, measured);
and a larger window, or a screen of twice the pixels, in proportion to
its pixels. (GL on llvmpipe, which GTK doesn't choose by
itself, takes seconds a frame at the cap for blurs and shadows.)

| Over the whole layer | Points | So, at most, a frame |
|---|---|---|
| A rectangle in a colour; a linear gradient of 2 stops (each stop past 2, 2.5 more); a radial gradient; a conic one | 1.3; 2.2; 52; 130 | 770; 450; 19; 7 |
| A rounded rectangle | its paint's + 0.3 | |
| A path filled or stroked, a circle (a mask on GL): in a colour; with a gradient | 12.7 + the paint's; 24 + the paint's | 71; 38 (linear) |
| A picture scaled into its rectangle (a still, a frame of an animation, a field); tiled | 8.2; 5.5 | 120; 180 |
| A picture's own pixels, per 760 × 640 of them: smooth and clean (`"trilinear"`); smooth or hard | 43; 16 | a 2048 × 2048 still: 2; 6 |
| A border; `draw:shadow`; a clip with rounded corners | 1.3; 4.3; 0.9 | |
| Text and glyphs (their box: the font's size by the text's bytes, three sizes tall; a glyph, the readout's whole value); with a gradient | 7; 23 + the paint's | |
| `push_opacity`; `push_mask`; `push_blend` (all that can be seen) | 13; 26; 48 | 76; 38; 20 |
| `push_blur(r)` | 140 + 1.6 r (0 for no blur) | 3 at 100, 7 at 1 |
| `push_shadow`: each shadow of blur b | 71 + 0.87 b; 13 unblurred | 6 at 100 |

The built-in themes' busiest frames, at rest and mid-animation (their
tests assert it, Swirls' at every size from 300 × 400 to 3840 × 2160):
Swirls' cost 6% of what a frame may, Sprinkles' 12%, and Aurora's 11%
(a result just after the equals key, or a new palette's four rays; 9%
with thirteen keys' rays alive at once, and 20% with eight keys, a result
and a new palette all at the same moment; Enter held down, a press and a
result thirty times a second, 32 events alive, 25% at 300 × 400 and 57% at
760 × 640, its dearest; its background standing, 5.5%:
one field behind one mask and a gradient that runs straight down. A ray
is charged by what it covers, so in a strip of a layer, 200 × 2160, the
same moment costs 44%. 0.2's four blobs and two arcs, radial gradients
all, were a fifth standing and 56% at their dearest).
`Api::frame_cost` and `Api::busiest_frame` (in `gmnb-theme`) say what the
last frame cost, and the dearest since the theme was loaded, as a share.

To get out of a theme that makes the window unusable, start GMNB with
`--theme base` (or `GMNB_THEME=base`): it starts in the base look without
changing the saved choice.

## Versions

- A theme states `api = 1`. The app speaks one or more versions and says
  the newest in `gmnb.api`.
- **Unknown keys warn; they never fail.** A theme written for a later
  minor addition still loads on an older app, without the part it doesn't
  know.
- API 1 only grows: new elements, properties, events, draw calls and
  `ctx` fields can be added without changing `api`. Anything that would
  change what an existing theme looks like, or remove something, makes
  it `api = 2`, and the app keeps loading version 1 themes as before.
- The base look can change between releases. A theme that returns `{}`
  follows it; that is the point.
- Added to API 1 this way so far: animations, `gmnb.pixels`, `draw:field`
  and ramps, sound (`sounds`, `sound`, `gmnb.synth`, `gmnb.sound`), the
  `mode` event, `action` and `via = "assistive"` on presses, and
  `draw:image`'s options. What was left for later is listed in
  [What is in API 1 and what is deferred](theming-media.md#what-is-in-api-1-and-what-is-deferred).

## What is scripted and what is built in

How each of Aurora's effects is made:

| Effect | How | Why |
|---|---|---|
| The seven palettes; System and Freestyle generated from one or two colours | **Scripted** (`style`, with the `gmnb` colour functions) | Pure data and arithmetic. |
| Glass keys, glass panels, the equals key's glow, lit bits, the whimsy chip, the sidebar's and flyouts' tints | **Declared** (style values: gradients, shadow lists). In the dark the glass is smoked with the sky's own colour, thin on keys and thicker on panels and under text that lies straight on the page; in the light it is milk glass | They are the toolkit's own box drawing. (White glass, as 0.2 had it in the dark, turns a night sky grey.) |
| Hover brightness, the press that springs back, the swap button's turn | **Declared** (states and transitions) | The toolkit animates them, and stops for reduced motion. |
| Readable over an aurora that moves | **Scripted** (`style`), from both sides: each of a palette's lights is tempered (the same hue, as saturated, nearer the sky's lightness) until the text over it reads through the thinnest glass there is; then every ink is worked out against each pane over the sky and over each light | A curtain's hem wanders under every key in turn. A palette can be any two colours. |
| The see-through frost and halos | **Scripted** (`style`): the frost under every surface that carries text is as strong as that palette's inks need over what the window shows at `ctx.opacity` (the desktop, black or white, through the thinned sky, and the lights over that), in sixty-fourths; and never less than two fifths of the way to solid by the time the sky is gone | A fixed tint is too much at 0.9 and, for some palette, too little at 0.1; a step that shows makes the slider jump; and what shows through may be a photograph, busy behind a key's sign in a way no contrast figure knows about. |
| The aurora: three curtains hanging along hems that wander, bright at the hem and fading upward in rays, folds travelling along them | **Scripted field** (`background` layer, `draw:field`): how bright, as one number a point on a grid of some 9,000, three snapshots a second worked out a quarter a frame; the app glides between two of them fifteen times a second | The light is smooth, so a coarse grid holds it; its rays are 20 px wide, so the grid's cells are narrow and tall. Per pixel in Luau it would take the whole of a frame's 8 ms many times over. |
| Its colours, by height: green at the hem, teal, violet and crimson at the crown | The field through **one ramp that is only white**, clear to opaque, drawn as a **mask** (`draw:push_mask`), and the palette's six lights showing through it, a gradient straight down the window | A theme may make eight ramps, and Freestyle makes a new palette at every step of a colour being dragged: the colours can't live in a ramp. A mask is filled with whatever colours there are. |
| The sky and its stars | **Declared** (`window.background`: a gradient, top to bottom) and **scripted** (160 stars at most, small squares in one colour, a window's worth by its size) | The sky is the window's own background, so the app thins it on a see-through window and it stands if the layer ever fails. |
| A pulse: a ray of light rising from a key, the sky lit round a result, four rays for a new palette | **Scripted**: soft ellipses drawn into the aurora's own mask | In the mask a pulse shows the palette's lights where it stands and can't be brighter than they are: what the text was proved over holds for every frame. Each key's ray is one of four heights, chosen as its note is. Thirty pictures a second while one plays: a soft light needs no more, and every picture of the background is one of the whole window. |
| See-through: the sky clears, the lights stay | **Scripted**: the layer says `fade = false` and thins itself: its lights at seven tenths or more of their strength at any opacity, its stars and grain gone with the sky | Faded with the sky, an aurora over a desktop at opacity 0.15 is 15% of an aurora. |
| Grain | **Scripted placement** of a **built-in** texture (`gmnb.noise`, `draw:tile`) | Filling 36 864 pixels per call is no job for a script; the recipe is specified so it never changes. It hides the banding of a dark sky's gradient. |
| Ripples, the pointer's light on key borders | **Scripted** (`keypad` layer) | |
| The key cascade | **Scripted** (`place`) | One small function per key per frame while it plays. |
| The display's gradient digits, glow, typing pop, result wave and shimmer, replace drift, error shake | **Scripted** (`display` layer). The app lays the text out and fits its size. | With glyphs handed over as drawable things, per-glyph animation is forty lines of Luau, and other themes can do something else entirely. Layout, fitting and accessibility must not be a theme's to get wrong. |
| Palette tiles in Settings | **Scripted** (`preview`) | |
| The graph's neon glow, canvas tint and rim, trace marker and bubble | **Built in, with parameters** (`graph.canvas`) | Curves are thousands of points, plotted on a worker thread, drawn again off screen for *Copy image*. A script would be slow and would have to be the plotter too. |
| Curves drawing themselves, the zoom glide, navigation icons drawing themselves | **Built in, timed by `motion`** | They need the length of each path. |
| The swatch's glow | **Declared** (`graph.swatch.glow`); the app supplies each equation's colour | |
| Page, mode and panel transitions; rows sliding in | **Built in, timed by `motion`** | The toolkit's stack and revealer. |
| The keyboard flash of a key | **Built in**: the key is `active` for 110 ms | |
| Outfit | A font file in the theme's folder, named in `fonts` | |

How each of Sprinkles' effects is made. Sprinkles is the other half of the API:
where Aurora computes light, it shows pictures, moves keys and makes a
noise.

| Effect | How | Why |
|---|---|---|
| The five flavours, and System and Freestyle | **Scripted** (`style`): a flavour is two colours, and every ink and surface is worked out from them against what it lies on. System's icing is the desktop's accent (`ctx.accent`, GNOME's blue when there is none) and its second colour the one most of the way round the wheel from it; Freestyle's are two colour options, shown only while it is chosen. Either keeps its hue and strength at a lightness icing has | Any colour at all can be an icing, so the readability tables are run for the extreme colours of rule 2 (`sprinkles_own_flavours_read`), all round the accent wheel and for random picks (`sweep.rs`); what the layers paint, in colours from both (`what_glazed_paints_reads`). Schemes are kept by flavour, colours and mode, a few dozen at most. |
| Keys as blocks: a line, a glint, a lip, a hard shadow; pressed, the lip is gone | **Declared** (borders and shadow lists with no blur; no transitions at all) | A state that changes at once is the look. |
| Short keys, in a window near its least (300×400, where Scientific's keys are fourteen pixels high and Programmer's twelve): a thin line, the lip under the key instead of inside it, the sign one Jersey pixel to the pixel | **Declared** (`keys.short`, below 24 pixels: where a number's capitals no longer fit between its key's lines), its gap 3 | The keys shrink with the window instead of holding it open; what has room at forty pixels has none at fourteen. |
| The tiled wall | A **still** the theme's script draws (`gmnb.image`, `draw:tile`, hard pixels at scale 2 too) | Four colours, 200 bytes; tiling it costs one node. |
| The awning's hem in the flavour's colours | Three **stills used as masks** (`draw:push_mask`, `draw:tile`, then a rectangle of the colour) | One set of pictures serves every flavour, light and dark: a still can't be recoloured, a mask can be filled. |
| Sprinkles falling down the wall | An **animated picture** (`gmnb.animation`, a 128-frame GIF the script writes), drawn side by side with hard pixels | The loop repeats in space as it does in time, so nine copies of one small animation cover a window, decoded once off the interface's thread. The app schedules its frames; the hook returns nothing. |
| The icing on the digits of a result (one that = or a function key gave): it pours, its drips run ahead, sprinkles land on it | A **picture the theme computes** (`gmnb.pixels`, 256×28, a dozen steps of the pour) shown through the **glyphs as a mask** (`display` layer); only on digits of 28 px and more, because the icing is held to large text's 3:1 | A drip is a pixel-stepped edge no gradient makes, and through the digits' own shapes it needs no knowledge of the typeface but two of its measures. |
| The doughnut on the readout's shelf: it nods, blinks, jumps for a result, ducks as the number grows towards it | A second **computed picture** (24×24, worked out from a few numbers: squash, face, where it looks), placed by the `display` layer from the events' ages | No sprite sheet: one function is every pose, in every flavour, and Settings' tiles draw the same function as rectangles. |
| Keys that sink under a finger, hop when let go, stack themselves when the keypad appears; the hop a result sends through them; the shake of an error | **Scripted** (`place`), in whole art pixels | The stylesheet has no "move down two pixels" for a state, and no events. |
| The fountain of sprinkles from the = key; a new flavour's shower | **Scripted** (`overlay` layer): thirty rectangles a frame for a second | Over the keys, never over the number: the throw is worked out from where the readout ends. |
| The same burst from frame to frame | **Scripted**: events carry no name, so the theme knows one again by its place and its growing age | A burst seeded by the frame would flicker. |
| The equals key's icing and sprinkles; crumbs under a press | **Scripted** (`keypad` layer) | |
| A scale to each flavour, a jingle timed to the icing | **Scripted** (`sound`, `gmnb.synth`): thirteen recipes; a key's note is one sound at another speed. System plays in G major's five notes; Freestyle in the key of the flavour whose icing is nearest its own round the wheel (a grey icing, in System's) | Sound that follows the look needs the same options the look has. |
| Jersey 10 | A font file in the theme's folder, set at whole multiples of its own pixel where it has to be sharp (the readout, the number keys), and at one and a half for words | The app sets text unhinted, to a quarter of a pixel: a pixel face is only crisp where its pixels are whole ones. |
| The flavours' tiles in Settings | **Scripted** (`preview`) | |
| A see-through window: the shop's window | **Scripted** (`background` layer with `fade = false`, and `style`): the layer paints the wall at the window's opacity itself, and as it goes the shop front's glass comes in where it was, from none at 0.95 to all at 0.6 (a frame round the counter in the line's colour, two stepped streaks of shine); the awning it paints stays solid, and from 0.95 down the header is the awning itself (`header.background`: sixteen stripes in three gradients' runs, which between 1 and 0.95 come in over the layer's own, both showing); `window.background`, what the title lies on until then, stays whole down to 0.95 and goes from 0.95 to 0.9 | Faded with the window, the wall, the awning and all of the shop were a fifth of themselves at 0.2, and the header a plain card that appeared at 0.99. The awning in style keeps what the title lies on where the tables see it, and holds if the layer is ever off. |

What Sprinkles asks of the engine while it lives, measured as Aurora's was
(release build, 760×640, the software renderer): the wall's loop at eight
frames a second costs 5 to 6% of a core, where Aurora's drift takes 15 to 17%;
at rest, in a window not in use, under reduced motion and once the app
has settled, nothing runs at all.

There are no effect presets: nothing in the app is specific to Aurora or
to Sprinkles.

### How each of Swirls' effects is made

Swirls is flowing paint inspired by ly's ColorMix (the
ColorMix field of [ly](https://github.com/fairyglade/ly), WTFPL), with the
calculator standing in it. Aurora's field is light behind a mask;
Swirls' is the whole picture, its colours in the ramp, and everything in
the theme is built on that.

| Effect | How | Why |
|---|---|---|
| The swirl | **Scripted field** (`background` layer, `draw:field`, `gmnb.ramp`): the theme works out the design's bent coordinate on a coarse grid in a Luau `buffer`, five times a second; the app glides between two snapshots and colours every pixel through the ramp, thirty times a second | The smooth part is cheap to compute coarsely and the sharp part (the bands' edges) is the app's, per pixel. The same picture per pixel in Luau takes 62 ms a frame. |
| One cost at every window size | **Scripted**: the grid has about 3,600 points whatever the window's size (a point every 12 px at 760×640, every 48 px at 4K), at most 256 a side, and the app is asked for about 160,000 pixels of picture (`quality`: half size at 760×640, less in a bigger window) | The bands grow with the window, so they are as smooth on the coarser grid, and a frame never costs more. |
| No frame carries a whole snapshot | **Scripted**: the snapshot *after* the two the picture glides between is worked out a fifth of its rows a frame | A snapshot is 1.6 ms of sines; a fifth of one is 0.3. Only the first frame at a new window size works out a whole one. |
| A slow machine keeps the theme | **Scripted**: that first snapshot times its first nine rows (`os.clock`). Where a whole one would take over 5 ms, the grid and the picture are made coarser by as much as brings it to 3.5; and a frame that has used 4 ms leaves the oldest disturbances for the next | A hook that overruns its 8 ms three times is switched off for good. |
| A frame drawn in between costs nothing | **Scripted**: the layer's clock is quantised to thirtieths of a second. Drawn again inside one (an event came, the colours changed), it asks for the numbers it had, and the app has that picture already | The app colours a layer's fields 30 times a second at most; a theme that asks for more is refused and owed a frame. |
| Thirty pictures a second, in step with the screen | **Scripted**: the hook returns the time left of its thirtieth, never a whole thirtieth | A wait of 1/30 s or more is a plain timer (see [Asking for the next frame](#asking-for-the-next-frame)), which is not in step with the screen: the paint ran at 20 pictures a second in one run and 28 in the next. |
| Stirring, the ripple, the shudder | **Scripted**: a press, a result or an error moves *where the field is looked up* (a twist about the key that swings back; a ring pushed outward from the readout), inside the box of grid points it touches, on both snapshots | The field itself is three sines deep; looking it up somewhere else is four reads and three blends a point, for a few hundred points. |
| Typing hurries the paint | **Scripted**: each recent press adds to the pace at which the swirl's phase advances, wearing off in a second | A function of the events the layer is handed, with their ages: no state to keep. |
| Glass keys and panes; the design's hairline, hover and focus ring | **Declared** (style values) | Box drawing is the toolkit's. |
| How strong the glass is | **Scripted** (`style`): the least strength at which every ink reads over every band, for this set of bands at this window opacity; the glass chosen is never thinner | White on the design's gold is 1.4 to 1. See [the readability rules](#readability-rules). |
| A pressed key clears | **Declared** (`keys.active` with a thinner background, and a slow `transition` back) | The toolkit animates it. |
| Figures made of paint | **Scripted** (`display` layer): the glyphs as a mask (`draw:push_mask`), and behind them a second, small field: a slice of the swirl as one number a point, kept inside the ramp's first band | With one channel and values between 0 and 1 a three-colour cyclic ramp gives only its first two colours and the blend between them, so no figure is ever dark. One ramp serves both layers. |
| System and Freestyle | **Scripted** (`style`, `gmnb.ramp`): System's bands from the desktop's accent (the accent, kept vivid; the colour as far round the wheel from it as the design's cyan is from its gold; a near-black in the second's hue), Freestyle's from three colour options shown only while it is chosen. Whatever they are, the dark band keeps their hue at the design's near-black and the bright ones are lifted only as far as every blend between them needs to read as a figure on a panel over white paint. A new ramp each time the colours change | The glass is made of the dark band and the figures of the bright ones: any colour can be given, so the bands are made fit for both before anything else is worked out. A theme may make any number of ramps (eight are held at once). |
| Every page in the least window, a long figure in its readout | **Declared**: keys and bits with no least size of their own; the readouts' figures name their optical size | The window may be 300×400; a readout shrunk in Inter's smaller cuts would run past its end ([Font](#font)). |
| The panel's hairline round the readout | **Scripted** (`display` layer, `draw:border`) | The readout's own style has a background and a radius, no border. |
| Keys swirling into place | **Scripted** (`place`): each key turned about the keypad's middle and brought in, the nearest first | |
| The tiles in Settings | **Scripted** (`preview`): a 17 × 11 field through each set's ramp (System's and Freestyle's as their colours are now) | A tile is drawn once and stands. |
| The sounds | **Recipes** (`gmnb.synth`) and a **scripted** choice (`sound`): number keys fall on seven notes of one scale, and a result rings the last three that were typed | |
| One look in light and in dark | **Scripted**: `style` doesn't read `ctx.dark` | The page it comes from has one look. |
| A see-through window: ribbons of paint under the glass | **Scripted** (`background` layer with `fade = false`, and `style`): the paint's ramp is made with its bands' own opacities, the dark band's falling to nothing at 0.5 and the bright ones' to two fifths at 0.1 (a ramp for each 64th of opacity); `window.background` is a veil of the dark band under the paint, going from 1 to 0.7, so that the glass and what the faint text needs go from an opaque window's to a see-through one's over that range (by under 0.05 of the glass's opacity a 64th) rather than at once; the glass is as strong as its text needs over that paint and over the desktop, its hairline a little brighter as the paint clears; the figures and tiles stay solid | Faded as a whole, the near-black band laid a grey smoke over the desktop and the colour went with it. With the dark band gone first, the desktop shows clear between ribbons that keep their colour, and the glass is the same glass. |

Nothing in the app is specific to Swirls either. What it needed of the
app was one line: the graph's analysis column is a `plain` container now,
like the equations it replaces in the side panel (its text lies straight
on the page too).

Measured (release build, a 760×640 window on the private headless
compositor, the machine under a processor cap and shared with other
work; Aurora beside it in the same runs; ranges are over five runs, the
last after the merge with Sprinkles). Aurora's column is the aurora
0.3 has (three runs, with Swirls and with the Aurora it replaces in the
same session; its grid has been made coarser since, and its hook's own
figures are of that); the last column is that one, 0.2's four blobs and
two arcs, as it measured beside it:

| | Swirls | Aurora | Aurora as it was |
|---|---|---|---|
| The background's hook per frame, flowing (processor time; the app's colouring pass is inside it) | 0.9 to 1.2 ms: 0.55 the pass, 0.3 a fifth of a snapshot. 3.0 ms for the first frame at a window size (a whole snapshot) | 0.8 to 1.1 ms drifting: 0.35 the pass, the rest a quarter of a snapshot's columns and 160 stars. 2.5 to 3 ms for the first frame at a window size (a whole snapshot; 4 to 4.5 for the very first of a theme just loaded) | 0.23 ms with thirteen pulses (its gradients were rasterised by the toolkit, after the hook) |
| The same while somebody types (six stirs and a ripple alive at once) | 1.8 to 2.2 ms, 3.3 ms at worst; the limit is 8 | 0.9 ms with eight rays, a result and a new palette at once, 2.2 ms at worst | |
| The readout's hook per frame | 0.15 ms | 0.06 ms mid-result | the same |
| The keypad's, its keys coming in | 0.04 to 0.09 ms | 0.12 ms with the pointer over it | the same |
| A core, living, software renderer | 22 to 27% at 30 pictures a second (11% at 300×400, 14% at 360×600, 34% at 1280×800) | 15 to 17% at 15 (10% at 300×400, 13% at 360×600, 20% at 1280×800; 15% in the light; 19 to 20% see-through at 0.5) | 45 to 49% at 15 (44 to 63% when it was first measured) |
| A core, living, GL on llvmpipe | 82 to 93% | 80 to 82% | 140 to 143% |
| A core, typing all the while, software renderer | 33 to 41% | 65 to 67% (a key every quarter of a second: its rays are drawn thirty times a second) | 85 to 86% (its blooms every frame) |
| At rest: reduced motion (the toolkit's animations off), the window not in use, held still, settled | 0 ticks of the clock in 10 s, each | 0, each, on both renderers; and with Living background off | 0 |
| Resident memory (the base look: 69 MiB) | 78 to 82 MiB | 81 to 84 MiB living, 79 held still (in these runs: the base look 71, Swirls 84) | 79 MiB living, 77 held still |

`crates/gmnb-theme/tests/swirls.rs` prints the hook's figures
(`what_a_frame_costs`) and holds the background inside its limits at
window sizes from 300×400 to 3840×2160; `tests/aurora.rs` does the same
for Aurora.

Half of what a living Aurora cost the software renderer was neither its
blobs nor its aurora but its keys: each had a soft drop shadow, 18 px of
blur, and the sky moving behind the keys had every one of them blurred
again for every frame. With the new aurora and those shadows it took
31 to 33% of a core; without them (they could not be seen), 15 to 17%.
A theme whose background moves pays for every blur in its style once a
frame.

## Acceptance: the base look is DGMNB's

"Identical" means: **same geometry, same colours, same fonts and metrics;
only the edges of glyphs and curves may differ**, because GMNB's text is
rasterised by Pango/Cairo/GSK and DGMNB's by its own shaper.

### How it is measured

Both apps are captured by their screenshot hooks (`GMNB_SCREENSHOT`,
`DGMNB_SCREENSHOT`) on the same headless compositor: scale 1, the same
size, mode, light/dark and accent (`GMNB_ACCENT` = `DGMNB_ACCENT`), a
fresh profile, GMNB with no theme at opacity 1. The set, each in light
and dark: the seven pages (Standard, Scientific, Programmer, Date,
Graphing, Currency, Length) at 760×640; Standard at 360×600; Standard
after `1234*56=789/3=`; the sidebar open; Programmer with the bit grid;
Graphing with two equations and a trace; Date with a result.

For a pair, with `D` DGMNB's picture and `G` GMNB's:

1. **Flat pixels.** A pixel of `D` is *flat* if its 5×5 neighbourhood is
   one colour. At least **99.5%** of the flat pixels must match `G`
   within **2/255** per channel. This is the test of fills and of
   geometry: an edge that moved by 3 px or more turns flat pixels wrong.
2. **Edges.** Every other pixel of `D` outside text must find a pixel of
   `G` within 1 px of it (Chebyshev) that matches within **24/255**. At
   least **99%** must. So a border or corner may sit 1 px off and be
   anti-aliased differently, no more.
3. **Text.** For each run of text (DGMNB lists their boxes for the test):
   the box around the ink in `G` matches `D`'s within **1 px** on every
   side; the ink's mean colour matches within **6/255**; and after a 3×3
   box blur of both, the mean absolute difference over the box is at most
   **12/255**. That is the stated tolerance for glyph edges. The same
   three rules apply to the graph's curves.

Excluded from the comparison, because the structure differs by design:

- 16×16 px at each window corner (GMNB keeps the desktop's rounded
  corners and shadow; DGMNB's window is square).
- The bottom sheet's drag handle (narrow window).
- The app's name in the sidebar's header ("GMNB", "DGMNB"), and the
  header title it covers: 47,10 to 125,36 at scale 1.
- What the toolkit pops up: the unit list with its search box, the
  calendar's placement, menus, tooltips, the colour chooser.
- Settings and About (GMNB: dialogs; DGMNB: a page).
- The caret's blink and text selection.
- Text in a script none of the bundled fonts cover: each app then asks
  the system in its own way.

The comparison is a small program in `tools/theming/` that prints the
three figures per pair and fails below the thresholds. It needs a
display, so it runs with the screenshot harness, not in `cargo test`.

Tests that need no display back it up: the base colours are one function
shared by both apps; the resolved base `Style` is pinned against DGMNB's
constants (`ui.rs` text styles and radii, `HEADER_H`, `NAV_W`, …) by
importing them from a shared module, not by copying numbers.

## Acceptance: Aurora carries 0.2 on

Aurora is 0.2's look ported to a theme, and then made better: it is not a
pixel copy. It keeps 0.2's character and every one of its features (the
seven palettes, System and Freestyle, the living background, glass keys,
glows, ripples, the pointer's light, the cascade, the readout's effects,
the palette tiles, Outfit), and its author is free to refine any of them.
The 0.2.0 reference pictures are a starting point to judge it against by
eye, not a threshold. What is not optional: the [readability
rules](#readability-rules), the limits, and a still picture under reduced
motion.

One thing was not refined but made again: the aurora itself. 0.2's was
four soft blobs of colour behind frosted glass, and looked like purple
fog. 0.3's is an aurora: curtains of light over a night sky, sharp and
bright along their hems and fading upward in rays, green lowest, violet
and crimson at the crown, with stars behind them; and each palette is an
aurora of its own colours (Ember red with amber at its hem, Abyss blue,
Orchard from leaf green to blossom, Graphite with no colour in it,
System and Freestyle from their first colour at the hem to their second
at the crown). In the light it is the same aurora at dawn, on a pale
sky: a black sky under the light look's dark text and milk glass would
be the dark look.

Where 0.3's structure changed for the base look, Aurora follows it:

| What | 0.2 | Aurora on 0.3 |
|---|---|---|
| Window buttons | The toolkit's, with the desktop icon theme's glyphs | The app's own buttons with DGMNB's glyphs, styled as the toolkit's round grey ones |
| History rows | Delete by right-click only | A × appears on the row under the pointer or with keyboard focus (right-click still works) |
| Group headings in the sidebar | Stored upper-case | Shown upper-case; screen readers hear mixed case |
| Toolkit-drawn text fields | libadwaita's entry | Aurora's `field` values, matched to it by eye |
| Thresholds and sizes not in `layout` (wide at 620 px, the new-window size) | 0.2's | DGMNB's |
| The drift's phase | Starts at 0 s when animated | Starts from the still frame (37 s) |
| Readability under see-through | Judged by eye: a fixed tint under the keys, the readout and the panels, a halo around the rest | A frost of the sky's own colour under every surface that carries text, `plain` panes included, as strong as that palette needs at that opacity to pass the [see-through rule](#readability-rules) over what the window then shows, and no stronger; the graph's canvas turns almost solid |
| A see-through window | The whole background faded, aurora and all | The sky clears and the aurora stays: its lights hang over the desktop, seven tenths or more of them at any opacity, behind the frosted glass of the keys ([See-through windows](#see-through-windows)) |
| Glass in the dark | White, thin | Smoked with the sky's own colour: a key dims the aurora behind it and keeps its hue. Thin on keys; thicker on panels and under text straight on the page (the veil), where the small and the coloured text is |
| Text that steps back (captions, expressions, the number bases' names) | The text colour at about 0.6 whatever the palette: 4.1 to 4.4:1 in the light ones | The least opacity, from 0.62 up, that reaches 4.5:1 on that palette's glass over its sky and over each of its lights |
| Errors | The equals key's first colour as it came: 2:1 in the light Aurora, teal in Abyss | A rose, moved until it reaches 4.5:1 |
| A lit text button, the current number base, the operators' glyphs | The accent on a tint of itself: under 4.5:1 in the light palettes | The accent deepened until it reaches 4.5:1 on that tint |
| A key that can't be used | The whole key at 0.32 | Thinner glass and a faint glyph: on a see-through window the key keeps its frost |
| A key's drop shadow | 18 px of blur, drawn 12 px in | None. It could not be told from none in a picture, and the software renderer blurred every key's again for each frame of the drift, the sky moving behind them all: half of what a living Aurora cost it |
| The aurora | Four blobs | Three curtains of light over a night sky with stars, in the palette's six lights by height; each light as strong as the text over it lets it be |
| A pulse | One bloom | A ray of the aurora's own light rising from the key, as high as its note; the equals key's and a result's light the sky round them too; a new palette sends up four, left to right, as its chord plays |
| A toggle key that is on (2nd, hyp) | The equals key's gradient at 0.85: its small text fell to 2.5:1 in the light palettes | The gradient solid, deepened until the text reaches 4.5:1 |
| The keyboard's focus ring | The toolkit's: the accent at half opacity, 2 px inside the control, about 2:1 on the page and 2.2:1 on a key | The accent, solid, 2 px a pixel out from the control; round a key, which lies on the aurora itself, a line of the sky's colour along each side, and the accent moved 9:1 from that line, so that one of the two reaches 3:1 or more over the sky, over any light and over any desktop that shows through, at any opacity (rule 5) |

## Readability rules

Built-in themes must pass these; they are tests in `crates/gmnb-theme`
(and `appcore` for the base colours). They run the real themes in the
real engine, without a display.

**1. The base colours** (today: DGMNB's `any_accent_stays_readable`). For
no accent, `#ffff00`, `#000000`, `#ffffff`, `#808080`, `#0000ff` and
`(0.894, 0.651, 0.404)`, light and dark:
`on_accent` on `accent` ≥ 4.5; `accent_text` on `bg` and on `surface` ≥ 4.5;
`fg` on `surface` ≥ 12; every `series` colour on `bg` and on `surface` ≥ 3.

**2. Aurora's palettes** (today: `auroras_palettes_stay_readable` in
`crates/gmnb-theme/tests/css.rs`). For all seven palettes, and for System and
Freestyle generated from white, black, `#808080`, `#ffff00`, `#0000ff`,
`#00ff00` and `#7c4dff` (with the complement, and with the colour twice),
light and dark: the equals key's text on its gradient's two ends and
middle ≥ 3; the accent on the background's top and bottom colours ≥ 4.5;
every graph colour on both ≥ 3; text on both ≥ 4.5. The test calls
Aurora's own `style` and reads the resolved values.

**3. Every text on what is under it** (new). For the base look and each
built-in theme, each of its palettes, light and dark, at opacity 1: for
every pair in a fixed table of (text, the fills beneath it, bottom-most
last) the text's contrast against the composited fill is at least 4.5,
or 3 where the text is 24 px or larger (19 px if weight ≥ 700), is an
icon, a graph line, an operator key's glyph (÷ × − +: a symbol, not
text to read; DGMNB's own light palette puts them at 4.1 on `surface2`),
or a selected row's label (the sidebar's current page, the current
radix: [open point 11](#open-points)). Gradients are checked at both ends and the
middle. A translucent stack is composited over `window.background`'s
colours. Disabled text is exempt, as in WCAG. The table covers: each key
class and `keys.on`; `text_button` at rest and checked; the header title;
the readout's value and expression; history expression and result;
sidebar rows at rest and checked; `segmented` and `panel.tabs` items at
rest and checked; `field` text and placeholder (placeholder: 2.45, see
open point 9); chips
and the whimsy chip; lit bits; radix names and values; calendar days at
rest and checked; the toast; the graph's labels, axes and each series
colour on the canvas (which lies on the backdrop, not in a `plain`
container); the readout's value, its error colour and its expression on
the readout's own scrim; a card's heading. For Aurora the whole table is
`auroras_page_texts_read`, and `auroras_generated_palettes_read` runs it
again for System and Freestyle generated from rule 2's colours. For
Sprinkles it is `sprinkles_page_texts_read`, in each of its seven flavours,
and `sprinkles_own_flavours_read` runs it again for System and Freestyle
iced in rule 2's colours (with the complement, and with the colour twice);
`sweep.rs` runs it for both all round the accent wheel
(`sprinkles_under_any_accent_reads`) and for random picks
(`sprinkles_freestyle_random_colours_read`), as it does for Aurora's.
The table sees a style's fills, not what a layer draws, so a theme whose
layers paint under text, or paint the text, owes a test of its own:
Sprinkles' is `what_glazed_paints_reads` (`tests/sprinkles.rs`), which runs
the layers and holds the header's title and buttons to 4.5 on both
stripes of the awning its background layer draws, and every colour of
a glazed number, read out of the icing's picture pixel by pixel, to 3
on the readout's fill (and digits under 28 px, which are not large
text, wear no icing), in every flavour and in System and Freestyle in
colours from rule 2's and round the wheel. The
bits' indexes need 2.45, as the placeholder does (open point 9):
`the_bits_indexes_read`.

**Over a background the theme draws.** The table composites over
`window.background`. A theme whose `background` layer covers that with a
picture of its own must read over the picture: its `window.background`
names the picture's colours, and the table is run once more with every
colour the layer can draw under the window in turn. For Swirls those are
the 768 entries of its ramp (three bands and every blend between two),
taken from the ramp its background layer draws through (on a see-through
window each entry has its own opacity and lies over the veil and the
desktop, and
`swirls_reads_at_every_opacity` runs them at every twentieth from 0.1
to 1):
`swirls_texts_read_over_every_colour_of_the_field`, for each of its eight
sets of bands under each of its three strengths of glass;
`swirls_own_colours_read_over_every_colour_of_the_field` for System and
Freestyle worked out from rule 2's colours, and (a long sweep, run with
`--release -- --ignored`)
`swirls_under_any_accent_read_over_every_colour_of_the_field` from
colours all round the wheel and sixty at random; and `tests/swirls.rs`
holds its layers to that table: every pixel they draw, stirred or still,
is one of those entries, for System and Freestyle as their colours
change too (`own_colours_paint_in_their_own_ramp`). The figures its `display`
layer paints are checked the same way, on what they lie on. The plain
table for Swirls is `swirls_page_texts_read`.

For Aurora they are the sky, each of the palette's lights at its height,
and every blend of the two (its layer draws the lights through a mask,
and its pulses into the same mask; `tests/aurora.rs` holds every frame
to that, pixel by pixel:
`the_background_is_lights_and_stars_and_nothing_else`), and a star.
`auroras_texts_read_over_every_colour_of_the_sky` runs the table over
them at 17 heights and five strengths of the mask, in each of the seven
palettes and in System and Freestyle from rule 2's colours, light and
dark; see-through (rule 4) over what the window then shows, the sky
thinned over a black and a white desktop and the lights as strong as
the layer draws them there, at seven opacities for the palettes.
`tests/sweep.rs` does the same round the colour wheel and for random
pairs of colours.

**4. See-through** (new; 0.2 judged this by eye). The same table at
opacity 0.1 and 0.5, each stack composited over
`over(alpha(window.background, opacity), desktop)` for a black and for a
white desktop. Every pair must reach **3**; in the base look, `fg` pairs
must reach 4.5 (its scrim is computed to). Halos are not counted. Text
straight on a page counts wherever the style puts something under it in
`plain.background`: the base look's scrim, Aurora's frost, Sprinkles' cards
(solid at every opacity: only its wall thins).

**5. The focus ring** (new). The keyboard's focus is never hidden. For
the base look and each built-in theme, each palette, light and dark, at
opacity 1, 0.5 and 0.1 (see-through over a black and a white desktop, as
rule 4): every ring (`focus`, `date.calendar.focus`,
`converter.value.focus`) is at least 1 px wide and reaches **3** on what
it lies on. Around a key: the window's background (for Swirls each of
the 768 colours of its field), and the key's own fill where the ring is
inside it, the ring or its `edge` counting. Around a control on the
page's surfaces (`plain`, the header, the sidebar, a panel): that
surface, the ring alone. The calendar's ring: the popover, and a day. A
converter value's: its pane, and its field. A ring left to the toolkit
(`native` lists `focus`) is judged as libadwaita draws it: its accent at
half opacity, 2 px inside the control. The tests are
`every_focus_ring_reads` (Aurora's and Sprinkles' System and Freestyle
from rule 2's colours too), `every_focus_ring_reads_at_every_opacity`
(every twentieth from 0.1 to 1),
`swirls_focus_ring_reads_over_every_colour_of_the_field` (Swirls' System
and Freestyle from rule 2's colours, round the wheel and at random in the
field tests above) and `auroras_focus_ring_reads_over_every_colour_of_the_sky`
(`tests/css.rs`), and in the running app `tests/focus.rs`, which looks at
the pixels around a key. A see-through window's backdrop can be any
colour between the window's and the desktop's, so a ring round a key
there is kept [9:1 from its edge](#text-surface-control-state).

The same floor holds for every theme, not only the built-in ones, at the
window's own colours (`crates/gmnb-theme/src/focus.rs`): a ring under
1 px, or under 3:1 on what it lies on (with its edge around a key; and, for
`focus`, the header, the sidebar, a panel and `plain`, outside its control or
inside it, the ring alone), is
the base look's ring instead (2 px just outside the control, in the
theme's `accent_text`, else its `fg`, else black or white, whichever
reads), and Settings notes why. A `native` ring that doesn't reach 3:1
is the app's ring instead, held to the same floor. So `width = 0`, a
transparent colour, or the colour of the keys, can't hide where the
keyboard is (`a_ring_that_would_hide_the_focus_is_the_base_looks`).

## For implementers

### Environment hooks

| Variable | Effect |
|---|---|
| `GMNB_THEME=<id>` | Use this theme for the run without saving it. `base` is the base look. Also `--theme <id>`. |
| `GMNB_THEME_DIR=<path>` | Load a theme from this folder, as id `dev`, and use it for the run (unless `GMNB_THEME` or `--theme` names another). |
| `GMNB_THEME_OPTIONS='palette=ember;living=false'` | Option values for the run. |
| `GMNB_PALETTE=<key>` | Kept for the old scripts: the same as `GMNB_THEME=aurora` and `palette=<key>`. |
| `GMNB_ACCENT=<hex>` or `none` | What `ctx.accent` is. |
| `GMNB_STILL` | Layers see `motion = false`: no events, no frames, no `place`. The toolkit's own animations are untouched, as now. |
| `GMNB_DISPLAY_RECT` | Each readout prints where in the window it is (`display rect: x y w h`), for comparing it with DGMNB's. |
| `GMNB_ACTIVE` | The window counts as the one in use whatever the compositor says. A headless compositor never says so, and layers only animate in a window that is. |
| `GMNB_POINTER="x,y"` | A pointer position for the first keypad's layer, as now. |
| `GMNB_BENCH="themes=base,aurora,swirls,base;opacity"` | Measure: each theme switch, and an opacity drag from 1 to 0.1 and back, each to the end of the next painted frame; prints the figures and the hub's counters, then quits. |
| `GMNB_THEME_STATS` | Print the hub's counters once the window has settled, after 1.2 s or after as many milliseconds as its value: Luau machines, `style` calls, provider loads and their property counts; and for the layers the frames drawn by kind, the milliseconds their hooks and their replay took, the timers and frame callbacks ever armed and alive now, and each hook's failures; and for a theme's animations the media threads ever started (0 in the base look), the thread's wake-ups, the frames it decoded and the processor time they took; and Settings' calls into the theme (made, the passes they were made in, the longest pass in processor time), with a line `Settings has its answers` whenever it has made them all, and the processor time Settings took to open (`GMNB_PREFS=1`). |
| `GMNB_SOUND=0` | No sound for the run; nothing sound-related is created. `null`: the mixer runs with no sound server, for tests. |
| `GMNB_SOUND_DUMP=<folder>` | Every sound the theme makes is written there as a WAV file. |
| `GMNB_DATE_TO=YYYY-MM-DD` | The date page's "To" date (the comparison's "Date with a result"). |
| `GMNB_DATE_ADD` | Open the date page on "Add or subtract days". |
| `GMNB_PREFS=about` | Open About, as `GMNB_PREFS=1` opens Settings. |
| `GMNB_FOCUS=<css class>` | Give the keyboard's focus, its ring shown, to the first focusable widget with that class (`wc-conv-value`: the converter's first value; `wc-tile`: Settings' first tile), as Tab to it would, and say where it is in the window (`GMNB_FOCUS: <class> at x y w h`). |

`GMNB_DARK`, `GMNB_OPACITY`, `GMNB_SIZE`, `GMNB_MODE`, `GMNB_KEYS` and the
rest keep working.

### Where events come from

Every place 0.2 called `Ctx::pulse_at`, `Keypad::flash` or
`Keypad::cascade` is an event now; the colours and strengths that were
passed there are Aurora's business. Every event goes through one
function, `Layers::emit` (`apps/gmnb/src/layers/mod.rs`): it tells each
layer that listens, in that layer's coordinates, and then whoever asked to
hear all events (`Layers::listen`: sound). An event's `page` is read off
the widget it came from (the name its page has in the window's stack).

| Where | Call | Event |
|---|---|---|
| `calculator.rs` `on_key`, `converter.rs` keypad `connect_pressed` (a key clicked) | `key_pressed(pad, id, Some(point))` | `press`, `source = "key"`, `x, y` = the point pressed, `rect` = the key |
| `calculator.rs` `flash`, `converter.rs` key handler (a key typed) | `key_pressed(pad, id, None)` | `press`, `source = "key"`, `via = "keyboard"`, `x, y` = the key's middle. The key is also `active` for 110 ms. |
| `graphing.rs` keypad buttons | `pressed(button, Source::Key, action)` | `press`, `source = "key"`, `page = "graphing"`, `key.kind = "other"`, no `key.id`, `x, y` = the button's middle; ⌫ with `action = "backspace"` |
| `calculator.rs` `press_from` (memory row, F-E); MC, M+ and M− on a stored value | `pressed(button, Source::Button, …)` | `press`, `source = "button"`; the memory buttons with `action = "memory"` |
| `converter.rs` swap | `pressed(button, Source::Swap, None)` | `press`, `source = "swap"` |
| `date.rs` a day picked | `pressed(button, Source::Date, None)` | `press`, `source = "date"`, `rect` = the date button |
| `Display::set_value` given `Change::Result` or `Change::Error` (the calculator's, the converter's and the date page's readouts alike) | `outcome(readout, error)` | `result` / `error` with the readout's `page`, `rect` = the readout, and the `cause` and `via` of the press told less than 250 ms before it (every press is told before the calculator is pressed). Not from a readout that is not on screen (another page's; the date page's other result) |
| `Keypad` mapped; `Keypad::cascade` (the calculator's mode changed, the converter shown) | `shown(pad)` | `show`, `rect` = the keypad |
| `Hub::set_option` | `emit` | `option`, with `id` and `value` |
| `Window::set_mode`, after the first page | `mode_changed(page)` | `mode`, `page` = the new page, `rect` = the page |

- **`action`**: a key is `"backspace"` by its icon and `"clear"` by its
  label (C, CE); `Keypad::key_meaning`.
- **`via`**: a keypad notes every pointer press on it and every Space or
  Enter (capture phase, never claimed), and the backdrop does the same
  for the rest of the window, and a popover whose grab keeps them from
  the window does it for itself (`Layers::watch`: the date picker). A
  control that says it was used within a second of a pointer press on it
  was used by the pointer; else within a second of Space or Enter, by the
  keyboard; else through the accessibility interface (`"assistive"`). A
  key typed on the keyboard that presses its key on screen is always
  `"keyboard"`.

`Display::set_value`'s `Change` also becomes `frame.value.change` for
that readout, while a theme's display layer draws it. On the converter a
key typed is `typing` in the value typed into and `replace` in the other;
a unit chosen, the rates arriving and a paste are `replace` and `result`;
the swap is `result` in both (two `result` events, one from each
readout). On the date page a day picked and add/subtract switched are
`result`, the step boxes `typing`, the mode switched `replace`.

### Where the layers' code lives

| Where | What |
|---|---|
| `crates/gmnb-theme/src/draw.rs` | The `draw` table: every call recorded into a `DisplayList` of typed `Op`s, the caps, the push/save stack |
| `…/path.rs`, `…/picture.rs` | Paths (tables of numbers a Luau builder writes; read when drawn) and pictures (a file's header for its size, the noise recipe; pixels made when first drawn; an animation's timeline; pictures a theme fills) |
| `…/field.rs` | Ramps and `draw:field`: the pass (which is `gmnb-media`'s) and how often a layer's fields are coloured |
| `…/layer.rs`, `theme.rs` | The frame a hook is told, in host tables kept between frames; `Theme::draw` and `Theme::preview` |
| `…/sched.rs`, `…/events.rs` | "Asking for the next frame" as a state machine with no clock of its own; a layer's ring of events |
| `apps/gmnb/src/layers/mod.rs` | The runtime: an `Instance` per place a layer is drawn, the one function that arms a timer or a frame callback, `emit` |
| `…/layers/replay.rs` | A display list into a `GtkSnapshot`, as the draw API's table maps it; textures and layouts kept between frames; a theme's files decoded by the app's own decoders when first drawn, and its animations' player ([the media chapter](theming-media.md#for-implementers)) |
| `widgets/backdrop.rs`, `keypad.rs`, `display.rs`, `layers/tile.rs` | Where the four layers and the previews are drawn (the backdrop: `window.background` and a faded background layer in one group at the Background opacity, a layer that thins itself after it) |

A layer's hook runs inside its widget's `snapshot`, and only if the
layer needs a new picture; otherwise the picture it drew last is
replayed. None of this crate's code knows the toolkit; none of the
toolkit code knows Aurora.

### Fonts

- A theme's font files are added to the app's own font map from its
  folder (`pango_font_map_add_font_file`); a built-in theme served from
  GMNB's own copy (its folder couldn't be written) has them written to the
  cache directory first, as Outfit was in 0.2. Pango has no call that
  takes a file out of a font map again, so
  they stay known to the app until it exits; nothing uses them unless a
  style names their family.
- A font is found by the family name inside the file. The bundled Inter's
  is "Inter Variable" (`fc-scan`), not "Inter".
- If the desktop has a font installed under the same family name, which
  file wins is fontconfig's choice. For the base look that matters: a
  static Inter installed as "Inter Variable" would lose the `opsz` axis.
  The pixel comparison is therefore run with only the bundled fonts
  visible.

### What a frame costs

Measured for the language choice: 1000 frames of 300 calls into the app,
with the sandbox, memory limit and deadline on, averaged 76 µs. Aurora
makes 9 draw calls for a still background and 22 with thirteen pulses at
once, about 30 for a keypad with the pointer over it, and about 115 for
a nine-digit readout mid-animation in the dark (each digit twice, its
glow and itself, each inside a save and three transforms).

Measured in the real engine, under the real limits (`tests/draw.rs`,
medians of 200 frames): the background with thirteen pulses 0.26 ms, a
keypad with the pointer over it and a ripple 0.13 ms, the readout
mid-result 0.06 ms; the limit is 8 ms. Most of a frame is the theme
making colours and gradients: the `gmnb` library's functions therefore
run with the heap cap lifted (each builds one small value), which more
than halved the background's frame. The cap is checked as it is put
back, so it holds through them to within that one value.

### The sandbox, exactly

mlua 0.12 with the `luau` feature only (no `send`, no JIT). In this order:
create the machine with a memory limit of 8 MiB; remove `require` and
`loadstring`; register `gmnb`; call `sandbox(true)`. Load every chunk with
`ChunkMode::Text`. Run every call into the machine under an interrupt
that fails once the call's deadline has passed, and keep the deadline in
force until the call returns to Rust (a `pcall` in the theme can catch
the interrupt's error once; it must not buy more time).

Luau checks the interrupt only in its own instructions and the pattern
matcher's steps, never inside a library function's loop. So, before the
libraries are frozen: the searches and `table.move` refuse work past the
caps above; and the library functions that call a function they are
handed (`table.sort`'s comparator or `__lt`, `table.find`'s `__eq`,
`table.foreach`, `table.foreachi`, `string.format`'s `%*` and
`__tostring`) call it from Luau, so that one library function can't call
another once per element with no check between (`print` checks the
deadline itself between its arguments). `table.sort` compares strings
from Luau too: two long strings alike to their last byte cost their
length at each comparison. An error a library function
raises inside one of these names no line of its own (the traceback has
the theme's); a comparison that fails inside `table.sort`, of strings or
tables, says `library:0:`.

### Tests the engine must have

1. Every `var(--gmnb-…)` the structure stylesheet reads is emitted for
   the base look and for each built-in theme. (Parse the stylesheet for
   the names; compare with the generated provider.)
2. The structure stylesheet and the generated style load with **zero**
   parse errors, for the base look and each built-in theme, light and
   dark, opaque and see-through. (Needs the toolkit: runs with the
   screenshot harness.)
3. A theme returning `{}` resolves to exactly the base `Style`.
4. Unknown keys and wrong types warn and leave the base value; they never
   fail the theme.
5. Each limit in the table above, hit on purpose, gives the listed result
   and leaves the app in the base look or with the hook off.
6. No theme, or a theme whose hooks return `nil`: no timer and no tick
   callback is installed. The policy is tested with a clock it is handed
   (`sched.rs`: `nil` answers "arm nothing"); the app's sources are
   counted in its code (`theming_installs_no_sources`: a timer or a frame
   callback is made in one function only, from that answer); and the
   running app says how many it ever armed (`GMNB_THEME_STATS`).
7. The built-in themes type-check against `types/gmnb.d.luau`, and every
   key they return is one the definitions name.
8. The readability rules above.
9. Settings: the 0.2 migration, and each entry loading by itself.
10. Media and sound: the eleven tests in
    [the media chapter](theming-media.md#tests) (the synthesiser's test
    vectors, no sound system, the caps, nothing running at rest or in the
    base look, bad files, DGMNB linking none of it).
11. The themes folder (`crates/gmnb-theme/tests/install.rs`,
    `apps/gmnb/tests/themes_folder.rs`): the first start writes each
    built-in theme once, exactly; one deleted isn't written again; restore
    brings back only what is missing; an update replaces only a folder
    still as GMNB wrote it, and keeps one the user changed; a folder GMNB
    didn't write, or a link planted at a theme's folder or in it, is never
    written to or through; a folder that can't be written leaves GMNB's
    own copy standing in; the order of the list; the chosen theme deleted
    at start and while Settings is open; themes dragged in shown; nothing
    watching the folder once Settings is closed; and *Open themes folder*
    opening the folder in the app `mimeapps.list` names, on a private bus
    whose stand-in portal GTK would have used and isn't asked.
12. Settings' calls into a theme (`apps/gmnb/src/paced.rs`, and in the
    running app `tests/settings.rs`): a theme with 24 options of 16
    tiles whose hooks each take 6 ms opens Settings in one pass, no pass
    is much over its budget and one call, every call is made in the end
    with none failed, none is made once Settings has closed, and the
    opacity slider moved nine times in one frame calls `style` once.
13. A background layer on a see-through window (`contract.rs`,
    `a_background_layer_may_thin_itself`; in the running app
    `tests/see_through.rs`): faded with `window.background` unless its
    table says `fade = false`, then at full strength over it; the same
    picture either way on an opaque window; the key noted on any other
    layer.

### Open points

Decided here, and worth a second look by the owner:

1. **Only the window's background takes the opacity.** Keys, panels and
   cards stay opaque in the base look. Making them translucent too would
   be a base-look change, not an API change.
2. **A new window is 360×600** and may shrink to 300×400, as DGMNB.
   Saved sizes are kept.
3. **"Use the desktop's accent colour"** joins Settings (DGMNB has it).
4. **Old settings keys are kept**: written back unchanged, so going back
   to 0.2 afterwards finds the palette it had.
5. **Themes are code.** They are sandboxed, limited and can only change
   how GMNB looks, but installing one is still running someone's script.
   Settings says so beside *Open themes folder*.
6. **Operator glyphs count as symbols** in the readability rules (3:1).
   As text (4.5:1) the base look itself would fail: in the light palette
   `accent_text` on `surface2`, where the operator keys put it, is 4.1:1.
   The alternative is to move `accent_text` in `appcore`, which changes
   DGMNB's pixels.
7. **Theme fonts stay loaded** until the app exits (Pango can add a font
   file to a font map but not take it out).
8. **Media and sound** have their own list:
   [Open points](theming-media.md#open-points).
9. **A placeholder and the bits' indexes need 2.45:1**, not 3. DGMNB's
   light palette puts `fg_faint` on `surface` at 2.49:1 and on `bg` (the
   indexes) at 2.46:1, and the base look is DGMNB's. The alternative is to
   raise `fg_faint` in `appcore`, which changes DGMNB's pixels. On a
   see-through window the base look deepens the indexes toward `fg_dim`
   just enough to keep 2.45 over a black desktop (2.28 otherwise), rather
   than thicken the scrim under every `plain` container for them.
10. **The base look's text is set as DGMNB sets it** where the pages draw
   it (`widgets/fit.rs`): unhinted, each glyph at the nearest quarter
   pixel, the capitals centred on a whole pixel, a key's line about
   DGMNB's fractional cell. The toolkit's own labels were hinted (until
   point 12) and are centred by their line height; on the date page, the converter and the
   graph pad those missed the comparison's ink-colour and blur limits by
   a little. The keypads' labels (`widgets/keypad.rs`) and the readouts
   (`widgets/display.rs`) are set the same way, by the same functions.
11. **A selected row's label counts as a mark** in the readability rules
    (3:1): the sidebar's current page and the current radix are
    `accent_text` on the accent at .16, which DGMNB's own colours put at
    3.6:1 light and 4.4:1 dark. The tint shows the selection too. The
    alternative is again to move `accent_text` in `appcore`.
12. **GMNB draws text unhinted, as DGMNB**, whatever the desktop's font
    settings say ([Text in its place](#text-in-its-place)). It applies to
    every theme and to the dialogs, and to the toolkit's own labels.
13. **The built-in themes are folders in the themes folder**
    ([The themes folder](#the-themes-folder)). *Restore built-in themes*
    shows only while one of them has no folder. A theme deleted while
    Settings is closed keeps showing until Settings opens or GMNB starts
    again: nothing watches the folder at rest.
14. ***Open themes folder* in the Flatpak uses the portal's `OpenFile`,
    not `OpenDirectory`.** As of xdg-desktop-portal 1.22, `OpenDirectory`
    opens the folder that *holds* the one it is given (through
    `org.freedesktop.FileManager1.ShowItems`, the themes folder selected
    in its parent, in whichever file manager owns that name; else that
    parent with the default handler), not the folder itself. `OpenFile`
    given the folder opens it with the host's default handler for folders
    (`inode/directory`) and skips its app chooser for folders. Outside a
    sandbox GMNB asks GIO for that handler itself rather than through
    GTK's `GtkFileLauncher`: GTK 4.22 hands the folder to the portal
    whenever the session bus has one (`gdk_display_should_use_portal`:
    the portal's name can be started and its OpenURI is version 3 or
    later), sandboxed or not, and the portal then looks the handler up in
    its own environment, or asks.
15. **Settings' theme calls are paced, not capped**: a theme whose hooks
    all run just under their limits still gets every tile drawn, about
    a pass a frame, while Settings is open, rather than blanks after a
    total. Each change of `ctx` starts the tiles again (an opacity drag
    redraws a pass a frame).
16. **The focus ring's floor is judged on what the ring lies on**, not
    on the control it rings ([rule 5](#readability-rules)): a ring outside
    a key covers the page, not the key. DGMNB's own ring is `accent_text`
    touching an equals key filled with `accent`: 1.2:1 to that key (1.1
    in the dark), 4.5:1 and more to the page. Judged against its key, the
    base look would fail its own floor. A ring the colour of its key's
    fill, inside it, is caught.
17. **A ring's `edge` is drawn around a keypad's keys only**: they are
    the controls that lie straight on the window's background and
    whatever a theme draws there. Elsewhere a control lies on one of the
    style's own surfaces, where a ring of one colour can be checked (the
    toolkit draws an outline in one colour; a second line there would
    need every control's own shadows rewritten). On a see-through window
    the base look's ring has an edge of `bg`, and its colour is moved
    9:1 from it (rule 5 over a black desktop: 1.15:1 without them in the
    light palette at 0.5, and an edge alone left 2.15:1 at 0.7), which
    Sprinkles, whose ring starts from it, keeps. At opacity 1 the
    ring is DGMNB's.
