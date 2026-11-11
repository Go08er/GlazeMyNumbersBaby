--!strict
-- Swirls: flowing paint inspired by ly's ColorMix, with a calculator of
-- smoked glass standing in it.
--
-- The design's background is the ColorMix field of fairyglade/ly (WTFPL,
-- src/animations/ColorMix.zig): a coordinate bent three times and pushed
-- through three bands of colour, gold, cyan and near-black. Here the theme
-- works out the bent coordinate on a coarse grid five times a second, and
-- the app colours every pixel of it thirty times a second (draw:field,
-- docs/theming-media.md). Everything else is built on that one picture:
--
--   the keys      panes of dark glass with the design's cyan hairline, its
--                 hover and its gold focus ring; a pressed key clears, and
--                 the paint shows through it
--   the paint     is stirred where a key is pressed, runs faster while you
--                 type, and carries a ripple out from every result
--   the digits    are cut out of the same paint, without its black
--   the keypad    swirls into place
--   see-through   the dark band clears first, and the paint is ribbons of
--                 its bright bands over the desktop, under the same glass
--
-- The file reads top to bottom:
--   1. Colours     the six sets of bands, and two of your own
--   2. Inks, glass what keeps text readable over paint that moves
--   3. Style       how every part of the window looks
--   4. The field   the swirl itself, and what disturbs it
--   5. Keypad      the keys swirling in
--   6. Display     digits made of paint
--   7. Options     what Settings offers, and its tiles
--   8. Sound       drops, bubbles and a stirred glass

local hex, hsl, mix, alpha, over = gmnb.hex, gmnb.hsl, gmnb.mix, gmnb.alpha, gmnb.over
local white, black = gmnb.white, gmnb.black
local ease = gmnb.ease
-- 4.5 and 3: what WCAG asks of text, and of large text and lines.
local TEXT, UI = gmnb.TEXT_CONTRAST, gmnb.UI_CONTRAST

local cos, sin, sqrt, exp = math.cos, math.sin, math.sqrt, math.exp
local floor, ceil, min, max, clamp = math.floor, math.ceil, math.min, math.max, math.clamp
local readf32, writef32 = buffer.readf32, buffer.writef32
local TAU = 2 * math.pi

---------------------------------------------------------------------------
-- 1. Colours
---------------------------------------------------------------------------

-- A set of bands: two bright colours and the dark one they flow through.
--   a       the warm one. Whatever is on, lit or the next thing to press:
--           the equals key, the focus ring, a border under the pointer.
--   b       the cool one. Lines: borders, links, the operators' glyphs.
--   ground  the dark one, which the glass is made of too.
type Bands = { a: Rgba, b: Rgba, ground: Rgba }

local function bands(a: string, b: string, ground: string): Bands
	return { a = hex(a), b = hex(b), ground = hex(ground) }
end

-- The first is the own: #ffcf29, #0ed6f0, #010609.
local BANDS: { [string]: Bands } = {
	party = bands("#ffcf29", "#0ed6f0", "#010609"),
	lava = bands("#ffb224", "#ff3b4e", "#100306"),
	lagoon = bands("#7dffb0", "#2b8cff", "#010a14"),
	orchid = bands("#ff7fd0", "#8f7bff", "#08030f"),
	absinthe = bands("#d7ff3e", "#19e68c", "#030a04"),
	marble = bands("#f5faff", "#93a1ad", "#020507"),
}

-- And two sets of one's own. System is worked out from the desktop's
-- accent colour (this blue on a desktop that shares none), Freestyle from
-- three colours you pick (until you do, these). Either can start from any
-- colour at all: section 2 works out bands from them that the glass and
-- the figures can bear.
local FALLBACK_ACCENT = "#3584e4"
local DEFAULT_FIRST, DEFAULT_SECOND, DEFAULT_DARK = "#ffb86b", "#b18cff", "#160a2a"

-- The design's text colour.
local FG = hex("#f5faff")

-- The ramp of a set: its three colours in the design's order, each
-- blending into the next between 8% and 92% of its band, and after the
-- last the first again. The six are made once; one's own, whenever its
-- colours change (the same colours again are the same ramp).
local function ramp_of(set: Bands): Ramp
	return gmnb.ramp({ colors = { set.a, set.b, set.ground }, edge = { 0.08, 0.92 }, cycle = true })
end
local RAMPS: { [string]: Ramp } = {}
for id, set in BANDS do
	RAMPS[id] = ramp_of(set)
end

---------------------------------------------------------------------------
-- 2. Inks and glass
---------------------------------------------------------------------------
--
-- Text here lies on glass, and under the glass is paint that moves: gold
-- one second, near-black the next. White on that gold is 1.4 to 1. So
-- nothing is chosen by eye. The glass has three strengths to choose from,
-- and for each set of bands, at each window opacity, the least strength at
-- which every ink still reads over every colour the field can show is
-- worked out (`least_glass`); the glass is never thinner than that.
--
-- The field's colours are its three bands and the blends between two of
-- them. Glass is dark and the inks are light, so the worst ground is the
-- brightest one, and a blend is never brighter than the brighter of its
-- two ends: the three bands are all that needs checking. (The tests check
-- every entry of the ramp all the same: crates/gmnb-theme/tests/css.rs.)

-- How strong the glass is, by the `glass` option: how much of the paint
-- it hides. The last is the design's panels, rgba(1, 6, 9, .91).
local CLEAR, SMOKED, PANEL = 0.60, 0.76, 0.91
local STRENGTH: { [string]: number } = { clear = CLEAR, smoked = SMOKED, panel = PANEL }

-- Whether `ink`, which may be translucent, reaches `target` on every one
-- of `grounds`.
local function reads(ink: Rgba, grounds: { Rgba }, target: number): boolean
	for _, ground in grounds do
		if gmnb.contrast(over(ink, ground), ground) < target then
			return false
		end
	end
	return true
end

-- Text that steps back: `ink` at the lowest opacity, from `least` up, at
-- which it still reads on every ground.
local function recede(ink: Rgba, grounds: { Rgba }, least: number): Rgba
	if reads(alpha(ink, least), grounds, TEXT) then
		return alpha(ink, least)
	end
	local low, high = least, 1
	for _ = 1, 6 do
		local middle = (low + high) / 2
		if reads(alpha(ink, middle), grounds, TEXT) then
			high = middle
		else
			low = middle
		end
	end
	return alpha(ink, high)
end

-- `color` with its lightness moved, as little as possible, until it
-- reaches `target` on every ground.
local function legible(color: Rgba, grounds: { Rgba }, target: number): Rgba
	for _, ground in grounds do
		color = gmnb.with_contrast(color, ground, target)
	end
	return color
end

-- One thing that has to stay readable: an ink, the contrast it needs, and
-- what lies between it and the glass (bottom-most first): a tint.
type Check = { ink: Rgba, target: number, on: { Rgba } }

-- The least strength, in 64ths, at which glass of `tint` over each of
-- `backs` lets every check hold.
-- With `halves`, by halves instead (six tries, not as many as there are
-- steps it can come down): where every back is brighter than the glass,
-- as on a see-through window over a white desktop, more glass only ever
-- reads better, and what holds at one strength holds at every stronger.
local function least_glass(tint: Rgba, backs: { Rgba }, checks: { Check }, margin: number, halves: boolean?): number
	local function holds(strength: number): boolean
		for _, back in backs do
			local glass = over(alpha(tint, strength), back)
			for _, check in checks do
				local ground = glass
				for _, layer in check.on do
					ground = over(layer, ground)
				end
				if gmnb.contrast(over(check.ink, ground), ground) < check.target * margin then
					return false
				end
			end
		end
		return true
	end
	if halves then
		-- (`high` always holds: solid glass is the design's own pane.)
		local low, high = -1, 64
		while high - low > 1 do
			local middle = (low + high) // 2
			if holds(middle / 64) then
				high = middle
			else
				low = middle
			end
		end
		return high / 64
	end
	-- Down from solid to the last step that holds.
	local step = 64
	while step > 0 and holds((step - 1) / 64) do
		step -= 1
	end
	return step / 64
end

-- Bands of one's own. The design's own are two bright colours and a
-- near-black, and the glass and the figures lean on that: the glass is
-- made of the dark band, the text on it is light, and the figures are
-- painted in the blends from the first bright band to the second, on a
-- panel. So whatever the colours given:
--   the dark band  keeps the hue and some of the strength it was given,
--                  at the design's own near-black (#010609 is 2% light);
--   the two bright ones keep theirs, and are lifted (as little as they
--                  need, toward white) until every blend between them
--                  reads as a figure on a panel over white paint, the
--                  brightest thing that can be under one. That also puts
--                  the dark band's text 4.5 on either of them.

-- A colour option's saved text, or `fallback` if someone edited it into
-- nonsense. (Paint is solid: an opacity written into it is dropped.)
local function colour(value: OptionValue?, fallback: string): Rgba
	if type(value) == "string" and (#value == 7 or #value == 9) and string.match(value, "^#%x+$") then
		return hex(string.sub(value, 1, 7))
	end
	return hex(fallback)
end

local function dark_of(c: Rgba): Rgba
	local h, s = gmnb.to_hsl(c)
	return hsl(h, min(s, 0.8), 0.035)
end

local function bright_pair(a: Rgba, b: Rgba, ground: Rgba): (Rgba, Rgba)
	local under = over(alpha(ground, PANEL), white)
	local target = TEXT * 1.04
	local function reads(): boolean
		for i = 0, 8 do
			if gmnb.contrast(mix(a, b, i / 8), under) < target then
				return false
			end
		end
		return true
	end
	a, b = gmnb.with_contrast(a, under, target), gmnb.with_contrast(b, under, target)
	-- (Two colours far apart on the wheel are greyer, and darker, half way
	-- between: then both are lifted a little more.)
	for _ = 1, 30 do
		if reads() then
			break
		end
		a, b = mix(a, white, 0.05), mix(b, white, 0.05)
	end
	return a, b
end

-- System's bands: the accent, kept vivid (a grey stays grey), as the
-- first band; for the second, the colour as far round the wheel from it
-- as the design's cyan is from its gold; and the dark band in the second's
-- hue, as the design's is.
local function system_bands(accent: Rgba): Bands
	local h, s = gmnb.to_hsl(accent)
	local v = if s < 0.08 then s else clamp(s, 0.6, 1)
	local ground = dark_of(hsl(h + 140, v, 0.5))
	local a, b = bright_pair(hsl(h, v, 0.6), hsl(h + 140, v, 0.6), ground)
	return { a = a, b = b, ground = ground }
end

-- Freestyle's: the three colours picked.
local function custom_bands(o: { [string]: OptionValue }): Bands
	local ground = dark_of(colour(o.paint_dark, DEFAULT_DARK))
	local a, b = bright_pair(colour(o.paint_first, DEFAULT_FIRST), colour(o.paint_second, DEFAULT_SECOND), ground)
	return { a = a, b = b, ground = ground }
end

-- What the bands of set `id` are made from for `ctx`, as a name: the
-- same name, the same bands.
local function bands_name(id: string, ctx: Ctx): string
	if id == "system" then
		return "system " .. (if ctx.accent then gmnb.to_hex(ctx.accent) else "-")
	elseif id == "custom" then
		local o = ctx.options
		return "custom " .. tostring(o.paint_first) .. tostring(o.paint_second) .. tostring(o.paint_dark)
	end
	return id
end

local function bands_for(id: string, ctx: Ctx): Bands
	if id == "system" then
		return system_bands(ctx.accent or hex(FALLBACK_ACCENT))
	elseif id == "custom" then
		return custom_bands(ctx.options)
	end
	return BANDS[id]
end

-- A see-through window. The paint thins itself (its layer says
-- `fade = false`, and is drawn at full strength over the desktop): its
-- dark band clears first, so that from half opacity down the desktop shows
-- clear between ribbons of the two bright bands, and they keep much of
-- their colour to the end (two fifths at 0.1). The glass keys and panels stay,
-- as strong as the text on them needs over whatever shows through. Both
-- move smoothly with the slider, and are the paint as it was at 1.
-- Returns how opaque the bright bands are, and the dark one; and the veil:
-- how much of the dark band still lies under the paint as the window's
-- own background (all of it at 1, none from 0.7 down), so that the glass
-- and what the faint text needs go from an opaque window's to a see-through
-- one's over that range rather than at once (a scheme is made for each
-- 64th: some dozen steps).
local function smooth(t: number): number
	t = clamp(t, 0, 1)
	return t * t * (3 - 2 * t)
end
local function thinned(opacity: number): (number, number, number)
	return 0.4 + 0.6 * smooth((opacity - 0.1) / 0.9), smooth((opacity - 0.5) / 0.5), smooth((opacity - 0.7) / 0.3)
end

-- A set of bands, and everything worked out from it.
type Scheme = {
	id: string,
	-- The ramp its paint is coloured through (on a see-through window, the
	-- paint thinned), and the one its figures and tiles are: the bands
	-- themselves.
	ramp: Ramp,
	figure_ramp: Ramp,
	-- How far the dark band has cleared: 0 on an opaque window.
	cleared: number,
	-- How much of the dark band lies under the paint (see `thinned`).
	veil: number,
	a: Rgba,
	b: Rgba,
	ground: Rgba,
	fg: Rgba,
	-- The glass under the pointer: the design's #13252b.
	lift: Rgba,
	-- The text colour stepping back: function keys, then captions.
	soft: Rgba,
	dim: Rgba,
	faint: Rgba,
	-- The two bright colours as text.
	a_ink: Rgba,
	b_ink: Rgba,
	-- The focus ring: the warm band, 9:1 from the dark one.
	ring: Rgba,
	danger: Rgba,
	series: { Rgba },
	-- How strong the glass is: the least the text can bear; keys and
	-- panes; a key being pressed; the readout and the panels; the graph.
	least: number,
	glass: number,
	pressed: number,
	panel: number,
	canvas: number,
}

local function finish(id: string, set: Bands, strength: number, opacity: number): Scheme
	local a, b, ground = set.a, set.b, set.ground
	local fg = FG
	-- The design's hover colour, #13252b, is its near-black a twelfth of the
	-- way to its cyan and then a fourteenth of the way to its text.
	local lift = mix(mix(ground, b, 0.08), fg, 0.07)
	local colours = { a, b, ground }

	-- What the field shows as: itself, or on a see-through window the paint
	-- as it thins (`thinned`), each band at its own opacity and the blends
	-- between two as the ramp makes them (colour and opacity blended
	-- together), over a white desktop. (Glass is dark and every ink here
	-- light: the brightest of these is what matters, so a black desktop,
	-- under which all of them are darker, never does; and a blend is
	-- between its ends. A scheme is made for each 64th of an opacity drag,
	-- inside the style's time: no more is worked out than tells.)
	local bright, dark, veil = thinned(opacity)
	local backs: { Rgba } = {}
	if opacity >= 1 then
		backs = colours
	else
		local ends: { { color: Rgba, opacity: number } } = {
			{ color = a, opacity = bright },
			{ color = b, opacity = bright },
			{ color = ground, opacity = dark },
		}
		do
			-- Under the paint: the veil, which the app thins with the window.
			local under = over(alpha(ground, veil * opacity), white)
			for i, from in ends do
				local to = ends[i % 3 + 1]
				for _, t in { 0, 0.25, 0.5, 0.75 } do
					local c = mix(from.color, to.color, t)
					table.insert(backs, over(alpha(c, from.opacity + (to.opacity - from.opacity) * t), under))
				end
			end
			-- And that alone: what shows where the dark band has cleared,
			-- and everywhere if the paint is ever not drawn.
			table.insert(backs, under)
		end
	end

	-- The text colour stepping back. How far it may is settled on the
	-- clearest glass an opaque window can have, alone and under the wash
	-- of a field or of a button under the pointer, so that it is the same
	-- text whatever glass is chosen.
	local on_clear: { Rgba } = {}
	for _, band in colours do
		local glass = over(alpha(lift, CLEAR), band)
		table.insert(on_clear, glass)
		table.insert(on_clear, over(alpha(fg, 0.08), glass))
	end
	local soft = recede(fg, on_clear, 0.86)
	local dim = recede(fg, on_clear, 0.68)

	-- The glass: the strength chosen, or what the text needs if that is
	-- more. On a see-through window the text colour keeps the 4.5 it has
	-- on an opaque one, and everything fainter or coloured keeps at least 3
	-- (from 4.5 at 1 to 3 at 0.7, as the veil goes).
	local other = UI + (TEXT - UI) * (if opacity >= 1 then 1 else veil)
	local faint = alpha(fg, 0.42)
	local checks: { Check } = {
		{ ink = fg, target = TEXT, on = {} },
		{ ink = fg, target = TEXT, on = { alpha(fg, 0.08) } },
		{ ink = soft, target = other, on = {} },
		{ ink = dim, target = other, on = {} },
		{ ink = dim, target = other, on = { alpha(fg, 0.08) } },
		-- A field's placeholder: 2.45, as in the base look.
		{ ink = faint, target = 2.45, on = { alpha(fg, 0.08) } },
	}
	local see = opacity < 1
	local least = least_glass(lift, backs, checks, 1.02, see)
	local glass = max(strength, least)

	-- The two bright colours as text, and a coral for errors (neither band
	-- in any set): each as near itself as that glass lets it be, alone and
	-- on a wash of its own colour.
	local function ink(color: Rgba, wash: Rgba): Rgba
		local grounds: { Rgba } = {}
		for _, back in backs do
			local under = over(alpha(lift, glass), back)
			table.insert(grounds, under)
			table.insert(grounds, over(wash, under))
		end
		color = legible(color, grounds, other * 1.03)
		table.insert(checks, { ink = color, target = other, on = {} })
		table.insert(checks, { ink = color, target = other, on = { wash } })
		return color
	end
	local a_ink = ink(a, alpha(a, 0.16))
	local b_ink = ink(b, alpha(b, 0.16))
	local coral = hsl(4, 1.0, 0.70)
	local danger = ink(coral, alpha(coral, 0.10))
	-- (A colour that is as light as it can be and still doesn't read on
	-- its own wash, white on white's, needs more glass instead.)
	least = max(least, least_glass(lift, backs, checks, 1.02, see))
	glass = max(strength, least)

	-- A pressed key clears as far as its glyph (large: 3) allows.
	local pressed = min(glass, least_glass(lift, backs, { { ink = fg, target = UI, on = {} } }, 1.05, see))

	-- Graph colours: the two bands and four more, each made to read as a
	-- line on the graph's own glass, which is never thinner than a panel.
	local panel = max(glass, PANEL)
	local on_panel: { Rgba } = {}
	for _, back in backs do
		table.insert(on_panel, over(alpha(ground, panel), back))
	end
	local series: { Rgba } = {}
	for i, c in { a, b, hex("#ff7ad9"), hex("#8dff7a"), hex("#b79bff"), hex("#ff9a6e") } do
		series[i] = legible(c, on_panel, UI * 1.05)
	end

	-- The paint's ramp: the bands themselves, or thinned.
	local whole = RAMPS[id] or ramp_of(set)
	local paint = whole
	if opacity < 1 then
		paint = gmnb.ramp({
			colors = { alpha(a, bright), alpha(b, bright), alpha(ground, dark) },
			edge = { 0.08, 0.92 },
			cycle = true,
		})
	end

	return {
		id = id,
		ramp = paint,
		figure_ramp = whole,
		cleared = 1 - dark,
		veil = if opacity >= 1 then 1 else veil,
		a = a,
		b = b,
		ground = ground,
		fg = fg,
		lift = lift,
		soft = soft,
		dim = dim,
		faint = faint,
		a_ink = a_ink,
		b_ink = b_ink,
		ring = gmnb.with_contrast(a, ground, UI * UI * 1.01),
		danger = danger,
		series = series,
		least = least,
		glass = glass,
		pressed = pressed,
		panel = panel,
		canvas = panel,
	}
end

-- Finishing a set takes a few thousand contrast checks, and the draw hooks
-- ask for the scheme every frame: remember what was built.
local remembered: { [string]: Scheme } = {}
local remembered_count = 0

local function bands_id(value: OptionValue?): string
	return if type(value) == "string" and (BANDS[value] or value == "system" or value == "custom")
		then value
		else "party"
end

local function scheme_for(id: string, ctx: Ctx): Scheme
	local strength = STRENGTH[tostring(ctx.options.glass)] or SMOKED
	-- (Dragging the opacity slider: a scheme for every 64th. Dragging
	-- through the colour chooser: one for every step.)
	local opacity = ceil(ctx.opacity * 64) / 64
	local key = bands_name(id, ctx) .. "|" .. strength .. "|" .. opacity
	local s = remembered[key]
	if not s then
		if remembered_count >= 96 then
			remembered, remembered_count = {}, 0
		end
		s = finish(id, bands_for(id, ctx), strength, opacity)
		remembered[key] = s
		remembered_count += 1
	end
	return s
end

local function scheme(ctx: Ctx): Scheme
	return scheme_for(bands_id(ctx.options.bands), ctx)
end

---------------------------------------------------------------------------
-- 3. Style
---------------------------------------------------------------------------

-- The design's panels: radius 12.
local RADIUS = 12

local function style(ctx: Ctx): Style
	local s = scheme(ctx)
	local a, b, ground, fg, lift = s.a, s.b, s.ground, s.fg, s.lift
	local soft, dim, faint = s.soft, s.dim, s.faint
	local a_ink, b_ink, danger = s.a_ink, s.b_ink, s.danger
	-- The page is set in the system's own interface font at weight 500.
	local system = ctx.font
	local see = ctx.opacity < 1

	local function sans(size: number, weight: number?, spacing: number?): Font
		return { size = size, weight = weight or 500, spacing = spacing or 0 }
	end
	-- Numbers: the bundled Inter, heavy enough for paint to show in.
	local function figures(size: number?, weight: number): Font
		return { family = { "Inter Variable", system.family }, size = size, weight = weight, features = { "tnum" } }
	end
	-- A readout's figures, which the app shrinks to fit its width: their
	-- optical size held at the one they have at their largest. (Left to
	-- follow the size, a shrunk line is set in Inter's wider text cut and
	-- comes out longer than the width it was shrunk for: the date page's
	-- "4 months, 1 week, 1 day" was cut off in a new window.)
	local function readout(largest: number, weight: number): Font
		return {
			family = { "Inter Variable", system.family },
			weight = weight,
			features = { "tnum" },
			variations = { opsz = clamp(largest, 14, 32) },
		}
	end

	-- Glass. A pane is the dark band at the strength worked out above; a
	-- raised pane (a number key, the glass under the pointer) is the
	-- page's hover colour at the same strength.
	local pane = alpha(ground, s.glass)
	local raised = alpha(lift, s.glass)
	local slab = alpha(ground, s.panel)
	-- The design's lines: its cyan at 0.6 around a panel, fainter round a
	-- key (a little less faint as the paint clears from behind the keys).
	local line = alpha(b, 0.6)
	local hair = alpha(b, 0.3 + 0.25 * s.cleared)
	local on_a = ground
	-- The design's shadow under a panel: 0 8px 32px #01060955.
	local drop: Shadow = { y = 8, blur = 32, color = alpha(ground, 0.33) }
	-- A dark edge round text on a see-through window, for the wallpaper no
	-- contrast figure knows about (coming in as the veil goes).
	local edge = 1 - s.veil
	local halo: { Shadow } = if see
		then { { blur = 3, color = alpha(ground, 0.9 * edge) }, { blur = 10, color = alpha(ground, 0.75 * edge) } }
		else {}

	-- Something lit: the warm band, solid, with the dark one written on it.
	local lit: State = { background = a, color = on_a, border = false }
	-- Something chosen from a list: a wash of the warm band and a bar of it.
	local chosen: State = {
		background = alpha(a, 0.13),
		shadow = { { inset = true, x = 3, color = a } },
		color = fg,
		icon = { color = a_ink },
	}
	local wash: State = { background = alpha(fg, 0.08) }
	local wash_more: State = { background = alpha(fg, 0.15) }

	local function text_button(padding: Spacing, font: Font): Control
		return {
			min_width = 30,
			min_height = 30,
			padding = padding,
			radius = 8,
			font = font,
			color = soft,
			background = gmnb.transparent,
			transition = { background = 140, color = 140, shadow = 140 },
			hover = { background = alpha(fg, 0.08), color = fg },
			active = { background = alpha(fg, 0.15), color = fg },
			checked = { background = alpha(a, 0.16), color = a_ink, border = { width = 1, color = alpha(a, 0.6) } },
			disabled = { color = alpha(fg, 0.28) },
		}
	end

	local function icon_button(size: number, icon: number): Control
		return {
			min_width = size,
			min_height = size,
			radius = 8,
			color = fg,
			icon = { size = icon },
			transition = { background = 140 },
			hover = wash,
			active = wash_more,
			checked = { background = alpha(a, 0.16), color = a_ink },
			disabled = { opacity = 0.4 },
		}
	end

	return {
		-- Tokens. Anything not styled below is derived from these; the two
		-- surfaces are glass, so whatever is derived reads over the paint.
		colors = {
			bg = ground,
			surface = lift,
			surface2 = raised,
			border = hair,
			fg = fg,
			fg_dim = dim,
			fg_faint = faint,
			accent = a,
			on_accent = on_a,
			accent_text = b_ink,
			hover = alpha(fg, 0.08),
			press = alpha(fg, 0.15),
			danger = danger,
			series = s.series,
		},
		fonts = { ui = system.family, display = { "Inter Variable", system.family } },
		text = {
			body = sans(system.size),
			strong = sans(system.size, 700),
			small = sans(13),
			caption = sans(12),
			key = sans(19),
			title = sans(17, 600),
		},
		shape = { radius = RADIUS, row_radius = 8, card_radius = RADIUS },
		icons = { stroke = 1.9, taper = 0.5 },
		motion = {
			page = 240,
			mode = 200,
			panel = 220,
			flip = 260,
			graph_view = 220,
			graph_side = 240,
			date = 260,
			history_row = 300,
			equation_row = 260,
			icon_draw = 600,
			graph_draw = 1000,
			graph_zoom = 240,
		},
		layout = {
			calculator = { margin = { 0, 8, 8, 8 }, gap = 8, panel_width = 300, panel_margin = { 0, 8, 8, 0 } },
			converter = { margin = { 0, 12, 10, 12 }, gap = 10, gap_wide = 24 },
			date = { margin = { 4, 16, 16, 16 }, gap = 12 },
			graph = { margin = { 0, 8, 8, 8 }, gap = 8, side_width = 340 },
		},
		stock = {
			window = ground,
			view = mix(ground, black, 0.3),
			headerbar = ground,
			sidebar = ground,
			dialog = mix(ground, lift, 0.5),
			popover = lift,
			card = alpha(fg, 0.05),
			fg = fg,
			accent = b_ink,
			accent_bg = a,
			accent_fg = on_a,
			destructive = danger,
			error = danger,
		},

		window = {
			-- Under the field, which covers it: the design's own backdrop, its
			-- three bands on a slant. On a see-through window the veil of the
			-- dark band, going from 1 to 0.7 (see `thinned`), and then
			-- nothing: the thinned paint is all there is between the desktop
			-- and the glass.
			background = if see
				then alpha(ground, s.veil)
				else gmnb.linear({ angle = 135, colors = { a, ground, b } }),
			color = fg,
			font = sans(system.size),
			text_halo = halo,
		},
		plain = { background = pane, radius = RADIUS },
		-- The design's gold ring. Around a key it lies on the paint itself,
		-- which is gold too where the warm band flows (and the equals key
		-- is filled with it): a line of the dark band along each side
		-- keeps it seen there, and the gold wherever the paint is dark.
		-- Ring and line 9:1 apart, one of them reaches 3:1 on any colour
		-- at all (orchid's pink is lifted a little for it).
		focus = { width = 3, color = s.ring, offset = 5, edge = { width = 1, color = ground } },
		scrim = alpha(ground, 0.55),

		header = {
			background = pane,
			height = 46,
			title = { font = sans(17, 600), color = fg },
			button = icon_button(34, 18),
			controls = {
				min_width = 24,
				min_height = 24,
				radius = 999,
				margin = { 0, 5 },
				background = alpha(fg, 0.10),
				color = fg,
				icon = { size = 16 },
				hover = { background = alpha(fg, 0.20) },
				active = { background = alpha(fg, 0.30) },
			},
			close = { hover = { background = danger, color = on_a }, active = { background = danger, color = on_a } },
		},
		nav = {
			background = alpha(ground, 0.95),
			shadow = { { inset = true, x = -1, color = line } },
			width = 290,
			title = { font = sans(17, 600), color = fg },
			header = { font = sans(12, 600, 0.1), color = dim, margin = { 10, 18, 2, 18 } },
			uppercase_headers = true,
			row = {
				radius = 8,
				margin = { 1, 8 },
				padding = { 0, 10 },
				min_height = 40,
				font = sans(15),
				color = fg,
				icon = { size = 18, color = soft },
				transition = { background = 140 },
				hover = wash,
				active = wash_more,
				checked = chosen,
			},
			settings = { radius = 8, hover = wash, active = wash_more },
		},

		display = {
			-- The design's panel. Its hairline is drawn by the display layer.
			background = slab,
			radius = RADIUS,
			padding_x = 16,
			heights = { standard = "auto", scientific = "auto", programmer = "auto", compact = "auto" },
			sizes = { standard = 62, scientific = 54, programmer = 46, compact = 42 },
			min_size = 18,
			value = {
				font = readout(62, 700),
				-- The display layer paints the value; these are what it would
				-- be without it.
				color = gmnb.linear({ angle = 90, colors = { a, b } }),
				error_color = danger,
				error_scale = 0.6,
				glow = false,
			},
			expression = { font = sans(14), color = dim },
			expression_min_size = 13,
			halo = halo,
		},

		keys = {
			gap = 6,
			gap_compact = 4,
			radius = RADIUS,
			-- No least size: in the least window, 300 × 400, the keys take
			-- what room is left, as the base's do (and so do the bits).
			min_width = 0,
			min_height = 0,
			padding = { 0, 2 },
			icon = { size = 22, stroke = 1.9 },
			-- A key fogs over again slowly after it was pressed (the keypad
			-- layer and the field know the same 420 ms).
			transition = {
				background = { ms = 420, ease = "ease-out" },
				shadow = 180,
				transform = { ms = 300, ease = "spring" },
				filter = 180,
			},
			-- The design's hover: its lifted glass and a gold border.
			hover = { background = raised, border = { width = 1, color = a } },
			-- Pressed, the glass clears and the paint shows.
			active = {
				background = alpha(lift, s.pressed),
				border = { width = 1, color = a },
				scale = 0.96,
				transition = { background = 40, transform = 60 },
			},
			-- A key that can't be used keeps its glass (a see-through window
			-- needs it) and loses its glyph.
			disabled = { background = alpha(ground, s.glass), border = { width = 1, color = alpha(b, 0.10) }, fade = 0.3 },
			number = {
				font = sans(21),
				color = fg,
				background = raised,
				border = { width = 1, color = hair },
			},
			func = {
				font = sans(15),
				color = soft,
				background = pane,
				border = { width = 1, color = alpha(b, 0.18) },
			},
			operator = {
				font = sans(23, 400),
				color = b_ink,
				background = { alpha(b, 0.10), pane },
				border = { width = 1, color = line },
			},
			equals = {
				font = sans(25, 600),
				color = on_a,
				background = gmnb.linear({ angle = 180, colors = { mix(a, white, 0.18), a } }),
				border = false,
				icon = { stroke = 2.4 },
				hover = { brightness = 1.08 },
				active = { brightness = 1.16, scale = 0.96, transition = { transform = 60, filter = 60 } },
			},
			on = lit,
			pad = { gap = 4, radius = 8, min_height = 32, font = sans(15) },
		},

		text_button = text_button({ 0, 6 }, sans(13, 600, 0.03)),
		text_button_gap = 2,
		flyout_button = text_button({ 0, 10 }, sans(13)),
		icon_button = icon_button(34, 18),
		button = { background = alpha(fg, 0.10), radius = 8, hover = { background = alpha(fg, 0.16) }, checked = lit },
		segmented = {
			gap = 4,
			item = {
				font = sans(13),
				color = soft,
				radius = 8,
				background = alpha(fg, 0.07),
				border = { width = 1, color = alpha(b, 0.18) },
				transition = { background = 160, color = 160 },
				hover = { background = alpha(fg, 0.12), color = fg },
				active = { background = alpha(fg, 0.18), color = fg },
				checked = lit,
			},
		},
		field = {
			background = alpha(fg, 0.07),
			color = fg,
			border = { width = 1, color = hair },
			radius = 8,
			font = sans(15),
			min_height = 36,
			padding = { 0, 10 },
			placeholder = faint,
			selection = alpha(b, 0.35),
			caret = a,
			focus = { border = { width = 2, color = a } },
			error = { border = { width = 2, color = danger } },
		},
		row = { radius = 8, hover = wash, active = wash_more, checked = { background = alpha(a, 0.14) } },
		section = { font = sans(12, 600, 0.04), color = dim },
		slider = {
			track = { height = 4, color = alpha(fg, 0.18), radius = 2 },
			fill = a,
			knob = { size = 16, color = a, halo = alpha(a, 0.2), halo_size = 26 },
		},
		switch = { off = alpha(fg, 0.2), on = a, knob = fg, knob_on = on_a },
		scrollbar = { color = alpha(fg, 0.35) },
		-- (Settings' groups, on its dialog.)
		card = { background = lift, radius = RADIUS },
		popover = {
			padding = 8,
			radius = RADIUS,
			background = lift,
			border = { width = 1, color = line },
			shadow = { drop },
		},
		menu = { padding = 6, radius = 10, background = lift, border = { width = 1, color = line }, shadow = { drop } },
		toast = { background = alpha(fg, 0.95), color = ground, radius = RADIUS },
		tooltip = { background = alpha(fg, 0.95), color = ground, radius = 8 },

		panel = {
			-- The design's panel, whole.
			background = slab,
			radius = RADIUS,
			border = { width = 1, color = line },
			shadow = { drop },
			padding = 8,
			sheet = { background = alpha(lift, 0.98), border = { width = 1, color = line }, radius = 14, shadow = { drop } },
			tabs = { item = { font = sans(14, 600), radius = 8, checked = lit } },
			row = { radius = 8, transition = { background = 140 }, hover = wash, active = wash_more },
			expression = { font = sans(13), color = dim },
			result = { font = figures(22, 600), color = fg },
			delete = { min_width = 22, min_height = 22, radius = 999, color = soft, hover = wash_more, active = wash_more, transition = { opacity = 160 } },
			delete_reveal = "hover",
			memory_button = {
				min_height = 24,
				padding = { 0, 8 },
				radius = 8,
				font = sans(11, 600, 0.04),
				color = soft,
				background = alpha(fg, 0.08),
				hover = { background = alpha(fg, 0.15), color = fg },
				active = { background = alpha(fg, 0.22), color = fg },
				transition = { background = 140, opacity = 160 },
			},
			memory_reveal = "hover",
			empty = { font = sans(14), color = dim },
		},

		programmer = {
			radix = {
				row = {
					radius = 8,
					padding = { 0, 8 },
					min_height = 26,
					transition = { background = 160, shadow = 160 },
					hover = wash,
					active = wash_more,
					checked = { background = alpha(a, 0.13), shadow = { { inset = true, x = 3, color = a } } },
				},
				name = { font = sans(12, 700, 0.06), color = dim },
				name_checked = a_ink,
				value = { font = figures(14, 500), color = fg },
			},
			bit = {
				min_width = 0,
				min_height = 0,
				padding = 0,
				radius = 6,
				font = figures(15, 500),
				color = dim,
				background = alpha(fg, 0.05),
				transition = { background = 160, color = 160 },
				hover = { background = alpha(fg, 0.12) },
				active = { background = alpha(fg, 0.2) },
				-- A set bit is lit.
				checked = { background = a, color = on_a, font = { weight = 700 } },
				disabled = { opacity = 0.3 },
			},
			bit_index = { font = sans(10), color = dim },
		},

		date = {
			-- (A pane has no padding of its own: what lies on one steps in
			-- from its edge by its own margin.)
			label = { font = sans(12, 600, 0.04), color = dim, margin = { 8, 0, 2, 10 } },
			field = {
				min_height = 40,
				padding = { 0, 12 },
				radius = 8,
				font = sans(15),
				color = fg,
				background = alpha(fg, 0.07),
				border = { width = 1, color = hair },
				hover = { background = alpha(fg, 0.12), border = { width = 1, color = a } },
				active = { background = alpha(fg, 0.18), border = { width = 1, color = a } },
				checked = lit,
			},
			result_panel = {
				background = slab,
				border = { width = 1, color = line },
				radius = RADIUS,
				padding = { 12, 16, 14, 16 },
				margin = { 10, 8, 8, 8 },
			},
			result = { size = 32, font = readout(32, 650) },
			add_result = { size = 28, font = readout(28, 650) },
			subresult = { font = sans(15), color = dim },
			calendar = {
				title = { font = sans(15, 600), color = fg },
				weekday = { font = sans(12), color = dim },
				day = {
					radius = 999,
					font = { size = 13, weight = 500, features = { "tnum" } },
					color = fg,
					hover = wash,
					active = wash_more,
					checked = lit,
				},
				other_month = faint,
				today = { width = 1, color = b },
				focus = { width = 2, color = a, offset = 1 },
			},
		},

		converter = {
			-- Each value in a panel of its own, as the readout is: its figures
			-- are made of paint too.
			field = {
				background = slab,
				border = { width = 1, color = alpha(b, 0.18) },
				radius = RADIUS,
				padding = { 6, 10, 8, 10 },
				margin = { 8, 8, 0, 8 },
				transition = { shadow = 180 },
				checked = { border = { width = 1, color = a } },
			},
			marker = { color = a, width = 36, height = 3 },
			value = { size = 42, font = readout(42, 600), checked_weight = 700, focus = { width = 3, color = a, offset = 5, radius = 8 } },
			symbol = { font = sans(24), color = dim },
			unit = {
				margin = { 6, 8, 2, 8 },
				radius = 8,
				padding = { 2, 10 },
				min_height = 34,
				font = sans(15),
				color = fg,
				background = alpha(fg, 0.07),
				border = { width = 1, color = hair },
				hover = { background = alpha(fg, 0.12), border = { width = 1, color = a } },
				active = { background = alpha(fg, 0.18), border = { width = 1, color = a } },
				checked = { background = alpha(fg, 0.18), border = { width = 1, color = a } },
			},
			-- The swap button turns the values over, and itself.
			swap = {
				margin = { 0, 0, 0, 10 },
				min_width = 36,
				min_height = 36,
				radius = 999,
				color = b_ink,
				background = alpha(b, 0.12),
				border = { width = 1, color = line },
				transition = { transform = { ms = 420, ease = "spring" }, background = 140 },
				hover = { background = alpha(b, 0.2), border = { width = 1, color = a } },
				active = { background = alpha(b, 0.2), border = { width = 1, color = a }, rotate = 180, scale = 0.92 },
			},
			caption = { font = sans(13), color = dim, margin = { 6, 12, 2, 12 } },
			chip = {
				padding = { 4, 10 },
				radius = 999,
				min_height = 0,
				font = sans(13),
				color = fg,
				background = alpha(fg, 0.07),
				border = { width = 1, color = alpha(b, 0.18) },
			},
			whimsy = { background = a, color = on_a, border = false },
			whimsy_icon = true,
			ratio = { font = sans(14), color = fg, margin = { 6, 12, 0, 12 } },
			timestamp = { font = sans(12), color = dim, margin = { 0, 0, 10, 12 } },
			link = { font = sans(12), padding = { 0, 6 }, min_height = 22, color = b_ink, hover = wash, active = wash_more },
		},

		graph = {
			divider = hair,
			row = { padding = { 3, 6 }, radius = 8 },
			row_error = { background = alpha(danger, 0.10) },
			error = { font = sans(12), color = danger },
			entry = { min_height = 36, radius = 8, font = sans(15), background = alpha(fg, 0.07) },
			row_button = icon_button(30, 16),
			swatch = {
				size = 18,
				hidden_opacity = 0.35,
				glow = { blur = 10, spread = -2 },
				active_scale = 0.8,
				transition = { opacity = 200, transform = { ms = 300, ease = "spring" } },
			},
			add = {
				min_height = 38,
				radius = 8,
				font = sans(15),
				color = dim,
				background = gmnb.transparent,
				border = { width = 1, color = alpha(b, 0.18) },
				hover = { color = fg, background = alpha(fg, 0.07), border = { width = 1, color = a } },
				active = { color = fg, background = alpha(fg, 0.12), border = { width = 1, color = a } },
			},
			variable = { font = { size = 16, weight = 600, italic = true }, color = fg },
			analysis = {
				title = { font = sans(17, 600), color = fg },
				card = { padding = { 8, 12 }, radius = 8, background = alpha(fg, 0.05), border = { width = 1, color = alpha(b, 0.18) } },
				card_title = { font = sans(12, 600, 0.04), color = dim },
				value = { font = sans(15), color = fg },
				text = { font = sans(13), color = soft },
				note = { font = sans(12), color = dim },
			},
			toolbar = { padding = 4, radius = 10, background = alpha(lift, 0.94), border = { width = 1, color = line }, shadow = { drop } },
			toolbar_button = icon_button(30, 16),
			canvas = {
				-- A panel, like the readout.
				background = alpha(ground, s.canvas),
				radius = RADIUS,
				border = { width = 1, color = line },
				grid_minor = alpha(fg, 0.05),
				grid_major = alpha(fg, 0.11),
				axis = { color = dim, width = 1.3 },
				label = { font = sans(11), color = dim },
				fill_opacity = 0.18,
				glow = { blur = 5, width = 3, opacity = 0.5 },
				trace = {
					hair_opacity = 0.5,
					hair_dash = { 4, 4 },
					halo = { grow = 3, color = "series", opacity = 0.3 },
					ring = { width = 1.5, color = fg },
					bubble = {
						background = lift,
						color = fg,
						radius = 8,
						font = sans(13),
						border = { width = 1, color = line },
						shadow = { drop },
					},
				},
			},
		},
	}
end

---------------------------------------------------------------------------
-- 4. The field
---------------------------------------------------------------------------

-- The picture thirty times a second, as on the page. The field five times
-- (the design's ten are for a browser's shader; at the design's pace the swirl
-- turns a hundredth of a radian between two of these, and gliding over
-- that is still the design's picture).
local FIELD_RATE = 5
local SHOWN_RATE = 30
local SHOWN_PER_FIELD = SHOWN_RATE / FIELD_RATE
-- How fast the design's swirl turns: its phase, in radians a second.
local TURN = 0.06
-- Which thirtieth of a second a layer's clock is in. (A frame may come
-- half a millisecond before it is due: it belongs to the thirtieth it
-- was drawn for.)
local function thirtieth(time: number): number
	return floor(time * SHOWN_RATE + 0.03)
end

-- How long a layer drawn at `time` waits for its next thirtieth to begin.
-- Never a whole thirtieth: the app would arm a plain timer for that,
-- which comes due just after every other frame of a 60 Hz screen as
-- often as just before it, and the paint would run at 20 pictures a
-- second half the time. Under a thirtieth it watches the screen's own
-- frames, and draws on the first one due.
local function until_next(time: number): number
	return clamp((thirtieth(time) + 1) / SHOWN_RATE - time, 0.001, 1 / SHOWN_RATE - 0.001)
end

-- About as many points as a 760 × 640 window has at one every 12 pixels:
-- a bigger window gets a coarser grid, not more to work out. (The bands
-- grow with the window, so they are as smooth in it.)
local POINTS = 3600
local FINEST = 12
-- And about as many pixels for the app to colour as that window has at
-- half size (`quality` 0.5, the design's own sharpness there): a bigger
-- window's picture is scaled up further, not coloured for longer. The
-- bands are soft-edged and grow with the window; it doesn't show.
local PIXELS = 160000

-- The bent coordinate (u, v) at every point of a cols × rows grid whose
-- corners are the corners of a width × height picture, as two 32-bit
-- floats a point. This is the design's shader, line for line. Only the
-- rows from `first` up to (not including) `last` are worked out.
local function swirl(buf: buffer, cols: number, rows: number, width: number, height: number, turned: number, first: number, last: number)
	local p = turned % TAU
	local px, py = 1.31 + p, 4.27 - p
	local sx, sy = 0.825 * width / height, 1.65
	local o = first * cols * 8
	for j = first, last - 1 do
		local v0 = (1 - 2 * j / (rows - 1)) * sy
		for i = 0, cols - 1 do
			local u = (2 * i / (cols - 1) - 1) * sx
			local v = v0
			local u2 = u + v
			local v2 = u2
			for _ = 1, 3 do
				local len = sqrt(u * u + v * v)
				u2 += u + len
				v2 += v + len
				u += 0.5 * cos(px + v2 * 0.2)
				v += 0.5 * sin(py + u2)
				local d = cos(u + v) - sin(u * 0.7 - v)
				u -= d
				v -= d
			end
			writef32(buf, o, u)
			writef32(buf, o + 4, v)
			o += 8
		end
	end
end

-- How much coarser than that this machine needs both: 1, unless it is
-- slow (`keep`, below, finds out).
local thin = 1

local function grid(width: number, height: number): (number, number)
	local step = max(FINEST, sqrt(width * height * thin / POINTS))
	return clamp(ceil(width / step) + 1, 2, 256), clamp(ceil(height / step) + 1, 2, 256)
end

-- How many pixels the app colours per pixel of the window, along a side.
local function sharpness(width: number, height: number): number
	return clamp(sqrt(PIXELS / thin / (width * height)), 0.1, 0.5)
end

-- Where the design's swirl stands before it has flowed at all.
local STILL = 0

-- How far the swirl has turned, in radians, since the theme was loaded:
-- where the later of the two snapshots in use is. It runs on while the
-- window is in use, at the `speed` chosen, and faster for a moment after
-- every press.
local turned = 0

-- What the background keeps between frames: two snapshots of the field a
-- fifth of a second apart, the one that comes after them (worked out a
-- few rows a frame, so that no frame carries a whole snapshot), which
-- thirtieth is shown, and room to bend both when something disturbs the
-- paint.
type Kept = {
	cols: number,
	rows: number,
	width: number,
	height: number,
	a: buffer,
	b: buffer,
	-- The next snapshot, whether it has been begun, how many of its rows
	-- are done, and how far turned each of the three is.
	c: buffer,
	begun: boolean,
	done: number,
	turn_a: number,
	turn_b: number,
	turn_c: number,
	bent_a: buffer,
	bent_b: buffer,
	dx: buffer,
	dy: buffer,
	-- The thirtieth shown, the fifth its snapshots are of, and how far
	-- between them it is.
	shown: number,
	tick: number,
	mix: number,
	-- The part of the grid something has pushed about this frame: columns
	-- left to right, rows top to bottom (none: right < left).
	left: number,
	right: number,
	top: number,
	bottom: number,
	-- Whether the snapshots handed over are the bent ones, and whether
	-- anything is still disturbing the paint.
	bent: boolean,
	stirred: boolean,
}
local kept: Kept? = nil

local function make(width: number, height: number): Kept
	local cols, rows = grid(width, height)
	local size = cols * rows * 8
	return {
		cols = cols,
		rows = rows,
		width = width,
		height = height,
		a = buffer.create(size),
		b = buffer.create(size),
		c = buffer.create(size),
		begun = false,
		done = 0,
		turn_a = turned,
		turn_b = turned,
		turn_c = turned,
		bent_a = buffer.create(size),
		bent_b = buffer.create(size),
		dx = buffer.create(size / 2),
		dy = buffer.create(size / 2),
		shown = -1,
		tick = -1,
		mix = 0,
		left = 0,
		right = -1,
		top = 0,
		bottom = -1,
		bent = false,
		stirred = false,
	}
end

-- A draw hook has 8 ms of the processor, and loses its place for good
-- after three frames that take longer. A whole snapshot takes 1.6 ms on
-- the machine this was written on. Where one would take over 5, the grid
-- and the picture are made as much coarser as brings it to 3.5, for as
-- long as the theme is loaded. The first nine rows tell: three batches of
-- three, of which the quickest counts (the machine may have been busy
-- with something else during the others).
local SLOW, AFFORDED = 0.005, 0.0035

-- What is kept for a picture of this size, and whether it was just made:
-- then with one whole snapshot, as far turned as the paint is, which is
-- both of its two until the next is ready.
local function keep(width: number, height: number): (Kept, boolean)
	local k = kept
	if k and k.width == width and k.height == height then
		return k, false
	end
	local made = make(width, height)
	local row, quickest = 0, math.huge
	while row + 3 <= min(made.rows, 9) do
		local started = os.clock()
		swirl(made.a, made.cols, made.rows, width, height, STILL + turned, row, row + 3)
		quickest = min(quickest, os.clock() - started)
		row += 3
	end
	local whole = quickest / 3 * made.rows
	if row > 0 and whole > SLOW and thin < 16 then
		thin = min(16, thin * whole / AFFORDED)
		made = make(width, height)
		row = 0
	end
	swirl(made.a, made.cols, made.rows, width, height, STILL + turned, row, made.rows)
	buffer.copy(made.b, 0, made.a)
	kept = made
	return made, true
end

-- A disturbance: something that pushes the paint about for a moment. All
-- of them move where the field is looked up, not the field: the bands are
-- dragged, and spring back.
--   a stir    turns the paint round a point, there and back, like a spoon
--   a ripple  pushes it outward in a ring that travels
local STIR_LIFE = 1.3
local RIPPLE_SPEED = 620
local RIPPLE_WIDTH = 100

-- A stir's turn, in radians at its middle, `age` seconds after it began:
-- a quick twist that swings back past rest and settles.
local function stir_turn(age: number): number
	return exp(-3.2 * age) * sin(TAU * 1.15 * age) * 2.6
end

-- The columns and rows a disturbance reaching `reach` pixels from
-- (cx, cy) can touch, which the kept box is grown to hold.
local function touch(k: Kept, sx: number, sy: number, cx: number, cy: number, reach: number): (number, number, number, number)
	local i0, i1 = max(0, floor((cx - reach) / sx)), min(k.cols - 1, ceil((cx + reach) / sx))
	local j0, j1 = max(0, floor((cy - reach) / sy)), min(k.rows - 1, ceil((cy + reach) / sy))
	if i1 >= i0 and j1 >= j0 then
		if k.right < k.left then
			k.left, k.right, k.top, k.bottom = i0, i1, j0, j1
		else
			k.left, k.right = min(k.left, i0), max(k.right, i1)
			k.top, k.bottom = min(k.top, j0), max(k.bottom, j1)
		end
	end
	return i0, i1, j0, j1
end

-- Adds a stir at (cx, cy), `reach` pixels wide, turned `angle` at its
-- middle, to the displacement of every point it touches.
local function add_stir(k: Kept, sx: number, sy: number, cx: number, cy: number, reach: number, angle: number)
	-- (Under a hundredth of a turn nothing is seen to move.)
	if math.abs(angle) < 0.03 then
		return
	end
	local cols = k.cols
	local i0, i1, j0, j1 = touch(k, sx, sy, cx, cy, reach)
	local dx, dy = k.dx, k.dy
	local rr = reach * reach
	for j = j0, j1 do
		local y = j * sy - cy
		for i = i0, i1 do
			local x = i * sx - cx
			local d = (x * x + y * y) / rr
			if d < 1 then
				local f = (1 - d) * (1 - d)
				local t = angle * f
				local c, s = cos(t) - 1, sin(t)
				local o = (j * cols + i) * 4
				writef32(dx, o, readf32(dx, o) + (c * x - s * y) / sx)
				writef32(dy, o, readf32(dy, o) + (s * x + c * y) / sy)
			end
		end
	end
end

-- Adds a ripple from (cx, cy), `age` seconds old, pushing by `push`
-- pixels at its crest.
local function add_ripple(k: Kept, sx: number, sy: number, cx: number, cy: number, age: number, push: number)
	if push < 0.25 then
		return
	end
	local cols = k.cols
	-- (It starts a ring already: there is no pushing outward from a point.)
	local crest = RIPPLE_WIDTH + RIPPLE_SPEED * age
	local outer = crest + RIPPLE_WIDTH
	local inner = crest - RIPPLE_WIDTH
	local i0, i1, j0, j1 = touch(k, sx, sy, cx, cy, outer)
	local dx, dy = k.dx, k.dy
	local oo, ii = outer * outer, inner * inner
	for j = j0, j1 do
		local y = j * sy - cy
		for i = i0, i1 do
			local x = i * sx - cx
			local d = x * x + y * y
			if d < oo and d > ii and d > 1 then
				local r = sqrt(d)
				local w = (r - crest) / RIPPLE_WIDTH
				local f = (1 - w * w)
				f = f * f * push / r
				-- (Looked up nearer the middle: the paint moves outward.)
				local o = (j * cols + i) * 4
				writef32(dx, o, readf32(dx, o) - f * x / sx)
				writef32(dy, o, readf32(dy, o) - f * y / sy)
			end
		end
	end
end

-- Both snapshots, looked up where the displacement says: inside the box
-- that was touched, each point of the bent copies is the snapshot's value
-- at the place the paint there came from.
local function bend(k: Kept)
	local cols, rows = k.cols, k.rows
	local a, b, bent_a, bent_b = k.a, k.b, k.bent_a, k.bent_b
	local dx, dy = k.dx, k.dy
	local right, bottom = cols - 1, rows - 1
	buffer.copy(bent_a, 0, a)
	buffer.copy(bent_b, 0, b)
	for j = k.top, k.bottom do
		local o = (j * cols + k.left) * 4
		for i = k.left, k.right do
			local x, y = readf32(dx, o), readf32(dy, o)
			-- (Moved by a third of a pixel or less: left where it is.)
			if x * x + y * y > 0.0008 then
				x = clamp(i + x, 0, right)
				y = clamp(j + y, 0, bottom)
				local i0, j0 = min(floor(x), right - 1), min(floor(y), bottom - 1)
				local tx, ty = x - i0, y - j0
				local p = (j0 * cols + i0) * 8
				local q = p + cols * 8
				local u0, v0 = readf32(a, p), readf32(a, p + 4)
				local u1, v1 = readf32(a, q), readf32(a, q + 4)
				u0 += (readf32(a, p + 8) - u0) * tx
				v0 += (readf32(a, p + 12) - v0) * tx
				u1 += (readf32(a, q + 8) - u1) * tx
				v1 += (readf32(a, q + 12) - v1) * tx
				writef32(bent_a, o * 2, u0 + (u1 - u0) * ty)
				writef32(bent_a, o * 2 + 4, v0 + (v1 - v0) * ty)
				u0, v0 = readf32(b, p), readf32(b, p + 4)
				u1, v1 = readf32(b, q), readf32(b, q + 4)
				u0 += (readf32(b, p + 8) - u0) * tx
				v0 += (readf32(b, p + 12) - v0) * tx
				u1 += (readf32(b, q + 8) - u1) * tx
				v1 += (readf32(b, q + 12) - v1) * tx
				writef32(bent_b, o * 2, u0 + (u1 - u0) * ty)
				writef32(bent_b, o * 2 + 4, v0 + (v1 - v0) * ty)
			end
			o += 4
		end
	end
end

-- What each event does to the paint, the newest first, for as long as
-- the frame has time left (`started`: when it began; on a machine that
-- gets through them all, which is any but a very slow one, nothing is
-- left out). Returns whether anything is still moving it.
local function disturb(k: Kept, events: { Event }, width: number, height: number, started: number): boolean
	local sx, sy = width / (k.cols - 1), height / (k.rows - 1)
	-- (Only what the last frame touched needs wiping.)
	if k.right >= k.left then
		local across = (k.right - k.left + 1) * 4
		for j = k.top, k.bottom do
			local o = (j * k.cols + k.left) * 4
			buffer.fill(k.dx, o, 0, across)
			buffer.fill(k.dy, o, 0, across)
		end
	end
	k.left, k.right, k.top, k.bottom = 0, -1, 0, -1
	local alive = false
	local turned_once = false
	for i = #events, 1, -1 do
		local e = events[i]
		local age = e.age
		if os.clock() - started > 0.004 then
			-- (What is left is older, and has less to say.)
			alive = true
			break
		end
		if e.kind == "press" then
			if age < STIR_LIFE then
				alive = true
				-- Each key turns its own way; the equals key stirs wide.
				local key = e.key
				local way = if key and key.id and key.id % 2 == 1 then -1 else 1
				local wide = key ~= nil and key.kind == "equals"
				local reach = if wide then 220 elseif e.source == "key" then 135 else 100
				add_stir(k, sx, sy, e.x, e.y, reach, way * stir_turn(age) * (if wide then 1.2 else 1))
			end
		elseif e.kind == "result" or e.kind == "error" then
			-- From under the last figures of the readout: a ripple for a
			-- result, a shudder for an error.
			local x, y = e.rect.x + e.rect.w * 0.8, e.rect.y + e.rect.h * 0.6
			if e.kind == "error" then
				if age < STIR_LIFE then
					alive = true
					add_stir(k, sx, sy, x, y, 260, exp(-4 * age) * sin(TAU * 4.5 * age) * 1.1)
				end
			elseif e.cause ~= "operator" and e.cause ~= "digit" then
				local life = sqrt(width * width + height * height) / RIPPLE_SPEED
				if age < life then
					alive = true
					-- It swells in two frames and wears out on its way.
					add_ripple(k, sx, sy, x, y, age, 64 * min(age / 0.07, 1) * (1 - age / life) ^ 1.5)
				end
			end
		elseif e.kind == "mode" or e.kind == "option" then
			-- A new page, or new colours: one slow turn of the whole window,
			-- for the newest of them only. (A colour or a slider being
			-- dragged is an option a step; the turns would pile up.)
			if age < STIR_LIFE and not turned_once then
				turned_once = true
				alive = true
				local reach = sqrt(width * width + height * height) * 0.6
				add_stir(k, sx, sy, width / 2, height / 2, reach, stir_turn(age * 0.8) * 0.5)
			end
		end
	end
	k.bent = k.right >= k.left
	if k.bent then
		bend(k)
	end
	return alive
end

local function number_option(ctx: Ctx, id: string, fallback: number): number
	local value = ctx.options[id]
	return if type(value) == "number" then value else fallback
end

local function background(draw: Draw, frame: Frame): Again
	local started = os.clock()
	local width, height = frame.width, frame.height
	if width < 2 or height < 2 then
		return nil
	end
	local ctx = frame.ctx
	local ramp = scheme(ctx).ramp
	local k, fresh = keep(width, height)
	local cols, rows = k.cols, k.rows

	if not frame.motion then
		-- A still picture: the paint as it stands.
		draw:field(0, 0, width, height, {
			data = k.a,
			cols = cols,
			rows = rows,
			channels = 2,
			ramp = ramp,
			scale = 1.25,
			quality = sharpness(width, height),
		})
		return nil
	end

	-- Which thirtieth of a second this is. Drawn again within the same one
	-- (an event came, the window's colours changed), the numbers are the
	-- same and the app has the picture already.
	local speed = number_option(ctx, "speed", 1)
	local stirring = ctx.options.stir ~= false
	local shown = thirtieth(frame.time)
	if fresh or shown ~= k.shown then
		local tick = floor(shown / SHOWN_PER_FIELD)
		-- The snapshot after the two in use is begun: as far on as the paint
		-- flows in a fifth of a second from the later of them. Typing
		-- hurries it: every press just made adds to its pace, and wears off
		-- in a second. (No further on at all: it is that one again.)
		local function begin()
			local hurry = 0
			if stirring then
				for _, e in frame.events do
					if e.kind == "press" then
						hurry += 2.5 * exp(-2.4 * e.age)
					elseif e.kind == "result" and e.cause ~= "operator" and e.cause ~= "digit" then
						hurry += 6 * exp(-2.4 * e.age)
					end
				end
			end
			k.turn_c = k.turn_b + TURN / FIELD_RATE * speed * (1 + min(hurry, 12))
			k.done = 0
			k.begun = true
			if k.turn_c == k.turn_b then
				buffer.copy(k.c, 0, k.b)
				k.done = rows
			end
		end
		if k.tick < 0 then
			k.tick = tick
		elseif tick ~= k.tick and k.begun then
			-- A new fifth of a second: the later snapshot becomes the earlier
			-- one, the one that was being worked out the later (what is left
			-- of it is done now, if frames were missed), and the next is begun.
			if k.done < rows then
				swirl(k.c, cols, rows, width, height, STILL + k.turn_c, k.done, rows)
			end
			k.a, k.b, k.c = k.b, k.c, k.a
			k.turn_a, k.turn_b = k.turn_b, k.turn_c
			turned = k.turn_b
			k.tick = tick
			k.begun = false
		end
		if not k.begun then
			begin()
		end
		-- A few rows of the next snapshot: all of it in five frames of six.
		if k.done < rows then
			local upto = min(rows, k.done + ceil(rows / (SHOWN_PER_FIELD - 1)))
			swirl(k.c, cols, rows, width, height, STILL + k.turn_c, k.done, upto)
			k.done = upto
		end
		k.mix = (shown - tick * SHOWN_PER_FIELD) / SHOWN_PER_FIELD
		k.shown = shown
		k.bent = false
		k.stirred = stirring and #frame.events > 0 and disturb(k, frame.events, width, height, started)
	end

	draw:field(0, 0, width, height, {
		data = if k.bent then k.bent_a else k.a,
		next = if k.bent then k.bent_b else k.b,
		mix = k.mix,
		cols = cols,
		rows = rows,
		channels = 2,
		ramp = ramp,
		scale = 1.25,
		quality = sharpness(width, height),
	})

	-- Again when the next thirtieth begins, for as long as anything moves:
	-- the paint is meant to flow, a glide is under way or to come, or
	-- something is disturbing it.
	if speed > 0 or k.turn_a ~= k.turn_b or k.turn_b ~= k.turn_c or k.stirred then
		return until_next(frame.time)
	end
	return nil
end

---------------------------------------------------------------------------
-- 5. Keypad: the keys swirl into place
---------------------------------------------------------------------------

local SWIRL_IN = 0.55
local SWIRL_STEP = 0.0009

local function newest(events: { Event }, kind: string): Event?
	for i = #events, 1, -1 do
		if events[i].kind == kind then
			return events[i]
		end
	end
	return nil
end

-- Whenever a keypad comes into view its keys come round into their
-- places, the ones nearest the middle first.
local function place(key: KeyCell, frame: Frame): Placement?
	local show = newest(frame.events, "show")
	if not show then
		return nil
	end
	local r = key.rect
	local x, y = r.x + r.w / 2 - frame.width / 2, r.y + r.h / 2 - frame.height / 2
	local away = sqrt(x * x + y * y)
	local p = clamp((show.age - away * SWIRL_STEP) / SWIRL_IN, 0, 1)
	if p >= 1 then
		return nil
	elseif p <= 0 then
		return { opacity = 0 }
	end
	local e = ease.out_cubic(p)
	local turn = (1 - e) * 0.9
	local c, s = cos(turn), sin(turn)
	local out = 1 + (1 - e) * 0.22
	return {
		dx = (c * x - s * y) * out - x,
		dy = (s * x + c * y) * out - y,
		scale = 0.7 + 0.3 * ease.out_back(p),
		opacity = e,
	}
end

local function keypad(_draw: Draw, frame: Frame): Again
	-- Nothing is drawn over the keys: their glass and the paint under it
	-- are the picture. `place` moves them for as long as the swirl-in runs.
	local show = newest(frame.events, "show")
	if show then
		local reach = sqrt(frame.width * frame.width + frame.height * frame.height) / 2
		if show.age <= reach * SWIRL_STEP + SWIRL_IN then
			return 0
		end
	end
	return nil
end

---------------------------------------------------------------------------
-- 6. Display: digits made of paint
---------------------------------------------------------------------------
--
-- The app lays the value out and hands over its glyphs; this paints them.
-- They are a window onto the swirl too: a slice of the same field, pushed
-- through the first band of the same ramp only, from the warm colour to
-- the cool one and back, so that no digit is ever written in the dark one.

-- How much of the swirl a readout's width shows, and how many times the
-- warm and cool colours change places across it.
local SLICE = 2.6
local WAVES = 2.4
-- The digits' paint runs this many times faster than the window's.
local DIGIT_FLOW = 7

-- One snapshot of a readout's slice: a number from 0 to 1 at every point.
local function slice(buf: buffer, cols: number, rows: number, width: number, height: number, turn: number)
	local p = turn % TAU
	local px, py = 1.31 + p, 4.27 - p
	local sy = SLICE * height / width
	local o = 0
	for j = 0, rows - 1 do
		local v0 = (1 - 2 * j / (rows - 1)) * sy + 0.4
		for i = 0, cols - 1 do
			local u = (2 * i / (cols - 1) - 1) * SLICE
			local v = v0
			local u2 = u + v
			local v2 = u2
			for _ = 1, 3 do
				local len = sqrt(u * u + v * v)
				u2 += u + len
				v2 += v + len
				u += 0.5 * cos(px + v2 * 0.2)
				v += 0.5 * sin(py + u2)
				local d = cos(u + v) - sin(u * 0.7 - v)
				u -= d
				v -= d
			end
			writef32(buf, o, 0.5 - 0.47 * cos(sqrt(u * u + v * v) * WAVES))
			o += 4
		end
	end
end

type Slice = { cols: number, rows: number, a: buffer, b: buffer, tick: number, shown: number, mix: number, turn: number }
-- By size: two readouts of one size (the converter's) show one slice.
local slices: { [number]: Slice } = {}
local slices_count = 0

-- How long each kind of change plays for, in seconds.
local function change_time(kind: string, digits: number): number
	if kind == "typing" then
		return 0.24
	elseif kind == "result" then
		return 0.45 + digits * 0.03 + 0.4
	elseif kind == "replace" then
		return 0.3
	end
	return 0.55 -- error
end

local function display(draw: Draw, frame: Frame): Again
	local v = frame.value
	if not v then
		return nil
	end
	local ctx = frame.ctx
	local s = scheme(ctx)
	if v.role == "calculator" then
		-- The panel's hairline.
		draw:border(0, 0, frame.width, frame.height, RADIUS, 1, alpha(s.b, 0.6))
	end
	local digits = #v.glyphs
	local change = v.change
	if change and change.age > change_time(change.kind, digits) then
		change = nil
	end
	local kind = if change then change.kind else "none"
	local t = if change then change.age else 0
	local from = if change then change.from else 1

	-- An error shudders, as the paint under it does.
	local shake = 0
	if kind == "error" then
		shake = exp(-4 * t) * sin(TAU * 4.5 * t) * 10
	end

	-- Replace: the old value sinks away.
	local old = v.previous
	if kind == "replace" and old then
		local p = min(t / 0.2, 1)
		draw:save()
		draw:translate(0, 10 * p)
		draw:push_opacity((1 - p) * 0.7)
		for _, g in old.glyphs do
			draw:glyph(g, s.fg)
		end
		draw:pop()
		draw:restore()
	end

	-- Where one glyph is in the change being played: how far above its
	-- place, how opaque, how large.
	local function pose(g: Glyph): (number, number, number)
		if kind == "typing" and g.index >= from then
			-- A new digit drops in.
			local p = min(t / 0.24, 1)
			return (1 - ease.out_back(p)) * v.size * 0.22, sqrt(p), 1
		elseif kind == "result" then
			-- The number wells up from the right, where the ripple starts:
			-- each figure there at once, and swelling to its size.
			local since = t - (digits - g.index) * 0.03
			return 0, clamp(since / 0.08, 0, 1), 0.6 + 0.4 * ease.out_back(clamp(since / 0.45, 0, 1))
		elseif kind == "replace" then
			local e = ease.out_cubic(min(t / 0.3, 1))
			return (1 - e) * 10, e, 1
		end
		return 0, 1, 1
	end

	local function glyphs(color: Rgba)
		for _, g in v.glyphs do
			local up, opacity, size = pose(g)
			if opacity > 0.001 then
				local cx, cy = g.x + g.w / 2, g.y + g.h / 2
				draw:save()
				draw:translate(cx + shake, cy - up)
				draw:scale(size)
				draw:translate(-cx, -cy)
				draw:glyph(g, alpha(color, opacity))
				draw:restore()
			end
		end
	end

	if digits == 0 then
		return nil
	end
	local band = v.band
	local living = frame.motion and number_option(ctx, "speed", 1) > 0
	if v.error or ctx.options.digits == false or band.w < 8 or band.h < 8 then
		-- An error is written in its own colour, plainly; and so is every
		-- value when the paint is kept out of the digits.
		if v.error then
			glyphs(s.danger)
		else
			draw:push_mask("alpha")
			glyphs(white)
			draw:pop()
			draw:rect(band.x, band.y, band.w, band.h, gmnb.linear({
				from = { 0, band.y },
				to = { 0, band.y + band.h },
				colors = { s.fg, mix(s.fg, s.b, 0.35) },
			}))
			draw:pop()
		end
		return if change then 0 else nil
	end

	-- The slice of the swirl behind this readout's digits: a point every
	-- eight pixels of the band the value sits in.
	local cols, rows = clamp(ceil(band.w / 8) + 1, 2, 160), clamp(ceil(band.h / 8) + 1, 2, 24)
	local key = cols * 32 + rows
	local k = slices[key]
	if not k then
		if slices_count >= 24 then
			slices, slices_count = {}, 0
		end
		k = { cols = cols, rows = rows, a = buffer.create(cols * rows * 4), b = buffer.create(cols * rows * 4), tick = -1, shown = -1, mix = 0, turn = 0 }
		slices[key] = k
		slices_count += 1
	end
	if not frame.motion then
		if k.tick ~= -2 then
			slice(k.a, cols, rows, band.w, band.h, STILL + turned * DIGIT_FLOW)
			k.tick = -2
		end
		k.mix = 0
	else
		local shown = thirtieth(frame.time)
		if k.tick < 0 or shown ~= k.shown then
			local tick = floor(shown / SHOWN_PER_FIELD)
			local turn = STILL + turned * DIGIT_FLOW
			if k.tick < 0 then
				slice(k.a, cols, rows, band.w, band.h, turn)
				buffer.copy(k.b, 0, k.a)
				k.turn = turn
				k.tick = tick
			elseif tick ~= k.tick then
				if turn ~= k.turn then
					k.a, k.b = k.b, k.a
					slice(k.b, cols, rows, band.w, band.h, turn)
					k.turn = turn
				end
				k.tick = tick
			end
			k.mix = (shown - tick * SHOWN_PER_FIELD) / SHOWN_PER_FIELD
			k.shown = shown
		end
	end

	draw:push_mask("alpha")
	glyphs(white)
	draw:pop()
	draw:field(band.x, band.y, band.w, band.h, {
		data = k.a,
		next = k.b,
		mix = k.mix,
		cols = cols,
		rows = rows,
		channels = 1,
		ramp = s.figure_ramp,
	})
	if kind == "result" then
		-- The figures land white-hot, and cool into their paint.
		local p = clamp(t / change_time("result", digits), 0, 1)
		draw:rect(band.x, band.y, band.w, band.h, alpha(white, 0.9 * (1 - p) ^ 2))
	elseif kind == "typing" then
		draw:rect(band.x, band.y, band.w, band.h, alpha(white, 0.3 * (1 - min(t / 0.24, 1))))
	end
	draw:pop()

	if change then
		return 0
	elseif living then
		return until_next(frame.time)
	end
	return nil
end

---------------------------------------------------------------------------
-- 7. Options
---------------------------------------------------------------------------

-- A tile's own small field: the swirl as it stands still, 96 × 56.
local TILE_COLS, TILE_ROWS = 17, 11
local tile_field = buffer.create(TILE_COLS * TILE_ROWS * 8)
local tile_made = false

local function tile_swirl(draw: Draw, w: number, h: number, ramp: Ramp)
	if not tile_made then
		swirl(tile_field, TILE_COLS, TILE_ROWS, 96, 56, STILL, 0, TILE_ROWS)
		tile_made = true
	end
	draw:field(0, 0, w, h, { data = tile_field, cols = TILE_COLS, rows = TILE_ROWS, channels = 2, ramp = ramp, scale = 1.25, quality = 1 })
end

-- A set of bands: its swirl, and a key of its glass with its equals sign.
-- (The bands alone, and their own ramp: a tile is drawn in a tenth of the
-- time a style may take, and needs no glass worked out.)
local function bands_tile(draw: Draw, frame: Preview, choice: string)
	local id = bands_id(choice)
	local set = bands_for(id, frame.ctx)
	local w, h = frame.width, frame.height
	draw:push_clip(0, 0, w, h, RADIUS)
	tile_swirl(draw, w, h, RAMPS[id] or ramp_of(set))
	draw:rounded_rect(w - 40, h - 30, 30, 20, 6, set.a)
	draw:rect(w - 31, h - 23, 12, 2, set.ground)
	draw:rect(w - 31, h - 19, 12, 2, set.ground)
	draw:pop()
end

-- A strength of glass: a key of it over the swirl.
local function glass_tile(draw: Draw, frame: Preview, choice: string)
	local s = scheme(frame.ctx)
	local w, h = frame.width, frame.height
	-- (As strong as it would be: the strength named, or what the text needs.)
	local strength = max(STRENGTH[choice] or SMOKED, s.least)
	draw:push_clip(0, 0, w, h, RADIUS)
	tile_swirl(draw, w, h, s.figure_ramp)
	draw:rounded_rect(14, 10, w - 28, h - 20, 8, alpha(s.lift, strength))
	draw:border(14, 10, w - 28, h - 20, 8, 1, alpha(s.b, 0.5))
	local tw, th = draw:text_size("7", { size = 20, weight = 500 })
	draw:text("7", (w - tw) / 2, (h - th) / 2, { size = 20, weight = 500 }, s.fg)
	draw:pop()
end

local function is(bands: string)
	return function(options: { [string]: OptionValue }, _ctx: Ctx): boolean
		return options.bands == bands
	end
end

local options: { Option } = {
	{
		id = "bands",
		type = "choice",
		group = "Paint",
		label = "Colours",
		description = "The three bands of the swirl, and what the keys borrow from them",
		default = "party",
		choices = {
			{ id = "party", label = "s0y b0y" },
			{ id = "lava", label = "Lava" },
			{ id = "lagoon", label = "Lagoon" },
			{ id = "orchid", label = "Orchid" },
			{ id = "absinthe", label = "Absinthe" },
			{ id = "marble", label = "Marble" },
			{ id = "system", label = "System" },
			{ id = "custom", label = "Freestyle" },
		},
		preview = bands_tile,
	},
	{
		id = "system_note",
		type = "note",
		group = "Paint",
		label = "Worked out from your desktop's accent colour (the freedesktop portal setting your desktop or shell publishes), and follows it live.",
		visible = is("system"),
	},
	{
		id = "paint_first",
		type = "color",
		group = "Paint",
		label = "First colour",
		description = "The = key, what is lit and the focus ring. Lightened if the figures need it",
		default = DEFAULT_FIRST,
		visible = is("custom"),
	},
	{
		id = "paint_second",
		type = "color",
		group = "Paint",
		label = "Second colour",
		description = "Lines, links and the operators' signs. Lightened if the figures need it",
		default = DEFAULT_SECOND,
		visible = is("custom"),
		actions = {
			{
				label = "Opposite",
				tooltip = "Use the colour opposite the first on the colour wheel",
				run = function(o: { [string]: OptionValue }, _ctx: Ctx): { [string]: OptionValue }
					return { paint_second = gmnb.to_hex(gmnb.complement(colour(o.paint_first, DEFAULT_FIRST))) }
				end,
			},
		},
	},
	{
		id = "paint_dark",
		type = "color",
		group = "Paint",
		label = "Dark colour",
		description = "What the two flow through, and the glass: its hue, kept near black",
		default = DEFAULT_DARK,
		visible = is("custom"),
	},
	{
		id = "glass",
		type = "choice",
		group = "Paint",
		label = "Glass",
		description = "How much of the paint shows through the keys. It is never clearer than the text on it can bear",
		default = "smoked",
		choices = {
			{ id = "clear", label = "Clear" },
			{ id = "smoked", label = "Smoked" },
			{ id = "panel", label = "Panels" },
		},
		preview = glass_tile,
	},
	{
		id = "speed",
		type = "slider",
		group = "Motion",
		label = "Flow",
		description = "How fast the paint moves: 1 is the own pace, 0 holds it still",
		min = 0,
		max = 4,
		step = 0.25,
		default = 1,
	},
	{
		id = "stir",
		type = "toggle",
		group = "Motion",
		label = "Stir with every press",
		description = "Keys turn the paint under them, typing hurries it along, a result sends a ripple out",
		default = true,
	},
	{
		id = "digits",
		type = "toggle",
		group = "Motion",
		label = "Digits made of paint",
		description = "The readout's numbers are cut out of the swirl. Off, they are plain",
		default = true,
	},
	{
		id = "motion_note",
		type = "note",
		group = "Motion",
		label = "Everything here stands still under the system's “reduce animations” setting, and rests when the window isn't in use.",
	},
}

---------------------------------------------------------------------------
-- 8. Sound
---------------------------------------------------------------------------
--
-- The calculator stands in paint, and sounds like it: a drop for every
-- key, a deeper one for the equals key, and for a result the rings that
-- spread from where it fell. Nothing here is a recording: every sound is
-- one recipe, a sine whose pitch runs up as it swells, the way a bubble's
-- does as it closes, with a speck of noise where it breaks the surface.
--
-- The drops are tuned. Number keys fall on the seven notes from F#5 to A6
-- of D's five-note scale, each key always on its own, so that typing is a
-- run of drops that never clash; and a result answers with the last three
-- of them, lowest first, ringing: the sum plays back what was typed.

-- A drop: a sine that runs from `from` to `to` hertz in `rise` seconds,
-- swelling all the while, and then rings out in `seconds`. It is loudest
-- as it arrives, so `to` is the note that is heard. (A soft start, with
-- no click of its own: the splash is the touch.)
local function drop(from: number, to: number, rise: number, seconds: number, more: SynthLayer?): SynthLayer
	local glide: PitchGlide = { from, to, time = rise }
	local layer: SynthLayer = { freq = glide, env = { attack = rise, decay = seconds } }
	if more then
		for key, value in more :: { [string]: any } do
			(layer :: { [string]: any })[key] = value
		end
	end
	return layer
end

-- Where it breaks the surface: a speck of noise around `freq` hertz.
local function splash(seed: number, freq: number, level: number, seconds: number, at: number?): SynthLayer
	return {
		wave = "noise",
		seed = seed,
		filter = { type = "bandpass", freq = freq, q = 1.1 },
		env = { attack = 0, decay = seconds },
		gain = level,
		at = at,
	}
end

-- A key's sound is dry, and has nothing low in it: a drop's thump is for
-- speakers that can carry it, and a keypad's are small.
local dry: { SynthEffect } = { { type = "highpass", freq = 200 } }

-- Number keys: a small drop that lands on A5, and a smaller one on D6.
-- Played faster or slower they are the scale's other notes.
local drop_a = gmnb.synth({
	gain = 0.24,
	layers = { drop(500, 880, 0.012, 0.048), splash(41, 2600, 0.07, 0.005) },
	effects = dry,
})
local drop_d = gmnb.synth({
	gain = 0.22,
	layers = { drop(680, 1174.66, 0.01, 0.042), splash(42, 3200, 0.06, 0.005) },
	effects = dry,
})

-- Operators: a rounder, heavier drop, on E5.
local plop = gmnb.synth({
	gain = 0.22,
	layers = { drop(360, 659.26, 0.014, 0.05), splash(43, 1500, 0.08, 0.008) },
	effects = dry,
})

-- Function keys and buttons: a bead, more tick than tone.
local bead = gmnb.synth({
	gain = 0.19,
	layers = { drop(1150, 1760, 0.006, 0.026), splash(44, 4200, 0.12, 0.004) },
	effects = dry,
})

-- The equals key: a drop from higher up. A low one on A4 goes under, a
-- small one on E6 comes back off the surface. Dry: the rings that follow
-- are the result's.
local plunge = gmnb.synth({
	gain = 0.25,
	layers = {
		drop(240, 440, 0.02, 0.11),
		drop(800, 1318.51, 0.012, 0.05, { at = 0.05, gain = 0.4 }),
		splash(45, 1900, 0.14, 0.014),
	},
	effects = dry,
})

-- 2nd and hyp: two beads, the second a fourth up.
local pair = gmnb.synth({
	gain = 0.18,
	layers = {
		drop(1150, 1760, 0.006, 0.024),
		drop(1500, 2349.32, 0.006, 0.026, { at = 0.045, gain = 0.8 }),
		splash(46, 4200, 0.1, 0.004),
	},
	effects = dry,
})

-- Backspace: a drop taken back. It starts on its note and falls away.
local back = gmnb.synth({
	gain = 0.22,
	layers = {
		drop(1174.66, 587.33, 0.03, 0.048, { env = { attack = 0.003, decay = 0.048 } }),
		splash(47, 2400, 0.06, 0.005),
	},
	effects = dry,
})

-- Clear: the glass wiped. Noise through a band that sweeps down, and a
-- thread of tone falling with it.
local wipe = gmnb.synth({
	gain = 0.24,
	layers = {
		{
			wave = "noise",
			seed = 48,
			filter = { type = "bandpass", freq = { 3600, 520, time = 0.1 } :: Glide, q = 1.4 },
			env = { attack = 0.012, decay = 0.11 },
		},
		drop(880, 330, 0.1, 0.1, { gain = 0.14, env = { attack = 0.01, decay = 0.1 } }),
	},
	effects = dry,
})

-- A ring spreading: the note a drop left, in a little room. On A5; a
-- result plays it on the notes that were typed.
local ring_fx: { SynthEffect } = {
	{ type = "highpass", freq = 240 },
	{ type = "delay", time = 0.11, feedback = 0.3, mix = 0.22, ping_pong = true },
	{ type = "reverb", size = 0.5, damp = 0.45, mix = 0.24 },
}
local ring = gmnb.synth({
	gain = 0.15,
	length = 1.1,
	layers = {
		drop(600, 880, 0.02, 0.55, { detune = -4, pan = -0.4 }),
		drop(600, 880, 0.02, 0.5, { detune = 4, pan = 0.4 }),
		-- Its octave, shorter: the glint on the ring.
		{ freq = 1760, env = { attack = 0.004, decay = 0.2 }, gain = 0.18 },
		splash(49, 3000, 0.05, 0.006),
	},
	effects = ring_fx,
})

-- An error: the sum goes down the drain. Two bubbles that fall instead
-- of rising, the second lower, as far apart as two swings of the
-- readout's shudder (4.5 a second there). Nothing under 120 Hz.
local glug_fx: { SynthEffect } = { { type = "highpass", freq = 150 } }
local glug = gmnb.synth({
	gain = 0.24,
	layers = {
		drop(560, 230, 0.11, 0.17, { wave = "triangle", env = { attack = 0.006, decay = 0.17 } }),
		splash(50, 700, 0.2, 0.02),
		drop(420, 170, 0.13, 0.2, { wave = "triangle", env = { attack = 0.006, decay = 0.2 }, at = 0.222, gain = 0.75 }),
		splash(51, 520, 0.15, 0.02, 0.222),
	},
	effects = glug_fx,
})

-- A change of page or mode: the paint stirred once round. Air through a
-- band that rises on one side and falls on the other, and four beads
-- carried round in it.
local stir_fx: { SynthEffect } = {
	{ type = "highpass", freq = 200 },
	{ type = "reverb", size = 0.45, damp = 0.5, mix = 0.2 },
}
local stirred = gmnb.synth({
	gain = 0.2,
	length = 0.85,
	layers = {
		{
			wave = "noise",
			seed = 52,
			filter = { type = "bandpass", freq = { 420, 2100, time = 0.24 } :: Glide, q = 1.3 },
			env = { attack = 0.09, decay = 0.26 },
			pan = -0.5,
			gain = 0.8,
		},
		{
			wave = "noise",
			seed = 53,
			filter = { type = "bandpass", freq = { 2100, 520, time = 0.3 } :: Glide, q = 1.3 },
			env = { attack = 0.1, decay = 0.3 },
			pan = 0.5,
			at = 0.16,
			gain = 0.7,
		},
		drop(560, 880, 0.01, 0.045, { at = 0.08, pan = -0.5, gain = 0.3 }),
		drop(740, 1174.66, 0.01, 0.045, { at = 0.17, pan = -0.1, gain = 0.3 }),
		drop(940, 1479.98, 0.01, 0.045, { at = 0.26, pan = 0.3, gain = 0.3 }),
		drop(740, 1174.66, 0.01, 0.045, { at = 0.36, pan = 0.6, gain = 0.25 }),
	},
	effects = stir_fx,
})

-- The scale: its seven notes, and for each the drop that plays it and at
-- what speed (a drop played slower is longer: none goes under 0.84, and
-- none lasts 80 ms), and the speed of the ring for it (the ring is on A5).
type Note = { drop: Sound, speed: number, ring: number }
local function note(name: string, sound: Sound, home: string): Note
	return { drop = sound, speed = gmnb.note(name) / gmnb.note(home), ring = gmnb.note(name) / gmnb.note("A5") }
end
local NOTES: { Note } = {
	note("F#5", drop_a, "A5"),
	note("A5", drop_a, "A5"),
	note("B5", drop_a, "A5"),
	note("D6", drop_d, "D6"),
	note("E6", drop_d, "D6"),
	note("F#6", drop_d, "D6"),
	note("A6", drop_d, "D6"),
}

-- What each set of bands sounds like poured in: which notes of the scale.
local POURS: { [string]: { number } } = {
	-- Fifths and fourths on A: open, bright.
	party = { 2, 4, 5, 7 },
	-- D major from its third: warm.
	lava = { 1, 2, 4, 6 },
	-- A, B, E, A: wide water.
	lagoon = { 2, 3, 5, 7 },
	-- B minor from its fifth.
	orchid = { 1, 3, 4, 6 },
	-- B minor with its seventh on top: tart.
	absinthe = { 3, 4, 6, 7 },
	-- Three notes, no third among them: stone.
	marble = { 2, 4, 7 },
	-- Fourths stacked up from the bottom: plain, as a desktop is.
	system = { 1, 3, 5, 7 },
	-- D major, the whole chord up from its root: paint of your own.
	custom = { 4, 6, 7 },
}

-- Which sound for which event. This table is the whole of it for most
-- events, and all of it if `sound` below is ever switched off.
local sounds: Sounds = {
	digit = { sound = { drop_a, drop_d }, pick = "key", rate = { 0.9, 1.12 }, pan = "position" },
	operator = { sound = plop, rate = { 0.985, 1.015 }, pan = "position" },
	func = { sound = bead, pick = "key", rate = { 0.92, 1.08 }, pan = "position" },
	equals = { sound = plunge, pan = "position" },
	toggle = { sound = pair, pan = "position" },
	clear = wipe,
	backspace = { sound = back, rate = { 0.97, 1.03 }, pan = "position" },
	-- The memory row: the operators' drop, a fourth up.
	memory = { sound = plop, rate = 1.335, volume = 0.9, pan = "position" },
	button = { sound = bead, volume = 0.9, pan = "position" },
	-- Any result, when `sound` isn't there to play what was typed: one ring.
	result = { sound = ring, rate = 1.335, volume = 0.8, delay = 0.06 },
	-- It starts as the readout reaches the end of its first swing.
	error = { sound = glug, delay = 0.03 },
	mode = stirred,
}

-- What the table can't know: which notes were typed. The last few number
-- keys' notes, newest last.
local typed: { number } = {}
-- What the last result rang, for a result that follows no typing (= again).
local rang: { number } = { 2, 4, 6 }
-- Number keys pressed so far.
local turn = 0

local function sound(event: SoundEvent, _ctx: Ctx): SoundChoice
	if event.kind == "press" then
		local key = event.key
		if key and key.kind == "number" and not event.action then
			-- The key's own note: next-door keys are a third or a fourth apart.
			local index = (key.id or 0) * 3 % #NOTES + 1
			local n = NOTES[index]
			table.insert(typed, index)
			if #typed > 6 then
				table.remove(typed, 1)
			end
			-- Never the same sound twice running, even on one key: a breath of
			-- difference in pitch (a tenth of a semitone at most) and in
			-- loudness.
			turn += 1
			local play: Play = {
				sound = n.drop,
				rate = n.speed * (0.994 + 0.012 * (turn * 0.618034 % 1)),
				volume = 0.86 + 0.14 * (turn * 0.414214 % 1),
				pan = event.pan * 0.6,
			}
			return play
		elseif event.action == "clear" then
			table.clear(typed)
		elseif event.action == "backspace" then
			table.remove(typed)
		end
		return nil
	elseif event.kind == "result" and event.page == "calculator" then
		if event.cause == "equals" then
			-- The rings: the last three different notes typed, lowest first,
			-- left to right, the first just after the equals key's own drop.
			local notes: { number } = {}
			for i = #typed, 1, -1 do
				if #notes < 3 and not table.find(notes, typed[i]) then
					table.insert(notes, typed[i])
				end
			end
			if #notes == 0 then
				-- = again: the same rings, each a step up the scale.
				for i, index in rang do
					notes[i] = index % #NOTES + 1
				end
			end
			table.sort(notes)
			rang = notes
			table.clear(typed)
			local rings: { Play } = {}
			for i, index in notes do
				rings[i] = {
					sound = ring,
					rate = NOTES[index].ring,
					volume = 0.62 + 0.1 * i,
					pan = (i - (#notes + 1) / 2) * 0.4,
					delay = 0.06 + 0.1 * (i - 1),
				}
			end
			return rings
		elseif event.cause == "function" then
			-- A function's result (sin, a square root): one ring, on the
			-- last note typed.
			local index = typed[#typed] or 4
			local one: Play = { sound = ring, rate = NOTES[index].ring, volume = 0.75, delay = 0.06 }
			return one
		end
		-- (The running total an operator shows: its own drop said it.)
		return nil
	elseif event.kind == "option" and event.id == "bands" then
		-- New paint poured in: its own few rings up the scale, left to
		-- right, while the window turns once.
		local value = event.value
		local pour = (if type(value) == "string" then POURS[value] else nil) or POURS.party
		local poured: { Play } = {}
		for i, index in pour do
			poured[i] = {
				sound = ring,
				rate = NOTES[index].ring,
				volume = 0.7,
				pan = (2 * (i - 1) / (#pour - 1) - 1) * 0.5,
				delay = 0.09 * (i - 1),
			}
		end
		return poured
	end
	return nil
end

local theme: Theme = {
	api = 1,
	name = "Swirls",
	description = "Flowing paint inspired by ly's ColorMix, under a calculator of dark glass. Keys stir it.",
	author = "GMNB contributors",
	license = "MIT",
	credits = { { title = "The swirl (ColorMix, from ly)", file = "credits/ColorMix.txt" } },
	options = options,
	style = style,
	sounds = sounds,
	sound = sound,
	layers = {
		background = { draw = background, fade = false, events = { press = STIR_LIFE, result = 2.5, error = STIR_LIFE, mode = STIR_LIFE, option = STIR_LIFE } },
		keypad = { draw = keypad, place = place, events = { show = 1.6 } },
		display = { draw = display },
	},
}

return theme
