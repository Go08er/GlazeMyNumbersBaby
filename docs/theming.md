# Theming GMNB

GMNB 0.3 can be restyled by a *theme*: one Luau file that says how the
calculator looks, and can draw behind it, over its keys and in its readout.

This is the contract for theme API **1**. The first half is for people
writing a theme; from [How style reaches the app](#how-style-reaches-the-app)
on it is also the specification the app is built to.

- The matching type definitions are [`types/gmnb.d.luau`](../types/gmnb.d.luau).
- Two themes ship with the app and serve as examples:
  [`mono`](../apps/gmnb/themes/mono/theme.luau) (20 lines) and
  [`aurora`](../apps/gmnb/themes/aurora/theme.luau) (everything).

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
-- ~/.local/share/gmnb/themes/mono/theme.luau
return {
	api = 1,
	name = "Mono",
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
```

- The folder's name is the theme's **id**: lower-case letters, digits, `-`
  and `_`, at most 32 characters, starting with a letter or digit.
- Your themes live in `$XDG_DATA_HOME/gmnb/themes/<id>/`, usually
  `~/.local/share/gmnb/themes/<id>/`.
- **Flatpak:** the sandbox sets `XDG_DATA_HOME` itself, so the same folder is
  `~/.var/app/io.github.Go08er.GlazeMyNumbersBaby/data/gmnb/themes/<id>/`.
  No extra permission is needed. Settings → Theme → *Open themes folder*
  opens it either way.
- Built-in themes (`aurora`, `mono`) are compiled into the app from
  `apps/gmnb/themes/`. A folder of yours with the same id replaces the
  built-in one; Settings marks it "your copy".
- `theme.luau` is Luau source text, at most 512 KiB. Compiled bytecode is
  refused.
- A theme cannot `require` other files, and has no `loadstring`, `io`,
  `os.execute` or `debug`. The rest of Luau's standard library is there.

### The theme table

What `theme.luau` returns:

| Field | Type | Meaning |
|---|---|---|
| `api` | number, required | The API version the theme is written for: `1`. |
| `name` | string, required | Shown in Settings. |
| `description`, `author`, `version`, `license` | string | Shown in Settings. |
| `fonts` | list of file paths | Font files (TrueType or OpenType) in the theme's folder. They become available to this app only, under the family names inside them. |
| `credits` | list of `{ title, file }` | Shown in About → Legal while the theme is installed: a heading, and a text file from the theme's folder. For font licences. |
| `options` | list | What Settings offers. See [Options](#options). |
| `style` | function | `style(ctx)` → the style table. See the [reference](#reference-tokens). |
| `layers` | table | Draw hooks. See [Drawing](#drawing). |

## When a theme runs

| When | What runs | What the app keeps |
|---|---|---|
| The theme is chosen, at start, or on *Reload* | The file, once, top to bottom. Its return value is the theme. | The theme table; fonts registered; the file's locals live until the theme is unloaded. |
| Right after, and whenever `ctx` changes: light/dark, the desktop's accent, an option, the Background opacity, reduced motion, the desktop's font | `style(ctx)` | The checked result and the style generated from it. Regenerated only if the result differs. |
| A layer needs a new picture (see [Drawing](#drawing)) | That layer's `draw` | The picture, until the layer next needs one. |
| Settings shows the theme's options | `visible`, `preview`, an action's `run` | Each tile's picture until `ctx` changes. |

Rules the app keeps:

- **A theme with no draw hooks costs nothing at rest.** No timers, no
  frames. The same holds for a theme whose hooks return `nil`.
- The Background opacity slider calls `style` at most once per frame while
  it is dragged.
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
  size in points instead.

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

**FocusRing** `{ width, color, offset, radius }`: the ring around what
holds the keyboard focus. It shows for keyboard use only; a click never
draws it. `offset` is the gap to the element's edge (negative: inside).

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
| Font | `--P-font-family`, `--P-font-size`, `--P-font-weight`, `--P-font-style`, `--P-letter-spacing`, `--P-font-features`, `--P-font-variations` |
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

Aurora uses this to keep the toolkit's switches, sliders, menus and focus
ring exactly as 0.2 had them.

### See-through windows

The Background opacity setting (0.1 to 1) is the app's, not the theme's.

- The window is always a transparent surface. The backdrop widget paints
  `window.background` and then the `background` layer, both inside one
  group at `ctx.opacity`. Colours keep their balance; the compositor may
  blur what shows through.
- Everything else is drawn at full strength. A theme decides what to
  put under its text: `style(ctx)` sees `ctx.opacity`.

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
re-derive it from its own accent (as Mono does).

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
| `icons.taper` | 0 | At `size` px a line is `stroke × (taper + (1 − taper) × size / 24)` px wide. 0 scales the line with the icon; 1 keeps it the same at every size. |

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
| `layout.date.max_width` | 480 | unlimited |
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
| `plain.background` | Fill | `base.text_scrim` | `.wc-plain` *(new)*: the memory row, radix list, bit grid, programmer and scientific toolbars, the date column, the converter's value column, the graph side panel, empty-state texts |
| `plain.radius` | Radius | `shape.card_radius` | same |
| `scrim` | Color | black at 0.30 | The shade over what the sidebar or a sheet covers |
| `focus` | FocusRing | `{ width 2, color accent_text, offset 2 }` | `*:focus-visible { outline }` for every control; the ring's corners follow the control's radius |

### Header and window buttons

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `header.background` | Fill | `base.text_scrim` | `window.gmnb headerbar` |
| `header.height` | number | 46 | `headerbar { min-height }` |
| `header.title` | Text | `text.title`, `fg` | `.wc-title` |
| `header.button` | Control | `icon_button` | `headerbar .wc-icon-button`: menu, History, Keep on top, the graph's toggles |
| `header.controls` | Control | `icon_button` | `button.wc-window-button` *(new)*: minimise, maximise/restore, close. The app's own buttons with DGMNB's glyphs; the toolkit's are switched off. |
| `header.close` | Control | `header.controls` | `button.wc-window-button.wc-close` *(new)* |

### Navigation sidebar

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `nav.background` | Fill | `bg` | `.wc-sidebar` |
| `nav.shadow` | Shadow list | `{ inset, x −1, border }` (a line down the right edge) | `.wc-sidebar` |
| `nav.width` | number | 280 | The split view's sidebar width (Rust); it always leaves 40 px of the window uncovered |
| `nav.title` | Text | `text.title`, `fg` | `.wc-nav-title` *(new)*: the app's name beside the menu button |
| `nav.show_title` | boolean | `true` | Rust: the label's visibility |
| `nav.header` | Text | `text.caption`, `fg_dim`, margin `{ 8, 12, 4, 12 }` | `.wc-nav-header` |
| `nav.uppercase_headers` | boolean | `false` | Rust: a Pango text transform on the label; what a screen reader hears doesn't change |
| `nav.row` | Control | radius `shape.row_radius`; min_height 40; margin `{ 1, 6 }`; padding `{ 0, 8 }` (the icon in a 28 px box, the label 44 px in); `text.body`; `fg`; icon `{ size 18 }`; `checked = { background accent at 0.16, color accent_text }` | `.wc-nav row`; `checked` is `:selected` |
| `nav.settings` | Control | as `nav.row` | `button.wc-nav-settings` |

### Shared controls

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `icon_button` | Control | 36×36; radius 999; `fg`; icon `{ size 18 }`; `checked = { background accent at 0.18, color accent_text }` | `button.wc-icon-button` |
| `text_button` | Control | min_height 34; padding `{ 0, 6 }`; radius `shape.radius`; `text.caption`; `fg`; no background; `checked = { background accent, color on_accent }` | `button.wc-mem`: MC MR M+ M− MS M▾, DEG, F-E, the programmer bar |
| `text_button_gap` | number | 2 | The rows' spacing (Rust) |
| `flyout_button` | Control | as `text_button`, in `text.small`, min_height 30 (its row is 36 high, the button 3 px in from top and bottom), padding `{ 0, 6 }` | `button.wc-mem.wc-flyout-button`: Trigonometry ▾ and Function ▾ |
| `button` | Control | background `surface2`; radius `shape.radius`; `text.body`; `fg`; min_height 34; padding `{ 0, 6 }`; `checked = { background accent, color on_accent }` | `window.gmnb.own-button button:not([class*="wc-"])` and dialog buttons |
| `segmented.gap` | number | 4 | `toggle-group { border-spacing }` (half of it: see [what the toolkit was found to support](#what-the-toolkit-was-found-to-support)) |
| `segmented.item` | Control | min_height 36; radius `shape.radius`; `text.small`; `fg`; background `surface2`; `checked = { background accent, color on_accent }` | `window.gmnb.own-segmented toggle-group > toggle` |
| `field` | Field | background `surface`; `fg`; border `{ 1, border }`; radius `shape.row_radius`; `text.body`; padding `{ 0, 10 }`; min_height 38; placeholder `fg_faint`; selection `accent` at 0.30; caret `fg`; `focus = { border { 2, accent_text } }`; `error = { border { 2, danger } }` | `window.gmnb.own-field entry, spinbutton` |
| `row` | Control | radius `shape.row_radius`; min_height 36; `checked = { background accent at 0.16 }` | Rows of menus, dropdown lists, the bit-shift list |
| `section` | Text | `text.caption`, `fg_dim` | `.wc-section` |
| `slider.track` | `{ height, color, radius }` | 4, `border`, 2 | `window.gmnb.own-slider scale trough` |
| `slider.fill` | Color | `accent` | `scale highlight` |
| `slider.knob` | `{ size, color, halo, halo_size }` | 16, `accent`, `accent` at 0.16, 24 | `scale slider`; the halo shows on hover and press |
| `switch` | `{ width, height, off, on, knob, knob_on }` | 46, 24, `border`, `accent`, `surface`, `on_accent` | `window.gmnb.own-switch switch` |
| `scrollbar` | `{ width, color, radius }` | 3, `fg_faint`, 1.5 | `window.gmnb.own-scrollbar scrollbar slider` |
| `card` | Surface | background `surface`; radius `shape.card_radius`; no border | `window.gmnb.own-card .card, list.boxed-list` (Settings) |
| `popover` | Surface | background `surface`; border `{ 1, border }`; radius `shape.card_radius`; padding 6; shadow: three rings, `{ spread 6, black 0.05 }, { spread 3, black 0.07 }, { spread 1, black 0.10 }` | `popover.wc-flyout > contents`, the calendar popover |
| `menu` | Surface | as `popover`, radius 10 | `window.gmnb.own-menu popover:not(.wc-flyout) > contents` |
| `toast` | `{ background, color, radius, font, min_height, padding }` | `fg` at 0.92; `bg`; 19; `text.body`; 38; `{ 0, 16 }` | `window.gmnb.own-toast toast` |
| `tooltip` | `{ background, color, radius, font, padding }` | `fg` at 0.92; `bg`; `shape.row_radius`; `text.small`; `{ 4, 8 }` | `window.gmnb.own-tooltip tooltip` |

### Display (the readout)

Drawn by `widgets/display.rs` from `Style`. The same widget shows the
converter's two values and the date page's results.

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `display.background` | Fill | `base.text_scrim` | `calcdisplay` (CSS) |
| `display.radius` | Radius | `shape.card_radius` | `calcdisplay` (CSS) |
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
| `keys.pad` | `{ gap, radius, min_height, font }` | 3, `shape.radius`, 0, the key classes' own | `.wc-graph-pad button.wc-pad-key` and its grid (the graphing page's keypad) |

The key classes are Controls but have no `checked`; a toggle that is on
uses `keys.on`. A class takes `keys`' shared values (`radius`, `padding`,
`min_width`, `min_height`, `transition`, `icon`, and the three states)
as the theme set them, unless it gives its own. Superscripts and subscripts in key labels are 70% of the
label's size (fixed).

### History and memory panel

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `panel` | Surface | background `surface2`; radius `shape.card_radius`; padding 8; no border, no shadow | `.wc-side-panel` (the wide window's panel) |
| `panel.sheet` | Surface | background `surface`; border `{ 1, border }`; radius 14; the popover's three-ring shadow | `.wc-sheet` and the bottom sheet's own surface (narrow window) |
| `panel.tabs.stretch` | boolean | `true` | Rust: the switch fills the panel's width in equal halves, or hugs its labels at the start |
| `panel.tabs.gap` | number | 0 | the switch's spacing |
| `panel.tabs.item` | Control | min_height 34; radius `shape.radius`; `text.strong`; `fg`; no background; `checked = { background accent, color on_accent }` | `.wc-toggle-group.wc-panel-tabs > toggle` *(new class)* |
| `panel.title` | Text | `text.strong`, `fg` | `.wc-panel-title` (Programmer has memory only) |
| `panel.row` | Control | radius `shape.row_radius`; min_height 64 (a memory row, with its three buttons under the value: 70); padding `{ 6, 10 }` | `.wc-panel-row` |
| `panel.expression` | Text | `text.small`, `fg_dim` | `.wc-hist-expr` |
| `panel.result` | Text | 22 / 500 with `{ "tnum" }`, `fg` | `.wc-hist-result` (history results and memory values) |
| `panel.delete` | Control | 22×22; radius 999; `fg`; the icon as DGMNB draws it | `button.wc-hist-delete` *(new)*: the × on each history row |
| `panel.delete_reveal` | `"always"` or `"hover"` | `"always"` | `"hover"`: opacity 0 until the row is hovered or holds the keyboard focus. The button is always there for the keyboard and for screen readers. |
| `panel.memory_button` | Control | background `surface2`; radius `shape.radius`; `text.caption`; `fg`; min_height 26; padding `{ 0, 8 }` | `button.wc-mem-small`: MC, M+, M− on a memory row |
| `panel.memory_reveal` | `"always"` or `"hover"` | `"always"` | `.wc-mem-actions`, as `delete_reveal` |
| `panel.empty` | Text | `text.small`, `fg_dim` | `.wc-empty` |

The trash button under the list is an `icon_button`.

### Programmer

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `programmer.radix.row` | Control | radius `shape.row_radius`; min_height 26; padding `{ 0, 8 }`; `checked` changes nothing | `button.wc-radix-row`; `checked` is `.wc-radix-active` |
| `programmer.radix.name` | Text | `text.strong`, `fg_dim` | `.wc-radix-name` |
| `programmer.radix.name_checked` | Color | `accent_text` | `.wc-radix-active .wc-radix-name` |
| `programmer.radix.value` | Text | 14 / 400 with `{ "tnum" }`, `fg` | `.wc-radix-value` |
| `programmer.bit` | Control | radius `shape.radius`; 16 / 400; `fg`; no background; min 14×30; `checked = { background accent, color on_accent, font { weight 700 } }` | `button.wc-bit` |
| `programmer.bit_index` | Text | `text.caption`, `fg_faint` | `.wc-bit-index` |

Word size, Bitwise and Bit shift are `text_button`s (DGMNB sets them in
its caption style, unlike Scientific's two flyout buttons); their flyouts
are `popover`s.

### Date page

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `date.label` | Text | `text.caption`, `fg_dim` | `.wc-section` on this page |
| `date.field` | Control | min_height 38; radius `shape.radius`; background `surface2`; `text.body`; `fg`; padding `{ 0, 12 }` | `menubutton.wc-field > button` |
| `date.result_panel` | Surface | nothing: no background, no padding | `.wc-result-panel` |
| `date.result`, `date.add_result` | `{ size, font }` | 28, weight 400; the same | The page's readouts (Rust) |
| `date.subresult` | Text | `text.body`, `fg_dim` | `.wc-subresult` |
| `date.calendar.title` | Text | `text.strong`, `fg` | `.wc-days-title` |
| `date.calendar.weekday` | Text | `text.caption`, `fg_dim` | `.wc-days-weekday` |
| `date.calendar.day` | Control | fills its cell (about 38×37: seven columns and six rows in the 300×316 popover, padding 10, gap 2); radius 999; `text.small` with `{ "tnum" }`; `fg`; `checked = { background accent, color on_accent }` | `button.wc-day`; `checked` is `.wc-day-selected` |
| `date.calendar.other_month` | Color | `fg_faint` | `.wc-day-other` |
| `date.calendar.today` | Border or `false` | `{ 1, accent_text }` | `.wc-day-today` |
| `date.calendar.focus` | FocusRing | `focus` | `button.wc-day:focus` (shown on plain focus: a popover isn't told of keyboard use) |

The mode switch is a `segmented`; the day-count boxes are `field`s; the
calendar sits in a `popover`; its arrows are `icon_button`s.

### Converter

| Path | Kind | Base | Feeds |
|---|---|---|---|
| `converter.field` | Control | no background; radius 0; padding 0; `checked` changes nothing | `.wc-conv-field`; `checked` is `.wc-active` (the field being typed into) |
| `converter.marker` | `{ color, width, height }` or `false` | `{ accent, 36, 3 }` | A bar at the bottom-left of the active field: a sized background layer on `.wc-conv-field.wc-active` |
| `converter.value` | `{ size, font, checked_weight, focus }` | 38; `fonts.display` weight 300 with `{ "tnum" }`; 500; `focus` | The two readouts (Rust); the ring is `.wc-conv-field calcdisplay:focus-visible` |
| `converter.symbol` | Text | 26 / 400, `fg_dim` | `.wc-currency-symbol` |
| `converter.unit` | Control | min_height 34; radius `shape.radius`; background `surface2`; `text.body`; `fg`; `checked = { background accent, color on_accent }` | `dropdown.wc-unit-dropdown > button`; `checked` while its list is open. The list is a `menu`. |
| `converter.swap` | Control | `icon_button` | `button.wc-swap` |
| `converter.caption` | Text | `text.caption`, `fg_dim` | "About equal to" |
| `converter.chip` | Control | min_height 28; radius 14; background `surface2`; `text.small`; `fg`; padding `{ 0, 10 }` | `.wc-chip` |
| `converter.whimsy` | State | changes nothing | `.wc-chip-whimsy` (the playful comparison) |
| `converter.whimsy_icon` | boolean | `false` | Rust: whether that chip shows its sparkle |
| `converter.ratio` | Text | `text.small`, `fg` | `.wc-ratio` |
| `converter.timestamp` | Text | `text.small`, `fg_dim` | `.wc-timestamp` |
| `converter.link` | Control | `text.small`; `accent_text`; no background | `button.wc-link` |

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
| `background` | The whole window, under every widget, over `window.background` | The window | Faded to `ctx.opacity` together with `window.background`. Draw it opaque. |
| `keypad` | Each keypad, over its keys | That keypad | Called once per visible keypad. Can also move its keys (`place`). |
| `display` | Each readout | That readout | Paints the value **instead of** the app. |
| `overlay` | The whole window, over every widget | The window | Never takes clicks. Sheets and dialogs inside the window are under it; popovers and menus are separate surfaces and are not. |

```lua
layers = {
	background = {
		draw = function(draw, frame) … return nil end,
		events = { press = 1.15, result = 1.15 },  -- which events, and how long each is kept
		pointer = false,                           -- true: frame.pointer, and a redraw as it moves
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
  toolkit's frame clock running.
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
| `"result"` | A calculation produced a value in a readout. | |
| `"error"` | It produced an error. | |
| `"show"` | A keypad came into view (first shown, or the mode changed). | |
| `"option"` | The user changed one of this theme's options. | `id`, `value` |

Every event has `age`, `page` (`"calculator"`, `"converter"`, `"date"`,
`"graphing"`, `"settings"`), `rect` (the control, readout or keypad it
came from) and `x`, `y`: the exact point pressed for a pointer press on a
key, else the middle of `rect`. All in the layer's own coordinates; a
keypad layer gets only its own keypad's events.

For `press`:

| `source` | What | `key` |
|---|---|---|
| `"key"` | A keypad key (also the graphing keypad's) | `{ id, kind }`: `kind` is `"number"`, `"operator"`, `"func"`, `"equals"`, `"toggle"` or `"other"`; `id` is a number that is the same for the same key every time. Use it for variety, not meaning. |
| `"button"` | The memory row, F-E and their like | |
| `"swap"` | The converter's swap button | |
| `"date"` | A date was picked | |

`via` is `"pointer"` or `"keyboard"` (a key typed on the keyboard presses
the matching on-screen key).

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
| `draw:image(image, x, y, w, h)` | `append_scaled_texture` (trilinear) |
| `draw:tile(image, x, y, w, h)` | `push_repeat` + `append_texture` + `pop`: the image repeated at its own size |
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

Pictures: `gmnb.image("images/stars.png")` loads a PNG or JPEG from the
theme's folder (`GdkTexture`). `gmnb.noise({ size, seed, color, opacity })`
makes a square of speckles: per pixel, in row order, a 32-bit xorshift
(`s ^= s << 13; s ^= s >> 17; s ^= s << 5`, starting from `seed`, default
`0x9e3779b9`) gives `n = (s & 0xff) / 255`, and the pixel is `color` at
alpha `n × opacity`, premultiplied (`GdkMemoryTexture`, RGBA8). Both are
remembered by their arguments: calling them every frame costs nothing
after the first.

Pushes must be popped and saves restored by the time `draw` returns. The
app closes what was left open and reports it once.

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
not guess.

### The display layer

The app lays the value out: it picks the size that fits, shapes the text
in `display.value.font`, and positions each character. The hook paints.

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
when `ctx` changes.

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
  is asked again after every change.
- **`actions`** are small buttons at the end of an option's row. An
  action's `run(options, ctx)` returns the options to change:

  ```lua
  actions = { { label = "Complement",
                tooltip = "Use the colour opposite the primary on the colour wheel",
                run = function(o) return { secondary = gmnb.to_hex(gmnb.complement(o.primary)) } end } }
  ```

  The returned values are checked like any other and saved.

Changing an option saves it, calls `style` again, and sends an `option`
event to layers that listen for it.

Limits: 24 options, 16 choices each, labels up to 80 characters,
descriptions and notes up to 300.

### Where Settings puts things

Settings → Appearance, top to bottom:

1. **Mode**: Light / Dark / System (as now), and "Use the desktop's accent
   colour" (new; on by default; off makes `ctx.accent` `nil`).
2. **Theme**: a list with "GMNB" (the base look) first, then built-in
   themes, then yours; each with its `description`. Below it *Reload* and
   *Open themes folder*. A theme that failed shows its error here; so do
   its warnings (the latest 20).
3. **The chosen theme's options**, one group per `group`.
4. **Window**: Background opacity, Vulkan acceleration (as now).

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

## Limits, and what happens when a theme fails

A theme is someone else's code running inside the calculator. It gets a
sandbox with hard limits, and it can only ever affect how GMNB looks.

| Limit | Value |
|---|---|
| Memory of the theme's Luau machine | 8 MiB of the theme's own; the tables the app builds for a call (`ctx`, a frame) are counted apart, up to 1 MiB more |
| Time: loading the file | 250 ms |
| Time: `style` | 50 ms |
| Time: one `draw` call; all `place` calls of one keypad frame | 8 ms; 2 ms |
| Time: one `preview`, `visible` or action | 10 ms |
| Draw calls per `draw` | 4096 |
| Open pushes / saves at once | 32 each |
| Path segments per frame | 16 384 |
| Text per `draw:text` call; text calls per frame | 256 bytes; 64 |
| Blur radius; shadow blur | 100 px |
| Images: file; decoded size; count; total decoded | 4 MiB; 2048×2048; 16; 32 MiB |
| Noise: size; count | 512×512; 8 |
| Fonts: file; count; total | 16 MiB; 8; 32 MiB |
| Any string the theme returns or passes to the app | 4 KiB (longer: refused) |
| A `credits` text file | 64 KiB |
| Events kept per layer; longest kept | 32; 5 s |

Every entry into the theme's code runs under its time limit, the first
load included. Files (`fonts`, `credits`, `gmnb.image`) are relative
paths inside the theme's own folder; `..`, absolute paths and links that
lead outside are refused.

| What goes wrong | What the user sees |
|---|---|
| The file doesn't load: a syntax error, an error while running, the time or memory limit, no table returned, `api` missing | The base look, and a toast: "Theme ‘Name’ couldn't be loaded". Settings shows the message and the line. |
| `api` is newer than the app | The same, with "needs a newer GMNB". |
| `style` fails | The last style that worked, or the base look. A toast, once. |
| A value is of the wrong type or a key is unknown | That value's base stays. A note under the theme in Settings; no toast. |
| A number is out of range | It is clamped. A note. |
| A `draw` fails, runs out of time or exceeds a cap | Nothing is drawn for that layer that frame (the display paints the base way). After 3 failures the hook is switched off until the theme is reloaded, and a toast says so, once. |
| A `draw` takes more than four times its limit | Switched off at once (a library call the app couldn't interrupt). |
| The memory limit is hit | The theme is unloaded: the base look, and a toast. |
| A font or image can't be read or is too big | The theme loads without it; a note. |

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

## What is scripted and what is built in

How each of Aurora's effects is made:

| Effect | How | Why |
|---|---|---|
| The seven palettes; System and Freestyle generated from one or two colours | **Scripted** (`style`, with the `gmnb` colour functions) | Pure data and arithmetic. |
| Glass keys, glass panels, the equals key's glow, lit bits, the whimsy chip, the sidebar's and flyouts' tints | **Declared** (style values: gradients, shadow lists) | They are the toolkit's own box drawing. |
| Hover brightness, the press that springs back, the swap button's turn | **Declared** (states and transitions) | The toolkit animates them, and stops for reduced motion. |
| See-through tints and halos | **Declared**, switched on `ctx.opacity` | |
| Aurora blobs and their drift, vignette, pulses | **Scripted** (`background` layer) | A few gradients a frame; all of its numbers are the theme's. |
| Grain | **Scripted placement** of a **built-in** texture (`gmnb.noise`, `draw:tile`) | Filling 36 864 pixels per call is no job for a script; the recipe is specified so it never changes. |
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

There are no effect presets: nothing in the app is specific to Aurora.

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

Where 0.3's structure changed for the base look, Aurora follows it:

| What | 0.2 | Aurora on 0.3 |
|---|---|---|
| Window buttons | The toolkit's, with the desktop icon theme's glyphs | The app's own buttons with DGMNB's glyphs, styled as the toolkit's round grey ones |
| History rows | Delete by right-click only | A × appears on the row under the pointer or with keyboard focus (right-click still works) |
| Group headings in the sidebar | Stored upper-case | Shown upper-case; screen readers hear mixed case |
| Toolkit-drawn text fields | libadwaita's entry | Aurora's `field` values, matched to it by eye |
| Thresholds and sizes not in `layout` (wide at 620 px, the new-window size) | 0.2's | DGMNB's |
| The drift's phase | Starts at 0 s when animated | Starts from the still frame (37 s) |
| Readability under see-through | Judged by eye | Any tint the [see-through test](#readability-rules) fails is raised until it passes, and the change is noted in the theme |
| A toggle key that is on (2nd, hyp) | The equals key's gradient at 0.85: its small text fell to 2.5:1 in the light palettes | The gradient solid, deepened until the text reaches 4.5:1 |

## Readability rules

Built-in themes must pass these; they are tests in `crates/gmnb-theme`
(and `appcore` for the base colours). They run the real themes in the
real engine, without a display.

**1. The base colours** (today: DGMNB's `any_accent_stays_readable`). For
no accent, `#ffff00`, `#000000`, `#ffffff`, `#808080`, `#0000ff` and
`(0.894, 0.651, 0.404)`, light and dark:
`on_accent` on `accent` ≥ 4.5; `accent_text` on `bg` and on `surface` ≥ 4.5;
`fg` on `surface` ≥ 12; every `series` colour on `bg` and on `surface` ≥ 3.

**2. Aurora's palettes** (today: `built_in_palettes_stay_readable` and
`generated_palettes_stay_readable_for_extreme_colours` in
`apps/gmnb/src/theme.rs`). For all seven palettes, and for System and
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
icon, a graph line, or an operator key's glyph (÷ × − +: a symbol, not
text to read; DGMNB's own light palette puts them at 4.1 on `surface2`). Gradients are checked at both ends and the
middle. A translucent stack is composited over `window.background`'s
colours. Disabled text is exempt, as in WCAG. The table covers: each key
class and `keys.on`; `text_button` at rest and checked; the header title;
the readout's value and expression; history expression and result;
sidebar rows at rest and checked; `segmented` and `panel.tabs` items at
rest and checked; `field` text and placeholder (placeholder: 3); chips
and the whimsy chip; lit bits; radix names and values; calendar days at
rest and checked; the toast; the graph's labels, axes and each series
colour on the canvas.

**4. See-through** (new; 0.2 judged this by eye). The same table at
opacity 0.1 and 0.5, each stack composited over
`over(alpha(window.background, opacity), desktop)` for a black and for a
white desktop. Every pair must reach **3**; in the base look, `fg` pairs
must reach 4.5 (its scrim is computed to). Halos are not counted.

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
| `GMNB_POINTER="x,y"` | A pointer position for the first keypad's layer, as now. |
| `GMNB_BENCH="themes=base,aurora,mono,base;opacity"` | Measure: each theme switch, and an opacity drag from 1 to 0.1 and back, each to the end of the next painted frame; prints the figures and the hub's counters, then quits. |
| `GMNB_THEME_STATS` | Print the hub's counters once the window has settled (Luau machines, `style` calls, provider loads and their property counts). |

`GMNB_DARK`, `GMNB_OPACITY`, `GMNB_SIZE`, `GMNB_MODE`, `GMNB_KEYS` and the
rest keep working.

### Where events come from

Every place 0.2 called `Ctx::pulse_at`, `Keypad::flash` or
`Keypad::cascade` becomes an event; the colours and strengths that were
passed there are now Aurora's business.

| In 0.2 | Event |
|---|---|
| `calculator.rs` `on_key`, `converter.rs` keypad `connect_pressed` (a key clicked) | `press`, `source = "key"`, `via = "pointer"`, `x, y` = the point pressed, `rect` = the key |
| `calculator.rs` `flash`, `converter.rs` key handler (a key typed) | `press`, `source = "key"`, `via = "keyboard"`, `x, y` = the key's middle. The key is also `active` for 110 ms. |
| `graphing.rs` keypad buttons | `press`, `source = "key"`, `page = "graphing"`, `key.kind = "other"`, `x, y` = the button's middle |
| `calculator.rs` `press_from` (memory row, F-E) | `press`, `source = "button"` |
| `converter.rs` swap | `press`, `source = "swap"` |
| `date.rs` a day picked | `press`, `source = "date"`, `rect` = the date button |
| `calculator.rs` `sync` with `Change::Result` / `Change::Error` | `result` / `error`, `rect` = the readout |
| Converter and date values changing with `Change::Result` | `result` with their `page` (0.2 drew nothing for these; Aurora ignores them) |
| `Keypad` mapped; the calculator's mode changed | `show`, `rect` = the keypad |
| `prefs.rs` palette chosen | `option`, `id = "palette"`; in general any option change |

`Display::set_value`'s `Change` becomes `frame.value.change` for that
readout; it is not an event.

### Fonts

- Built-in themes' font files are written to the cache directory and
  added to the app's own font map, as Outfit is in 0.2
  (`pango_font_map_add_font_file`); a user theme's are added from its
  folder. Pango has no call that takes a file out of a font map again, so
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
makes 7 draw calls for a still background and 22 with thirteen pulses at
once, about 30 for a keypad with the pointer over it, and at most 72 for
a nine-digit readout mid-animation.

### The sandbox, exactly

mlua 0.12 with the `luau` feature only (no `send`, no JIT). In this order:
create the machine with a memory limit of 8 MiB; remove `require` and
`loadstring`; register `gmnb`; call `sandbox(true)`. Load every chunk with
`ChunkMode::Text`. Run every call into the machine under an interrupt
that fails once the call's deadline has passed, and keep the deadline in
force until the call returns to Rust (a `pcall` in the theme can catch
the interrupt's error once; it must not buy more time).

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
   callback is installed (count the sources).
7. Both built-in themes type-check against `types/gmnb.d.luau`, and every
   key they return is one the definitions name.
8. The readability rules above.
9. Settings: the 0.2 migration, and each entry loading by itself.

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
