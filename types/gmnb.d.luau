-- GMNB theme API, version 1: type definitions for luau-lsp.
--
-- Editor setup (VS Code, luau-lsp):
--   "luau-lsp.types.definitionFiles": ["path/to/gmnb.d.luau"],
--   "luau-lsp.platform.type": "standard"
--
-- The contract these types belong to is docs/theming.md, with
-- docs/theming-media.md for pictures that move and for sound. Where they
-- and this file disagree, the documents win and this file has a bug.

---------------------------------------------------------------------------
-- Values
---------------------------------------------------------------------------

-- A colour as the gmnb helpers return it: red, green, blue and opacity, 0 to 1.
export type Rgba = { r: number, g: number, b: number, a: number? }

-- "#rrggbb", "#rrggbbaa", or an Rgba.
export type Color = string | Rgba

-- { x, y }
export type Point = { number }

-- { x, y, w, h } by name.
export type Rect = { x: number, y: number, w: number, h: number }

-- Made by gmnb.linear / gmnb.radial / gmnb.conic.
--   angle, at     relative to the box being painted; work everywhere.
--   from, to, center, radius in pixels: draw hooks only.
export type Gradient = {
	kind: "linear" | "radial" | "conic",
	colors: { Rgba },
	-- Where each colour sits, 0 to 1.
	stops: { number },
	-- linear: CSS degrees (0 up, 90 right, 180 down). conic: where it starts.
	angle: number?,
	-- linear, pixels.
	from: Point?,
	to: Point?,
	-- radial and conic: the centre as fractions of the box (0.5, 0.5 = middle).
	at: Point?,
	-- radial and conic: the centre in pixels.
	center: Point?,
	-- radial: pixels; in a style also "closest-side", "farthest-side",
	-- "closest-corner" or "farthest-corner".
	radius: (number | string)?,
	-- radial: the vertical radius when it differs (an ellipse).
	radius_y: number?,
}

export type Paint = Color | Gradient

-- One paint, or layers of them, topmost first.
export type Fill = Paint | { Paint }

export type Shadow = {
	x: number?,
	y: number?,
	blur: number?,
	spread: number?,
	color: Color,
	inset: boolean?,
}

-- Drawn inside the element's edge; never changes its size.
export type Border = { width: number, color: Color }

-- Pixels; 999 or more is fully round. Or four corners: top-left, top-right,
-- bottom-right, bottom-left.
export type Radius = number | { number }

-- Pixels: one value, { vertical, horizontal } or { top, right, bottom, left }.
export type Spacing = number | { number }

export type Font = {
	-- A family name, or several to fall back through. Names only: the font
	-- files come from the theme's `fonts` list or the system.
	family: (string | { string })?,
	-- Pixels.
	size: number?,
	-- 1..1000.
	weight: number?,
	italic: boolean?,
	-- Letter spacing in em.
	spacing: number?,
	-- OpenType features: { "tnum", "lnum" }, or "ss01=0" to turn one off.
	features: { string }?,
	-- Variable-font axes: { opsz = 24 }.
	variations: { [string]: number }?,
}

export type Ease = "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | "spring" | { number }

-- Milliseconds, or milliseconds with an ease (which defaults to "ease-out").
export type Duration = number | { ms: number, ease: Ease? }

-- Which properties ease when a state changes, and how long for.
export type Transition = {
	background: Duration?,
	color: Duration?,
	shadow: Duration?,
	transform: Duration?,
	filter: Duration?,
	opacity: Duration?,
}

-- The ring around what holds the keyboard focus.
export type FocusRing = {
	width: number?,
	color: Color?,
	-- Pixels between the element's edge and the ring (negative: inside).
	offset: number?,
	radius: number?,
}

export type Icon = {
	-- Pixels (the icons are drawn on a 24-unit grid and scaled to this).
	size: number?,
	-- Line width in pixels at that size.
	stroke: number?,
	color: Color?,
}

-- How an element differs while hovered, pressed, checked, disabled...
-- Anything left out stays as the element is at rest.
export type State = {
	background: Fill?,
	-- A colour laid over the background (how the base look shows hover and press).
	overlay: Color?,
	color: Color?,
	-- The text and icon at this opacity, the background as it was (how
	-- the base look shows a disabled key).
	fade: number?,
	border: (Border | false)?,
	shadow: { Shadow }?,
	opacity: number?,
	-- 1 = unchanged.
	brightness: number?,
	saturation: number?,
	scale: number?,
	-- Degrees.
	rotate: number?,
	-- Only `weight` is read.
	font: Font?,
	icon: Icon?,
	transition: Transition?,
}

export type Text = {
	color: Color?,
	font: Font?,
	-- A text shadow list (halos).
	shadow: { Shadow }?,
	margin: Spacing?,
}

export type Surface = {
	background: Fill?,
	border: (Border | false)?,
	shadow: { Shadow }?,
	radius: Radius?,
	padding: Spacing?,
	margin: Spacing?,
}

-- Something the user can press. Which states an element has is listed in
-- docs/theming.md; a state it doesn't have is reported and ignored.
export type Control = {
	background: Fill?,
	color: Color?,
	border: (Border | false)?,
	shadow: { Shadow }?,
	radius: Radius?,
	font: Font?,
	padding: Spacing?,
	margin: Spacing?,
	min_width: number?,
	min_height: number?,
	opacity: number?,
	icon: Icon?,
	transition: Transition?,
	hover: State?,
	active: State?,
	checked: State?,
	disabled: State?,
}

---------------------------------------------------------------------------
-- Tokens: the few values most of the look derives from
---------------------------------------------------------------------------

export type Colors = {
	-- The window.
	bg: Color?,
	-- Raised: number keys, fields, cards, popovers, the graph.
	surface: Color?,
	-- Function keys, the history panel, filled buttons, chips.
	surface2: Color?,
	border: Color?,
	fg: Color?,
	-- Secondary text.
	fg_dim: Color?,
	-- Disabled text, scrollbars.
	fg_faint: Color?,
	-- Fills: the equals key, what is switched on.
	accent: Color?,
	-- Text on `accent`.
	on_accent: Color?,
	-- The accent as text and lines on `bg` and `surface`.
	accent_text: Color?,
	-- Laid over a control under the pointer / being pressed.
	hover: Color?,
	press: Color?,
	danger: Color?,
	-- Six graph colours.
	series: ({ Rgba } | { string })?,
}

-- Font families: one name, or several to fall back through.
export type Fonts = {
	-- Everything but the readout.
	ui: (string | { string })?,
	-- The calculator's readout, and the converter's and date page's values.
	display: (string | { string })?,
}

-- The six sizes of text the base look is set in. Their family is
-- `fonts.ui` unless they name one.
export type TextStyles = {
	body: Font?,
	strong: Font?,
	small: Font?,
	caption: Font?,
	key: Font?,
	title: Font?,
}

export type Shape = {
	-- Keys, buttons.
	radius: number?,
	-- List rows, text fields.
	row_radius: number?,
	-- Panels, cards, popovers, the graph.
	card_radius: number?,
}

-- How thick the line icons are drawn: `stroke` on their 24-unit grid. At
-- `size` pixels the line is stroke * (taper + (1 - taper) * size / 24)
-- pixels wide: taper 0 scales it with the icon, 1 keeps it constant.
export type IconLines = { stroke: number?, taper: number? }

-- The space around and between a page's parts, in pixels.
export type Layout = {
	calculator: { margin: Spacing?, gap: number?, panel_width: number?, panel_margin: Spacing? }?,
	converter: { margin: Spacing?, gap: number?, gap_wide: number?, keypad_width: number? }?,
	date: { margin: Spacing?, gap: number?, max_width: number? }?,
	graph: { margin: Spacing?, gap: number?, side_width: number?, pad_height: number? }?,
}

-- Milliseconds; 0 = no animation. The base look has them all at 0.
export type Motion = {
	-- Switching pages (Standard to Graphing...).
	page: number?,
	-- Switching calculator modes.
	mode: number?,
	-- History / Memory.
	panel: number?,
	-- Programmer: keypad / bits.
	flip: number?,
	-- Graphing, narrow: equations / graph; and the side panel's own pages.
	graph_view: number?,
	graph_side: number?,
	-- Date: difference / add-subtract.
	date: number?,
	-- A new history row sliding in; an equation row appearing.
	history_row: number?,
	equation_row: number?,
	-- A navigation icon drawing itself when its row is picked.
	icon_draw: number?,
	-- A curve drawing itself; the graph gliding to a new zoom.
	graph_draw: number?,
	graph_zoom: number?,
}

---------------------------------------------------------------------------
-- The style tree: what `style(ctx)` may return. Everything is optional;
-- what is left out keeps its base value.
---------------------------------------------------------------------------

export type WindowStyle = {
	-- Painted under everything and faded by the Background opacity setting.
	background: Fill?,
	color: Color?,
	-- The font of everything not given one below.
	font: Font?,
	-- Text shadow for text drawn straight on the backdrop (title, labels).
	text_halo: { Shadow }?,
	-- The same for text on keys, buttons, fields, panels, cards, popovers
	-- and the sidebar. Disabled text never has a halo.
	raised_text_halo: { Shadow }?,
}

export type HeaderStyle = {
	background: Fill?,
	height: number?,
	title: Text?,
	-- Menu, History, Keep on top and the pages' own header buttons.
	button: Control?,
	-- Minimise, maximise, close.
	controls: Control?,
	-- Close only, where it differs (hover and active).
	close: Control?,
}

export type NavStyle = {
	background: Fill?,
	shadow: { Shadow }?,
	-- The sidebar's width (it never takes more than 85% of the window).
	width: number?,
	-- The app name beside the menu button.
	title: Text?,
	show_title: boolean?,
	-- "Calculator", "Converter".
	header: Text?,
	uppercase_headers: boolean?,
	-- `checked` is the page being shown.
	row: Control?,
	settings: Control?,
}

export type DisplaySizes = {
	standard: number?,
	scientific: number?,
	programmer: number?,
	compact: number?,
}

export type Glow = {
	color: Color,
	-- Blur radius as a fraction of the font size.
	blur: number,
}

export type DisplayStyle = {
	background: Fill?,
	radius: Radius?,
	-- Space left and right of the text.
	padding_x: number?,
	-- The readout's height per mode; "auto" sizes it to its text.
	heights: { standard: (number | "auto")?, scientific: (number | "auto")?, programmer: (number | "auto")?, compact: (number | "auto")? }?,
	-- The value's largest font size per mode (it shrinks to fit).
	sizes: DisplaySizes?,
	min_size: number?,
	value: {
		font: Font?,
		color: Paint?,
		-- When the display shows an error.
		error_color: Paint?,
		error_scale: number?,
		glow: (Glow | false)?,
	}?,
	expression: Text?,
	-- An expression too long for its line shrinks down to this size before
	-- it is cut short at its start.
	expression_min_size: number?,
	-- Shadows around the whole readout (see-through windows).
	halo: { Shadow }?,
}

export type KeysStyle = {
	gap: number?,
	gap_compact: number?,
	radius: Radius?,
	min_width: number?,
	min_height: number?,
	padding: Spacing?,
	transition: Transition?,
	icon: Icon?,
	number: Control?,
	-- "function" is a Luau keyword, hence the short name.
	func: Control?,
	operator: Control?,
	equals: Control?,
	-- A toggle key that is on (2nd, hyp...).
	on: State?,
	-- What every key adds while hovered, pressed or disabled, unless its
	-- own class says otherwise.
	hover: State?,
	active: State?,
	disabled: State?,
	-- The graphing page's small keypad.
	pad: { gap: number?, radius: Radius?, min_height: number?, font: Font? }?,
	-- A keypad whose keys would be less than `below` pixels high, laid out
	-- the usual gap apart, is short and takes these instead: its own gap,
	-- its keys' icons, each class, and a toggle that is on. Each is what
	-- it stands for unless given. `below` 0, the base look's: never.
	short: {
		below: number?,
		gap: number?,
		icon: Icon?,
		number: Control?,
		func: Control?,
		operator: Control?,
		equals: Control?,
		on: State?,
	}?,
}

export type Reveal = "always" | "hover"

export type PanelStyle = {
	background: Fill?,
	border: (Border | false)?,
	shadow: { Shadow }?,
	radius: Radius?,
	padding: Spacing?,
	margin: Spacing?,
	-- The narrow window's bottom sheet.
	sheet: Surface?,
	-- History / Memory.
	tabs: { stretch: boolean?, gap: number?, item: Control? }?,
	title: Text?,
	row: Control?,
	expression: Text?,
	result: Text?,
	-- The x on a history row. "hover": shown while its row is hovered or
	-- holds the keyboard focus.
	delete: Control?,
	delete_reveal: Reveal?,
	-- MC, M+, M- on a memory row.
	memory_button: Control?,
	memory_reveal: Reveal?,
	empty: Text?,
}

export type ProgrammerStyle = {
	-- HEX, DEC, OCT, BIN: `checked` is the base in use.
	radix: { row: Control?, name: Text?, name_checked: Color?, value: Text? }?,
	bit: Control?,
	bit_index: Text?,
}

export type DateStyle = {
	label: Text?,
	field: Control?,
	-- The frame around a result (none in the base look).
	result_panel: Surface?,
	result: { size: number?, font: Font? }?,
	add_result: { size: number?, font: Font? }?,
	subresult: Text?,
	calendar: {
		title: Text?,
		weekday: Text?,
		-- `checked` is the chosen day.
		day: Control?,
		other_month: Color?,
		today: (Border | false)?,
		-- The ring on the day the arrow keys are on.
		focus: FocusRing?,
	}?,
}

export type ConverterStyle = {
	-- `checked` is the field being typed into.
	field: Control?,
	-- A bar marking the field being typed into.
	marker: ({ color: Color, width: number, height: number } | false)?,
	value: { size: number?, font: Font?, checked_weight: number?, focus: FocusRing? }?,
	symbol: Text?,
	unit: Control?,
	swap: Control?,
	caption: Text?,
	chip: Control?,
	-- The playful comparison chip: how it differs from `chip`.
	whimsy: State?,
	whimsy_icon: boolean?,
	ratio: Text?,
	timestamp: Text?,
	link: Control?,
}

export type CanvasStyle = {
	background: Paint?,
	radius: number?,
	border: (Border | false)?,
	grid_minor: Color?,
	grid_major: Color?,
	axis: { color: Color?, width: number? }?,
	label: Text?,
	-- Shaded regions (inequalities): the curve's colour at this opacity.
	fill_opacity: number?,
	-- A blurred copy under each curve: `width` times the line's width.
	glow: ({ blur: number, width: number, opacity: number } | false)?,
	trace: {
		-- The cross-hair: the curve's colour at this opacity, dashed if
		-- `hair_dash` gives the lengths of dash and gap.
		hair_opacity: number?,
		hair_dash: ({ number } | false)?,
		-- A disc behind the dot, `grow` pixels larger: a colour, or
		-- "series" for the curve's colour at `opacity`.
		halo: ({ grow: number, color: Color | "series", opacity: number? } | false)?,
		ring: (Border | false)?,
		bubble: { background: Color?, color: Color?, radius: number?, font: Font?, border: (Border | false)?, shadow: { Shadow }? }?,
	}?,
}

export type GraphStyle = {
	side: Surface?,
	-- The line above the keypad.
	divider: (Color | false)?,
	row: Surface?,
	row_error: State?,
	-- The equation's text field: how it differs from `field`.
	entry: FieldStyle?,
	-- Style, analyse and remove.
	row_button: Control?,
	error: Text?,
	swatch: {
		size: number?,
		hidden_opacity: number?,
		-- A glow in the equation's own colour.
		glow: ({ blur: number, spread: number } | false)?,
		active_scale: number?,
		transition: Transition?,
	}?,
	add: Control?,
	variable: Text?,
	analysis: { title: Text?, card: Surface?, card_title: Text?, value: Text?, text: Text?, note: Text? }?,
	toolbar: Surface?,
	toolbar_button: Control?,
	canvas: CanvasStyle?,
}

export type SliderStyle = {
	track: { height: number?, color: Color?, radius: number? }?,
	fill: Color?,
	knob: { size: number?, color: Color?, halo: Color?, halo_size: number? }?,
}

export type SwitchStyle = {
	width: number?,
	height: number?,
	off: Color?,
	on: Color?,
	knob: Color?,
	knob_on: Color?,
}

export type FieldStyle = {
	background: Fill?,
	color: Color?,
	border: (Border | false)?,
	shadow: { Shadow }?,
	radius: Radius?,
	font: Font?,
	padding: Spacing?,
	min_height: number?,
	placeholder: Color?,
	selection: Color?,
	caret: Color?,
	-- While it has the keyboard focus / holds something invalid.
	focus: State?,
	error: State?,
}

-- Colours of stock widgets the app doesn't draw itself (the Settings and
-- About dialogs, menus, dropdown lists, the colour chooser).
export type StockColors = {
	window: Color?,
	view: Color?,
	headerbar: Color?,
	sidebar: Color?,
	dialog: Color?,
	popover: Color?,
	card: Color?,
	fg: Color?,
	accent: Color?,
	accent_bg: Color?,
	accent_fg: Color?,
	destructive: Color?,
	error: Color?,
}

export type Style = {
	colors: (Colors | BaseColors)?,
	fonts: Fonts?,
	text: TextStyles?,
	shape: Shape?,
	icons: IconLines?,
	motion: Motion?,
	layout: Layout?,
	-- Widget families left to the toolkit's own drawing, in the `stock`
	-- colours: "segmented", "field", "slider", "switch", "scrollbar",
	-- "menu", "toast", "tooltip", "button", "card", "focus". Of their style
	-- only `font` and `radius` still apply.
	native: { string }?,

	window: WindowStyle?,
	-- Containers whose text sits straight on the backdrop.
	plain: { background: Fill?, radius: Radius? }?,
	header: HeaderStyle?,
	nav: NavStyle?,
	display: DisplayStyle?,
	keys: KeysStyle?,
	panel: PanelStyle?,
	programmer: ProgrammerStyle?,
	date: DateStyle?,
	converter: ConverterStyle?,
	graph: GraphStyle?,

	-- Shared controls.
	icon_button: Control?,
	-- MC MR M+ M- MS, DEG, F-E, the programmer toolbar.
	text_button: Control?,
	text_button_gap: number?,
	-- "Trigonometry" and "Function": buttons that open a flyout of keys.
	flyout_button: Control?,
	-- Filled buttons.
	button: Control?,
	-- Two- and three-way switches; `checked` is the choice.
	segmented: { gap: number?, item: Control? }?,
	field: FieldStyle?,
	-- Rows of menus and lists; `checked` is the selected row.
	row: Control?,
	section: Text?,
	slider: SliderStyle?,
	switch: SwitchStyle?,
	scrollbar: { width: number?, color: Color?, radius: number?, inset: number? }?,
	card: Surface?,
	-- Flyouts: the keys' own popovers, the calendar, the graph's options.
	popover: Surface?,
	-- Menus and dropdown lists.
	menu: Surface?,
	toast: { background: Fill?, color: Color?, radius: Radius?, font: Font?, min_height: number?, padding: Spacing? }?,
	tooltip: { background: Fill?, color: Color?, radius: Radius?, font: Font?, padding: Spacing? }?,
	focus: FocusRing?,
	-- The shade over what a sheet or the sidebar covers.
	scrim: Color?,
	stock: StockColors?,
}

---------------------------------------------------------------------------
-- ctx: what a theme is told
---------------------------------------------------------------------------

export type OptionValue = string | number | boolean

export type Ctx = {
	dark: boolean,
	-- The desktop's accent colour: nil if it shares none, or if the user
	-- turned "Use the desktop's accent colour" off.
	accent: Rgba?,
	-- The Background opacity setting, 0.1 to 1. Below 1 the desktop shows
	-- through the window's background.
	opacity: number,
	-- This theme's options by id (saved values, else their defaults).
	options: { [string]: OptionValue },
	-- False when the desktop asks for reduced motion.
	motion: boolean,
	-- The desktop's interface font.
	font: { family: string, size: number },
	-- The base look's tokens for this `dark`, `accent` and `opacity`.
	-- `text_scrim` is the layer of `bg` the base look puts under text on a
	-- see-through window (transparent at full opacity).
	base: { colors: BaseColors, text: BaseText, shape: Shape, text_scrim: Rgba },
}

-- The base tokens, all present.
export type BaseColors = {
	bg: Rgba,
	surface: Rgba,
	surface2: Rgba,
	border: Rgba,
	fg: Rgba,
	fg_dim: Rgba,
	fg_faint: Rgba,
	accent: Rgba,
	on_accent: Rgba,
	accent_text: Rgba,
	hover: Rgba,
	press: Rgba,
	danger: Rgba,
	series: { Rgba },
}

export type BaseText = { body: Font, strong: Font, small: Font, caption: Font, key: Font, title: Font }

---------------------------------------------------------------------------
-- Drawing
---------------------------------------------------------------------------

export type Stroke = number | {
	width: number,
	cap: ("butt" | "round" | "square")?,
	join: ("miter" | "round" | "bevel")?,
	dash: { number }?,
	dash_offset: number?,
}

export type Path = {
	move_to: (self: Path, x: number, y: number) -> Path,
	line_to: (self: Path, x: number, y: number) -> Path,
	quad_to: (self: Path, cx: number, cy: number, x: number, y: number) -> Path,
	curve_to: (self: Path, c1x: number, c1y: number, c2x: number, c2y: number, x: number, y: number) -> Path,
	close: (self: Path) -> Path,
	rect: (self: Path, x: number, y: number, w: number, h: number) -> Path,
	rounded_rect: (self: Path, x: number, y: number, w: number, h: number, radius: Radius) -> Path,
	circle: (self: Path, cx: number, cy: number, r: number) -> Path,
}

-- A picture from the theme's images folder, or generated noise.
export type Image = { width: number, height: number }

-- An animated picture (GIF, APNG, animated WebP). It always loops.
export type Animation = {
	width: number,
	height: number,
	-- How many frames, and how long one pass takes, in seconds.
	frames: number,
	duration: number,
}

-- A picture the theme fills from a buffer.
export type Pixels = {
	width: number,
	height: number,
	-- Copies width × height × 4 bytes: each pixel red, green, blue,
	-- opacity (not premultiplied), rows from the top. As one number with
	-- buffer.writeu32: 0xAABBGGRR. Returns false, changing nothing, if
	-- the picture was changed too recently (30 times a second at most);
	-- the layer being drawn is then drawn once more when it can be.
	set: (self: Pixels, data: buffer) -> boolean,
}

export type Picture = Image | Animation | Pixels

export type ImageOptions = {
	-- "trilinear" (the default for stills): smooth, and clean when drawn
	-- much smaller. "linear" (the default for animations and Pixels):
	-- smooth. "nearest": hard pixels.
	filter: ("trilinear" | "linear" | "nearest")?,
	-- Animations: where in the animation, in seconds. Left out, the
	-- layer's clock, and the app schedules the next frame itself. Under
	-- reduced motion and in a preview the first frame is shown whatever
	-- this says.
	time: number?,
	-- Animations, with `time`: wrap around past the end (the default) or
	-- hold the last frame.
	loop: boolean?,
}

-- Made by gmnb.ramp: the colours a field is drawn in.
export type Ramp = { colors: { Rgba }, edge: { number }, cycle: boolean }

export type RampSpec = {
	-- 2 to 16.
	colors: { Color },
	-- Where in a band (0 to 1) the blend to the next colour starts and
	-- ends. { 0, 1 } by default; two equal numbers give a hard edge.
	edge: { number }?,
	-- True: after the last colour comes the first again (repeating
	-- bands). False (the default): a gradient map from the first colour
	-- at position 0 to the last at position #colors - 1.
	cycle: boolean?,
}

-- A coarse grid of numbers, coloured per pixel by the app.
export type FieldSpec = {
	-- cols × rows × channels 32-bit floats (buffer.writef32), rows from
	-- the top. The grid's corners sit on the rectangle's corners.
	data: buffer,
	cols: number,
	rows: number,
	-- 1: a point's value is the number. 2: the length of the pair.
	channels: number,
	ramp: Ramp,
	-- position = value × scale + offset. Defaults 1 and 0.
	scale: number?,
	offset: number?,
	-- A second snapshot of the same size, and how far towards it (0 to 1)
	-- to glide. Done on the numbers, before the colours.
	next: buffer?,
	mix: number?,
	-- Computed pixels per pixel of the rectangle along each side, 0.1 to
	-- 1. Default 0.5. The app may use less.
	quality: number?,
}

-- One character of the display's value, laid out by the app. A glyph of
-- the value before a change (frame.value.previous) carries previous = true.
export type Glyph = { index: number, x: number, y: number, w: number, h: number, previous: boolean? }

export type Draw = {
	-- Shapes.
	rect: (self: Draw, x: number, y: number, w: number, h: number, paint: Paint) -> (),
	rounded_rect: (self: Draw, x: number, y: number, w: number, h: number, radius: Radius, paint: Paint) -> (),
	circle: (self: Draw, cx: number, cy: number, r: number, paint: Paint) -> (),
	fill: (self: Draw, path: Path, paint: Paint, rule: ("winding" | "even-odd")?) -> (),
	stroke: (self: Draw, path: Path, stroke: Stroke, paint: Paint) -> (),
	border: (self: Draw, x: number, y: number, w: number, h: number, radius: Radius, width: number, color: Color) -> (),
	shadow: (self: Draw, x: number, y: number, w: number, h: number, radius: Radius, shadow: Shadow) -> (),
	-- Text and pictures.
	-- The font is laid onto the style's body text; nil is that font.
	text: (self: Draw, text: string, x: number, y: number, font: Font?, paint: Paint) -> (),
	text_size: (self: Draw, text: string, font: Font?) -> (number, number),
	glyph: (self: Draw, glyph: Glyph, paint: Paint) -> (),
	image: (self: Draw, image: Picture, x: number, y: number, w: number, h: number, options: ImageOptions?) -> (),
	tile: (self: Draw, image: Picture, x: number, y: number, w: number, h: number) -> (),
	field: (self: Draw, x: number, y: number, w: number, h: number, spec: FieldSpec) -> (),
	-- Groups: everything drawn until the matching pop() is affected.
	push_clip: (self: Draw, x: number, y: number, w: number, h: number, radius: Radius?) -> (),
	push_opacity: (self: Draw, opacity: number) -> (),
	push_blur: (self: Draw, radius: number) -> (),
	push_shadow: (self: Draw, shadows: { Shadow }) -> (),
	-- Two parts, each ended by pop(): first the mask, then what shows through it.
	push_mask: (self: Draw, mode: ("alpha" | "inverted-alpha" | "luminance" | "inverted-luminance")?) -> (),
	-- Two parts, each ended by pop(): first what is underneath, then what is blended onto it.
	push_blend: (self: Draw, mode: "multiply" | "screen" | "overlay" | "darken" | "lighten" | "color-dodge" | "color-burn" | "hard-light" | "soft-light" | "difference" | "exclusion" | "color" | "hue" | "saturation" | "luminosity") -> (),
	pop: (self: Draw) -> (),
	-- Transforms.
	save: (self: Draw) -> (),
	restore: (self: Draw) -> (),
	translate: (self: Draw, dx: number, dy: number) -> (),
	scale: (self: Draw, sx: number, sy: number?) -> (),
	rotate: (self: Draw, degrees: number) -> (),
}

---------------------------------------------------------------------------
-- Frames and events
---------------------------------------------------------------------------

export type KeyKind = "number" | "operator" | "func" | "equals" | "toggle" | "other"

export type Event = {
	-- "press": a control was used. "result" / "error": a calculation ended.
	-- "show": a keypad came into view. "option": one of the theme's
	-- options changed. "mode": the user changed page or calculator mode.
	kind: "press" | "result" | "error" | "show" | "option" | "mode",
	-- Seconds since it happened.
	age: number,
	-- (An option change comes from nowhere in particular: its rect is the
	-- layer's own area.)
	-- Where: the press point, or the middle of `rect`. In the layer's
	-- coordinates, like `rect` (the control, the display, the keypad).
	x: number,
	y: number,
	rect: Rect,
	page: "calculator" | "converter" | "date" | "graphing" | "settings",
	-- press: what was pressed and how.
	source: ("key" | "button" | "swap" | "date")?,
	-- "assistive": operated through the accessibility interface. A result
	-- or an error has the `via` of the press that brought it about.
	via: ("pointer" | "keyboard" | "assistive")?,
	-- result / error: what brought it about, the press just before it.
	-- "equals" is = (or Enter), "operator" the running total + − × ÷ show,
	-- "function" any other function key. A theme's `sounds` table plays
	-- `result` for "equals" and "function" only, `error` for every cause;
	-- `sound(event, ctx)` sees every one. Absent when no press did (a paste).
	cause: ("equals" | "operator" | "function" | "digit" | "clear" | "backspace" | "memory" | "button")?,
	-- press: what the control does, when it is one of these.
	action: ("clear" | "backspace" | "memory")?,
	-- press on a key: which one. `id` is the same for the same key every
	-- time; use it for variety, not meaning.
	key: { id: number, kind: KeyKind }?,
	-- option: which one, and its new value.
	id: string?,
	value: OptionValue?,
}

export type KeyCell = {
	-- Present on keys the calculator knows by number.
	id: number?,
	kind: KeyKind,
	rect: Rect,
	row: number,
	col: number,
	-- Its corner radius, from the style.
	radius: number,
	enabled: boolean,
	hovered: boolean,
	pressed: boolean,
	checked: boolean,
}

export type DisplayGlyphs = {
	glyphs: { Glyph },
	-- The fitted font size, and where the text sits.
	size: number,
	rect: Rect,
}

export type DisplayValue = {
	text: string,
	glyphs: { Glyph },
	size: number,
	rect: Rect,
	-- The band the value sits in, the readout's full width.
	band: Rect,
	error: boolean,
	align: "start" | "end",
	role: "calculator" | "converter" | "date",
	-- While a change is being shown: what kind, how long ago, and the
	-- first glyph that differs from the old value.
	change: { kind: "typing" | "result" | "replace" | "error", age: number, from: number }?,
	-- The value before the change, for as long as `change` is there.
	previous: DisplayGlyphs?,
}

export type Frame = {
	width: number,
	height: number,
	-- Seconds this layer has spent animating; stands still otherwise.
	time: number,
	-- Seconds since this layer's previous frame while it animates, else 0.
	dt: number,
	-- False: the desktop asked for reduced motion. Draw a still picture;
	-- there are no events and requests for more frames are ignored.
	motion: boolean,
	-- True: nothing has happened for a while and the app has stopped the
	-- clock. Requests for more frames wait until something happens.
	settled: boolean,
	-- The window is the one in use.
	active: boolean,
	-- Where the pointer is, for layers with `pointer = true`.
	pointer: { x: number, y: number }?,
	-- Newest last.
	events: { Event },
	ctx: Ctx,
	-- The keypad layer: every cell of the keypad being drawn.
	keys: { KeyCell }?,
	-- The display layer: the value to paint.
	value: DisplayValue?,
}

-- What a draw hook returns: nil for "this picture stands until something
-- changes", 0 for "again next frame", or seconds until the next frame.
export type Again = number?

export type Placement = { dx: number?, dy: number?, scale: number?, opacity: number? }

export type Layer = {
	draw: ((draw: Draw, frame: Frame) -> Again)?,
	-- Which events this layer is told about, and for how many seconds each
	-- stays in `frame.events`.
	events: { press: number?, result: number?, error: number?, show: number?, option: number?, mode: number? }?,
	-- True: `frame.pointer` is filled in and the layer is redrawn as the
	-- pointer moves over it.
	pointer: boolean?,
	-- Keypad only: move, scale or fade one cell (an entrance, say).
	place: ((key: KeyCell, frame: Frame) -> Placement?)?,
}

---------------------------------------------------------------------------
-- Sound
---------------------------------------------------------------------------

-- Made by gmnb.synth or gmnb.sound.
export type Sound = { kind: "sound" }

-- Hertz, or a note name: "A4" (440), "C#5", "Eb3".
export type Pitch = number | string

-- From the first value to the second over `time` seconds (left out: the
-- layer's or the sound's whole length), then staying there. "exp" (the
-- default) moves by equal musical steps and needs both above zero.
export type Glide = { [number]: number, time: number?, curve: ("exp" | "linear")? }
export type PitchGlide = { [number]: Pitch, time: number?, curve: ("exp" | "linear")? }

-- A layer's loudness over time, in seconds: up to full in `attack`, down
-- to `sustain` (0 to 1) in `decay`, there for `hold`, to silence in
-- `release`. Defaults 0.002, 0.1, 0, 0, 0. With sustain 0 the layer ends
-- after attack + decay.
export type Envelope = {
	attack: number?,
	decay: number?,
	sustain: number?,
	hold: number?,
	release: number?,
	-- How decay and release fall: "exp" (the default; 60 dB over the
	-- segment) or "linear".
	curve: ("exp" | "linear")?,
}

export type SynthFilter = {
	type: "lowpass" | "highpass" | "bandpass",
	-- Hertz, 20 to 20000; may glide.
	freq: number | Glide,
	-- 0.1 to 30. Default 0.707.
	q: number?,
}

export type SynthLayer = {
	-- Default "sine".
	wave: ("sine" | "triangle" | "square" | "saw" | "noise")?,
	-- Default 440. Noise: left out, white noise; given, each value is
	-- held for one period.
	freq: (Pitch | PitchGlide)?,
	-- Cents.
	detune: number?,
	-- Square: the fraction of a period spent high, 0.05 to 0.95.
	width: number?,
	-- Where in its period the wave starts, 0 to 1.
	phase: number?,
	-- Noise: which noise.
	seed: number?,
	-- A sine at `ratio` × the pitch bends this layer's phase by up to
	-- `index` radians.
	fm: { ratio: number, index: number | Glide }?,
	-- Pitch wobble: hertz, and cents either way.
	vibrato: { rate: number, depth: number }?,
	-- Loudness wobble: hertz, and 0 to 1.
	tremolo: { rate: number, depth: number }?,
	filter: SynthFilter?,
	env: Envelope?,
	-- Its share of the mix. Default 1.
	gain: number?,
	-- -1 left to 1 right.
	pan: number?,
	-- Seconds into the sound at which it starts.
	at: number?,
}

-- One effect on the whole mix. Which fields are read depends on `type`.
-- A number left out is: delay time 0.25, feedback 0.35, mix 0.3; reverb
-- size 0.5, damp 0.5, mix 0.3; drive amount 0.5; crush bits 8, rate 12000;
-- a filter's q 0.707 (its freq is required).
-- (In strict mode luau-lsp may reject a list of differing effects written
-- in place. Give the list a type: `local fx: { SynthEffect } = { ... }`.)
export type SynthEffect = {
	type: "delay" | "reverb" | "drive" | "crush" | "lowpass" | "highpass" | "bandpass",
	-- delay: seconds between echoes (0.001 to 1), how much quieter each
	-- is (0 to 0.9), and whether they bounce left and right.
	time: number?,
	feedback: number?,
	ping_pong: boolean?,
	-- delay and reverb: how much of it is added, 0 to 1.
	mix: number?,
	-- reverb: the room's length and how dull its walls are, each 0 to 1.
	size: number?,
	damp: number?,
	-- drive: 0 to 1.
	amount: number?,
	-- crush: bits (1 to 16) and samples a second (100 to 48000).
	bits: number?,
	rate: number?,
	-- lowpass, highpass, bandpass: as a layer's filter.
	freq: (number | Glide)?,
	q: number?,
}

export type Recipe = {
	-- 1 to 8, mixed.
	layers: { SynthLayer },
	-- Up to 4, applied to the mix in order.
	effects: { SynthEffect }?,
	-- The finished sound's loudest sample, 0 to 1. Default 0.5.
	gain: number?,
	-- The whole length in seconds, echoes included; 4 at most. Left out:
	-- as long as the layers, plus the tail of a delay or reverb.
	length: number?,
}

-- One sound, played one way.
export type Play = {
	sound: Sound,
	-- 0 to 1. Default 1.
	volume: number?,
	-- Speed and with it pitch, 0.5 to 2. Default 1.
	rate: number?,
	-- -1 left to 1 right. Default 0.
	pan: number?,
	-- Seconds before it starts, up to 1.
	delay: number?,
}

-- A value, or a range { low, high } to pick from for each play.
export type Varied = number | { number }

-- What the `sounds` table holds for one event name.
export type SoundEntry = Sound | false | {
	-- One sound, or up to 8 to choose from.
	sound: Sound | { Sound },
	-- "cycle" (the default): the list in turn, ranges at random.
	-- "random": both at random. "key": by the key pressed, so the same
	-- key always sounds the same.
	pick: ("cycle" | "random" | "key")?,
	volume: Varied?,
	rate: Varied?,
	-- "position": where the control is across the window, at 60% width.
	pan: (Varied | "position")?,
	delay: number?,
}

-- Which sound for which event. A name with no entry uses the next one
-- listed for it; false plays nothing and stops the search.
export type Sounds = {
	-- Any press.
	press: SoundEntry?,
	-- Any keypad key; then `press`.
	key: SoundEntry?,
	-- These seven try `key`, then `press`.
	digit: SoundEntry?,
	operator: SoundEntry?,
	func: SoundEntry?,
	equals: SoundEntry?,
	toggle: SoundEntry?,
	clear: SoundEntry?,
	backspace: SoundEntry?,
	-- Buttons that aren't keys; then `press`.
	button: SoundEntry?,
	-- The memory buttons; then `button`, then `press`.
	memory: SoundEntry?,
	-- A calculation gave a value, or an error, on the Calculator page.
	result: SoundEntry?,
	error: SoundEntry?,
	-- The page or the calculator's mode was changed.
	mode: SoundEntry?,
}

-- What the `sound` function is given: an event as a layer sees it, in
-- window coordinates, with age 0, and where it happened across the
-- window: -1 at the left edge to 1 at the right.
export type SoundEvent = Event & { pan: number }

-- nil: let the `sounds` table decide. false: play nothing. Else what to
-- play: a sound, one play, or up to 4 plays together.
export type SoundChoice = nil | false | Sound | Play | { Play }

---------------------------------------------------------------------------
-- Options
---------------------------------------------------------------------------

export type Preview = { width: number, height: number, ctx: Ctx }

export type OptionAction = {
	label: string,
	tooltip: string?,
	-- Returns the options to change, by id.
	run: (options: { [string]: OptionValue }, ctx: Ctx) -> { [string]: OptionValue },
}

export type Option = {
	id: string,
	type: "choice" | "color" | "toggle" | "slider" | "note",
	label: string?,
	description: string?,
	-- A heading to gather options under.
	group: string?,
	default: OptionValue?,
	-- choice
	choices: { { id: string, label: string } }?,
	-- choice: draw a tile for one choice; the choices then show as a grid of tiles.
	preview: ((draw: Draw, frame: Preview, choice: string) -> ())?,
	-- slider
	min: number?,
	max: number?,
	step: number?,
	-- slider: how the value is shown.
	format: ("number" | "percent")?,
	-- Shown only when this returns true.
	visible: ((options: { [string]: OptionValue }, ctx: Ctx) -> boolean)?,
	-- Buttons beside the option.
	actions: { OptionAction }?,
}

---------------------------------------------------------------------------
-- The theme: what theme.luau returns
---------------------------------------------------------------------------

export type Theme = {
	api: number,
	name: string,
	description: string?,
	author: string?,
	version: string?,
	license: string?,
	-- Font files in the theme's folder to make available by family name.
	fonts: { string }?,
	-- Shown in About: a title, and a text file in the theme's folder.
	credits: { { title: string, file: string } }?,
	options: { Option }?,
	style: ((ctx: Ctx) -> Style)?,
	layers: {
		-- The whole window, under everything.
		background: Layer?,
		-- Each keypad, over its keys.
		keypad: Layer?,
		-- Each readout: paints the value in place of the app.
		display: Layer?,
		-- The whole window, over everything.
		overlay: Layer?,
	}?,
	-- Which sound goes with which event.
	sounds: Sounds?,
	-- Called for every press, result, error, mode and option event before
	-- `sounds` is consulted.
	sound: ((event: SoundEvent, ctx: Ctx) -> SoundChoice)?,
}

---------------------------------------------------------------------------
-- The gmnb library
---------------------------------------------------------------------------

export type GradientSpec = {
	colors: { Color },
	-- Where each colour sits, 0 to 1, one per colour. Left out, they are
	-- spread evenly.
	stops: { number }?,
	angle: number?,
	from: Point?,
	to: Point?,
	at: Point?,
	center: Point?,
	radius: (number | string)?,
	radius_y: number?,
}

export type Easings = {
	linear: (p: number) -> number,
	in_quad: (p: number) -> number,
	out_quad: (p: number) -> number,
	in_out_quad: (p: number) -> number,
	in_cubic: (p: number) -> number,
	out_cubic: (p: number) -> number,
	in_out_cubic: (p: number) -> number,
	out_quart: (p: number) -> number,
	out_back: (p: number) -> number,
}

export type Gmnb = {
	-- The API version of the running app.
	api: number,

	-- Colours.
	white: Rgba,
	black: Rgba,
	transparent: Rgba,
	TEXT_CONTRAST: number,
	UI_CONTRAST: number,
	hex: (text: string) -> Rgba,
	-- Any Color as an Rgba.
	color: (c: Color) -> Rgba,
	rgb: (r: number, g: number, b: number, a: number?) -> Rgba,
	hsl: (h: number, s: number, l: number, a: number?) -> Rgba,
	to_hsl: (c: Color) -> (number, number, number),
	to_hex: (c: Color) -> string,
	mix: (a: Color, b: Color, t: number) -> Rgba,
	alpha: (c: Color, a: number) -> Rgba,
	over: (top: Color, bottom: Color) -> Rgba,
	complement: (c: Color) -> Rgba,
	luminance: (c: Color) -> number,
	contrast: (a: Color, b: Color) -> number,
	worst_contrast: (c: Color, against: { Rgba } | { string }) -> number,
	with_contrast: (c: Color, against: Color, target: number) -> Rgba,
	gradient_for_text: (text: Color, a: Color, b: Color, target: number) -> (Rgba, Rgba),
	readable_gradient: (a: Color, b: Color, light: Color, dark: Color, target: number) -> (Rgba, Rgba, Rgba),

	-- The base look's colours for any mode and accent.
	base: (dark: boolean, accent: Color?) -> BaseColors,

	-- Paints.
	linear: (spec: GradientSpec) -> Gradient,
	radial: (spec: GradientSpec) -> Gradient,
	conic: (spec: GradientSpec) -> Gradient,

	-- Drawing resources.
	path: () -> Path,
	svg_path: (d: string) -> Path,
	image: (file: string) -> Image,
	noise: (spec: { size: number?, seed: number?, color: Color?, opacity: number? }) -> Image,
	animation: (file: string) -> Animation,
	pixels: (width: number, height: number) -> Pixels,
	ramp: (spec: RampSpec) -> Ramp,

	-- Sound.
	synth: (recipe: Recipe) -> Sound,
	-- WAV or Ogg Vorbis. With `gain`, scaled so its loudest sample is that.
	sound: (file: string, options: { gain: number? }?) -> Sound,
	-- A note name's frequency in hertz: "A4" is 440.
	note: (name: string) -> number,

	-- Motion.
	ease: Easings,

	-- A note for the theme's author (Settings shows the latest few).
	warn: (message: string) -> (),
}

declare gmnb: Gmnb
