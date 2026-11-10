--!strict
-- Aurora: the look GMNB had up to version 0.2, as a theme.
--
-- A drifting aurora behind glass keys, in seven palettes. Everything here
-- is written against theme API 1 (docs/theming.md); nothing in the app
-- knows this theme by name.
--
-- The file reads top to bottom:
--   1. Palettes        the colours of each palette, light and dark
--   2. Style           how every part of the window looks
--   3. Background      the aurora, its grain, vignette and pulses
--   4. Keypad          ripples, the light that follows the pointer, the cascade
--   5. Display         gradient digits, their glow and their animations
--   6. Options         what Settings offers, and the palette tiles
--
-- Numbers that look arbitrary are 0.2's own: this theme has to look the same.

local hex, hsl, mix, alpha = gmnb.hex, gmnb.hsl, gmnb.mix, gmnb.alpha
local white, black = gmnb.white, gmnb.black
local ease = gmnb.ease

---------------------------------------------------------------------------
-- 1. Palettes
---------------------------------------------------------------------------

type Scheme = {
	dark: boolean,
	-- The background's gradient, top to bottom.
	base_top: Rgba,
	base_bottom: Rgba,
	-- The four drifting blobs, and how strong they are.
	blobs: { Rgba },
	blob_alpha: number,
	fg: Rgba,
	-- Operators, focus, selection.
	accent: Rgba,
	-- The equals key's gradient, and the text on it.
	hot_a: Rgba,
	hot_b: Rgba,
	on_hot: Rgba,
	-- Graph colours.
	series: { Rgba },
}

local function list(codes: { string }): { Rgba }
	local out = {}
	for i, code in codes do
		out[i] = hex(code)
	end
	return out
end

local function palette(
	dark: boolean,
	base_top: string,
	base_bottom: string,
	blobs: { string },
	blob_alpha: number,
	fg: string,
	accent: string,
	hot_a: string,
	hot_b: string,
	on_hot: string,
	series: { string }
): Scheme
	return {
		dark = dark,
		base_top = hex(base_top),
		base_bottom = hex(base_bottom),
		blobs = list(blobs),
		blob_alpha = blob_alpha,
		fg = hex(fg),
		accent = hex(accent),
		hot_a = hex(hot_a),
		hot_b = hex(hot_b),
		on_hot = hex(on_hot),
		series = list(series),
	}
end

-- Called afresh each time, because `readable` below adjusts what it is given.
local BUILT_IN: { [string]: (dark: boolean) -> Scheme } = {
	aurora = function(dark)
		if dark then
			return palette(true, "#0b0820", "#150d33",
				{ "#7c4dff", "#ff4fb0", "#19d3c5", "#3d6bff" }, 0.42,
				"#f6f1ff", "#b79bff", "#ff7ad9", "#8a63ff", "#ffffff",
				{ "#5ef2d6", "#ff7ad9", "#ffd36e", "#8fb4ff", "#b79bff", "#ff8f6b" })
		end
		return palette(false, "#fbf8ff", "#f1ebff",
			{ "#c6b1ff", "#ffbfd9", "#9eeedc", "#b4caff" }, 0.62,
			"#1f1640", "#6d3fe8", "#f05bbf", "#7448f0", "#ffffff",
			{ "#0fa38e", "#d6339a", "#c98a00", "#3b6fe0", "#6d3fe8", "#e0582e" })
	end,
	ember = function(dark)
		if dark then
			return palette(true, "#170708", "#241016",
				{ "#ff6a3d", "#ff3d7f", "#ffb13d", "#a13dff" }, 0.36,
				"#fff3ee", "#ff9a6e", "#ffc35a", "#ff4f6e", "#2a0b10",
				{ "#ffc35a", "#ff6a8d", "#7ee0ff", "#c69bff", "#ff9a6e", "#9cf08a" })
		end
		return palette(false, "#fffaf6", "#fff0ea",
			{ "#ffcfb3", "#ffc0d4", "#ffe39e", "#e6c9ff" }, 0.65,
			"#3a1416", "#d94a26", "#f7a21b", "#e6365b", "#ffffff",
			{ "#d97a00", "#d6335e", "#0f8fb8", "#8a4fe0", "#d94a26", "#2f9e3a" })
	end,
	abyss = function(dark)
		if dark then
			return palette(true, "#020c16", "#06182b",
				{ "#00a6ff", "#00e0b0", "#3d4dff", "#7a3dff" }, 0.38,
				"#eaf8ff", "#55d6ff", "#3af0d0", "#3a7bff", "#02121f",
				{ "#3af0d0", "#7ab8ff", "#ffd36e", "#ff7ad9", "#b79bff", "#a4f07a" })
		end
		return palette(false, "#f5fbff", "#e9f5ff",
			{ "#b3e3ff", "#a9f2e2", "#c4ccff", "#d8c8ff" }, 0.66,
			"#0b2236", "#0a7fc2", "#10b9a0", "#2563eb", "#ffffff",
			{ "#0a9e86", "#2563eb", "#c98a00", "#d6339a", "#6d3fe8", "#3f8f1f" })
	end,
	orchard = function(dark)
		if dark then
			return palette(true, "#07100a", "#0e1c12",
				{ "#4fd16a", "#c6e53f", "#22b3a0", "#ffc94a" }, 0.32,
				"#f1fbec", "#9be37a", "#e8f05a", "#35c98a", "#0b1a0e",
				{ "#e8f05a", "#5ee0c0", "#ff9a6e", "#8fb4ff", "#ff7ad9", "#9be37a" })
		end
		return palette(false, "#f9fdf4", "#eef8e6",
			{ "#c2efb8", "#e6f3a6", "#a8eadf", "#ffe3a6" }, 0.66,
			"#142a12", "#2f8f2a", "#b7c71a", "#1d9f6a", "#ffffff",
			{ "#2f8f2a", "#0f8f80", "#d2691e", "#3b6fe0", "#c0338f", "#8a6d00" })
	end,
	graphite = function(dark)
		if dark then
			return palette(true, "#0c0d10", "#15171c",
				{ "#5b6474", "#8b93a3", "#3c4250", "#c9ced8" }, 0.20,
				"#f2f3f5", "#dfe3ea", "#f2f3f5", "#aab1bf", "#111318",
				{ "#f2f3f5", "#9fb7ff", "#ffc58a", "#8ae0c8", "#ff9ac6", "#d5ff8a" })
		end
		return palette(false, "#fafafa", "#eff0f2",
			{ "#d9dce2", "#c8ccd4", "#e7e9ee", "#bfc4cd" }, 0.6,
			"#16181d", "#2c313a", "#2c313a", "#4a515e", "#ffffff",
			{ "#2c313a", "#2563eb", "#d2691e", "#0f8f80", "#c0338f", "#6b8e00" })
	end,
}

local DEFAULT_PRIMARY = "#7c4dff"
local DEFAULT_SECONDARY = "#ff6fb5"
-- What System uses on a desktop that shares no accent colour.
local FALLBACK_ACCENT = "#3584e4"

-- Keep what is drawn on top legible: the equals glyph against its whole
-- gradient (a large glyph: 3:1), accent text against the background
-- (4.5:1) and graph lines against it (3:1). Generated palettes can come
-- from anything (a neon-yellow desktop accent, pure grey...) and also pick
-- light or `dark_text` for the glyph; the built-in ones keep their glyph
-- colour and only have their colours nudged.
local function readable(s: Scheme, dark_text: Rgba?): Scheme
	local function fix(c: Rgba, target: number): Rgba
		return gmnb.with_contrast(gmnb.with_contrast(c, s.base_top, target), s.base_bottom, target)
	end
	if dark_text then
		s.on_hot, s.hot_a, s.hot_b = gmnb.readable_gradient(s.hot_a, s.hot_b, white, dark_text, gmnb.UI_CONTRAST)
	else
		s.hot_a, s.hot_b = gmnb.gradient_for_text(s.on_hot, s.hot_a, s.hot_b, gmnb.UI_CONTRAST)
	end
	s.accent = fix(s.accent, gmnb.TEXT_CONTRAST)
	for i, c in s.series do
		s.series[i] = fix(c, gmnb.UI_CONTRAST)
	end
	return s
end

-- A complete scheme from two colours: a tinted base, an aurora of the two
-- hues and their neighbours, readable text and accent, a hot gradient
-- running secondary to primary, and six well-spread graph colours.
local function generate(primary: Rgba, secondary: Rgba, dark: boolean): Scheme
	local ph, ps = gmnb.to_hsl(primary)
	local qh, qs = gmnb.to_hsl(secondary)
	-- Keep greys grey, but give real colours some life.
	local function vivid(sat: number): number
		return if sat < 0.08 then sat else math.clamp(sat, 0.55, 1.0)
	end
	local pv, qv = vivid(ps), vivid(qs)
	local hues = { ph, ph + 180, ph + 60, ph + 240, ph + 120, ph + 300 }
	local series = {}
	local s: Scheme
	if dark then
		for i, h in hues do
			series[i] = hsl(h, 0.8, 0.66)
		end
		s = {
			dark = true,
			base_top = hsl(ph, math.min(ps * 0.7, 0.55), 0.055),
			base_bottom = hsl(ph, math.min(ps * 0.75, 0.6), 0.105),
			blobs = { hsl(ph, pv, 0.58), hsl(qh, qv, 0.60), hsl(ph + 35, pv, 0.55), hsl(qh - 35, qv, 0.58) },
			blob_alpha = 0.40,
			fg = hsl(ph, math.min(ps, 0.5), 0.965),
			accent = hsl(ph, pv, 0.76),
			hot_a = hsl(qh, qv, 0.66),
			hot_b = hsl(ph, pv, 0.60),
			on_hot = white,
			series = series,
		}
	else
		for i, h in hues do
			series[i] = hsl(h, 0.7, 0.42)
		end
		s = {
			dark = false,
			base_top = hsl(ph, math.min(ps, 0.6), 0.985),
			base_bottom = hsl(ph, math.min(ps, 0.55), 0.955),
			blobs = {
				hsl(ph, math.min(pv, 0.8), 0.83),
				hsl(qh, math.min(qv, 0.8), 0.84),
				hsl(ph + 35, math.min(pv, 0.8), 0.85),
				hsl(qh - 35, math.min(qv, 0.8), 0.83),
			},
			blob_alpha = 0.62,
			fg = hsl(ph, math.min(ps, 0.45), 0.15),
			accent = hsl(ph, math.min(pv, 0.9), 0.42),
			hot_a = hsl(qh, math.min(qv, 0.8), 0.58),
			hot_b = hsl(ph, math.min(pv, 0.75), 0.50),
			on_hot = white,
			series = series,
		}
	end
	return readable(s, hsl(ph, 0.4, 0.10))
end

-- A colour option's saved text, or `fallback` if someone edited it into nonsense.
local function colour(value: OptionValue?, fallback: string): Rgba
	if type(value) == "string" and (#value == 7 or #value == 9) and string.match(value, "^#%x+$") then
		return hex(value)
	end
	return hex(fallback)
end

local function build(id: string, ctx: Ctx): Scheme
	if id == "system" then
		local p = ctx.accent or hex(FALLBACK_ACCENT)
		return generate(p, gmnb.complement(p), ctx.dark)
	elseif id == "custom" then
		return generate(
			colour(ctx.options.primary, DEFAULT_PRIMARY),
			colour(ctx.options.secondary, DEFAULT_SECONDARY),
			ctx.dark
		)
	end
	local make = BUILT_IN[id] or BUILT_IN.aurora
	return readable(make(ctx.dark), nil)
end

-- Building a generated palette takes a few hundred contrast checks, and the
-- draw hooks ask for the scheme every frame: remember what was built.
local remembered: { [string]: Scheme } = {}
local remembered_count = 0

local function scheme_for(id: string, ctx: Ctx): Scheme
	local key = table.concat({
		id,
		tostring(ctx.dark),
		if ctx.accent then gmnb.to_hex(ctx.accent) else "-",
		tostring(ctx.options.primary),
		tostring(ctx.options.secondary),
	}, "|")
	local s = remembered[key]
	if not s then
		-- Dragging through the colour chooser makes a new palette each step.
		if remembered_count >= 32 then
			remembered, remembered_count = {}, 0
		end
		s = build(id, ctx)
		remembered[key] = s
		remembered_count += 1
	end
	return s
end

local function scheme(ctx: Ctx): Scheme
	return scheme_for(tostring(ctx.options.palette), ctx)
end

---------------------------------------------------------------------------
-- 2. Style
---------------------------------------------------------------------------

-- The keys' corner radius: the ripples and the pointer's light follow it.
local KEY_RADIUS = 14

local function outfit(size: number, weight: number?, spacing: number?): Font
	return { family = "Outfit", size = size, weight = weight or 400, spacing = spacing or 0 }
end

local function down(top: Color, bottom: Color): Gradient
	return gmnb.linear({ angle = 180, colors = { top, bottom } })
end

local function across(left: Color, right: Color): Gradient
	return gmnb.linear({ angle = 90, colors = { left, right } })
end

local function slant(a: Color, b: Color): Gradient
	return gmnb.linear({ angle = 135, colors = { a, b } })
end

local function style(ctx: Ctx): Style
	local s = scheme(ctx)
	local dark = s.dark
	local fg, accent = s.fg, s.accent
	-- Below full opacity the desktop shows through the aurora: what must
	-- stay readable gets a layer of the backdrop's own colour under it.
	local see = ctx.opacity < 1
	local base = s.base_bottom
	-- 0.2 left whatever it didn't set in Outfit to the desktop's own font.
	local system = ctx.font

	-- Glass.
	local glass = alpha(white, if dark then 0.055 else 0.50)
	local glass_hi = alpha(white, if dark then 0.11 else 0.78)
	local glass_border = alpha(white, if dark then 0.10 else 0.85)
	local glass_num = alpha(white, if dark then 0.085 else 0.72)
	local shade = if dark then alpha(black, 0.32) else alpha(hex("#3a2a6a"), 0.10)
	local highlight: Shadow = { inset = true, y = 1, color = alpha(white, 0.10) }
	local rim: Shadow = { inset = true, spread = 1, color = glass_border }

	-- A gradient, over the see-through tint when there is one.
	local function over_tint(g: Gradient, tint: number): Fill
		local layers: { Paint } = { g, alpha(base, tint) }
		return if see then layers else g
	end

	local hot = slant(s.hot_a, s.hot_b)
	local hot_soft = slant(alpha(s.hot_a, 0.85), alpha(s.hot_b, 0.85))
	local halo: { Shadow } = if see
		then { { blur = 3, color = alpha(base, 0.9) }, { blur = 10, color = alpha(base, 0.75) } }
		else {}
	local display_base = if dark then s.base_bottom else s.base_top

	-- What the toolkit's own widgets are coloured with: dialogs, menus,
	-- lists, cards and the text on the accent colour.
	local window = if dark then s.base_bottom else s.base_top
	local view, sidebar, dialog, popover, card, on_accent
	if dark then
		view = mix(window, black, 0.15)
		sidebar = mix(window, white, 0.03)
		dialog = mix(mix(window, accent, 0.06), white, 0.05)
		popover = mix(mix(window, accent, 0.08), white, 0.07)
		card = alpha(white, 0.07)
		on_accent = gmnb.rgb(20 / 255, 12 / 255, 40 / 255)
	else
		view = mix(window, white, 0.6)
		sidebar = mix(window, accent, 0.04)
		dialog = mix(window, white, 0.7)
		popover = mix(window, white, 0.8)
		card = alpha(white, 0.75)
		on_accent = white
	end

	-- A flat button as the toolkit drew it in 0.2. (A state a theme gives
	-- replaces the base look's whole: these have no overlay.)
	local flat_hover: State = { background = alpha(fg, 0.07) }
	local flat_active: State = { background = alpha(fg, 0.16) }

	-- Shadows, listed topmost first: a highlight along the top edge, a
	-- hairline rim, and a soft drop shadow underneath.
	local key_shadow: { Shadow } = { highlight, rim, { y = 6, blur = 18, spread = -12, color = shade } }
	local operator_shadow: { Shadow } = {
		highlight,
		{ inset = true, spread = 1, color = alpha(accent, 0.22) },
		{ y = 6, blur = 18, spread = -12, color = shade },
	}
	-- The equals key glows in its own colours.
	local equals_shadow: { Shadow } = {
		{ inset = true, y = 1, color = alpha(white, 0.35) },
		{ y = 12, blur = 32, spread = -12, color = alpha(s.hot_b, 0.85) },
		{ y = 2, blur = 10, spread = -4, color = alpha(s.hot_a, 0.6) },
	}
	local bit_glow: { Shadow } = {
		{ blur = 14, spread = -2, color = alpha(s.hot_b, 0.75) },
		{ inset = true, y = 1, color = alpha(white, 0.3) },
	}
	local panel_shadow: { Shadow } = { highlight, rim, { y = 18, blur = 50, spread = -28, color = shade } }
	local flyout_shadow: { Shadow } = { rim, { y = 18, blur = 48, spread = -16, color = alpha(black, 0.5) } }
	local toolbar_shadow: { Shadow } = { rim, { y = 8, blur = 24, spread = -10, color = alpha(black, 0.4) } }
	local panel_fill = over_tint(down(glass_hi, glass), 0.55)

	-- The memory row's buttons, DEG, F-E and their like: text that lights up.
	local function text_button(padding: Spacing): Control
		return {
			min_width = 30,
			min_height = 30,
			padding = padding,
			radius = 10,
			font = outfit(13, 600, 0.04),
			color = alpha(fg, 0.78),
			background = if see then alpha(base, 0.45) else gmnb.transparent,
			transition = { background = 150, color = 150 },
			hover = { background = if see then alpha(mix(base, fg, 0.12), 0.6) else alpha(fg, 0.07), color = fg },
			active = { background = if see then alpha(mix(base, fg, 0.12), 0.6) else alpha(fg, 0.07), color = fg },
			checked = {
				background = if see then alpha(mix(base, accent, 0.2), 0.6) else alpha(accent, 0.12),
				color = accent,
			},
			disabled = { color = alpha(fg, 0.26) },
		}
	end

	return {
		-- Tokens. Anything not styled below is derived from these.
		colors = {
			bg = base,
			surface = popover,
			surface2 = alpha(fg, 0.06),
			border = alpha(fg, 0.12),
			fg = fg,
			fg_dim = alpha(fg, 0.62),
			fg_faint = alpha(fg, 0.38),
			accent = accent,
			on_accent = on_accent,
			accent_text = accent,
			hover = alpha(fg, 0.06),
			press = alpha(fg, 0.12),
			danger = s.hot_a,
			series = s.series,
		},
		fonts = { ui = system.family, display = "Outfit" },
		text = {
			body = { size = system.size, weight = 400 },
			strong = { size = system.size, weight = 700 },
			small = { size = 13, weight = 400 },
			caption = { size = 12, weight = 400 },
			key = outfit(17),
			title = outfit(19, 500, 0.01),
		},
		shape = { radius = 12, row_radius = 12, card_radius = 20 },
		icons = { stroke = 1.75, taper = 0.6 },
		motion = {
			page = 220,
			mode = 180,
			panel = 220,
			flip = 240,
			graph_view = 200,
			graph_side = 240,
			date = 260,
			history_row = 280,
			equation_row = 260,
			icon_draw = 700,
			graph_draw = 900,
			graph_zoom = 240,
		},
		layout = {
			calculator = { margin = { 0, 8, 8, 8 }, gap = 6, panel_width = 300, panel_margin = { 0, 10, 10, 0 } },
			converter = { margin = { 0, 12, 10, 12 }, gap = 10, gap_wide = 24 },
			date = { margin = { 4, 16, 16, 16 }, gap = 14 },
			graph = { margin = { 0, 10, 10, 10 }, gap = 10, side_width = 340 },
		},
		-- Drawn by the toolkit, as in 0.2, in the `stock` colours.
		native = { "segmented", "slider", "switch", "scrollbar", "menu", "toast", "tooltip", "button", "card", "focus" },
		stock = {
			window = window,
			view = view,
			headerbar = window,
			sidebar = sidebar,
			dialog = dialog,
			popover = popover,
			card = card,
			fg = fg,
			accent = accent,
			accent_bg = accent,
			accent_fg = on_accent,
		},

		window = {
			background = base,
			color = fg,
			font = { size = system.size, weight = 400 },
			text_halo = halo,
			raised_text_halo = halo,
		},
		plain = { background = gmnb.transparent, radius = 0 },
		header = {
			background = if see then alpha(base, 0.55) else gmnb.transparent,
			height = 47,
			title = { font = outfit(19, 500, 0.01), color = fg },
			button = {
				min_width = 34,
				min_height = 34,
				radius = 10,
				color = fg,
				icon = { size = 18 },
				hover = flat_hover,
				active = flat_active,
				checked = { background = alpha(fg, 0.10), color = fg },
			},
			-- The toolkit's round grey window buttons.
			controls = {
				min_width = 24,
				min_height = 24,
				radius = 999,
				margin = { 0, 5 },
				background = alpha(fg, 0.10),
				color = fg,
				icon = { size = 16 },
				hover = { background = alpha(fg, 0.15) },
				active = { background = alpha(fg, 0.30) },
			},
		},
		nav = {
			background = down(alpha(base, 0.94), alpha(base, 0.88)),
			shadow = { { inset = true, x = -1, color = alpha(fg, 0.08) } },
			width = 300,
			show_title = false,
			header = { font = outfit(12, 600, 0.12), color = alpha(fg, 0.5), margin = { 14, 18, 4, 18 } },
			uppercase_headers = true,
			row = {
				radius = 12,
				margin = { 1, 8 },
				padding = { 9, 12 },
				min_height = 0,
				font = { size = 15, weight = 400 },
				color = fg,
				icon = { size = 20, color = alpha(fg, 0.8) },
				transition = { background = 160 },
				hover = { background = alpha(fg, 0.06) },
				active = { background = alpha(fg, 0.06) },
				checked = {
					background = across(alpha(accent, 0.24), alpha(accent, 0.06)),
					shadow = { { inset = true, x = 3, color = accent } },
					color = fg,
					icon = { color = accent },
				},
			},
			settings = { padding = { 10, 12 }, radius = 12, min_height = 0, hover = flat_hover, active = flat_active },
		},

		display = {
			background = if see then alpha(base, 0.7) else gmnb.transparent,
			radius = if see then 18 else 0,
			padding_x = 14,
			heights = { standard = "auto", scientific = "auto", programmer = "auto", compact = "auto" },
			sizes = { standard = 64, scientific = 54, programmer = 46, compact = 44 },
			min_size = 18,
			value = {
				font = { family = "Outfit", weight = 250, features = { "tnum", "lnum" } },
				-- The display layer below paints the value; these are what it
				-- would be without it.
				color = fg,
				error_color = s.hot_a,
				error_scale = 0.62,
				glow = false,
			},
			expression = { font = outfit(15), color = alpha(fg, if see then 0.8 else 0.55) },
			expression_min_size = 15,
			halo = if see
				then { { blur = 3, color = alpha(display_base, 0.9) }, { blur = 10, color = alpha(display_base, 0.75) } }
				else {},
		},

		keys = {
			gap = 5,
			gap_compact = 3,
			radius = KEY_RADIUS,
			min_width = 40,
			min_height = 38,
			padding = { 0, 2 },
			icon = { size = 22, stroke = 1.9 * (0.6 + 0.4 * 22 / 24) },
			transition = { background = 160, shadow = 220, transform = { ms = 320, ease = "spring" }, filter = 160 },
			hover = { brightness = 1.12 },
			active = { scale = 0.93, brightness = 1.22, transition = { transform = 70, filter = 70 } },
			disabled = { opacity = 0.32, saturation = 0.4 },
			number = {
				font = outfit(22, 450),
				color = fg,
				background = over_tint(down(glass_hi, glass_num), 0.7),
				border = false,
				shadow = key_shadow,
			},
			func = {
				font = outfit(16),
				color = alpha(fg, 0.86),
				background = over_tint(down(glass_hi, glass), 0.7),
				border = false,
				shadow = key_shadow,
			},
			operator = {
				font = outfit(22, 350),
				color = if see then mix(accent, fg, 0.5) else accent,
				background = over_tint(down(alpha(accent, 0.20), alpha(accent, 0.07)), 0.7),
				border = false,
				shadow = operator_shadow,
			},
			equals = {
				font = outfit(26),
				color = s.on_hot,
				background = hot,
				border = false,
				icon = { stroke = 2.3 * (0.6 + 0.4 * 22 / 24) },
				shadow = equals_shadow,
			},
			on = { color = s.on_hot, background = over_tint(hot_soft, 0.7) },
			pad = { gap = 4, radius = 10, min_height = 32, font = { size = 15 } },
		},

		text_button = text_button({ 0, 6 }),
		text_button_gap = 2,
		flyout_button = text_button({ 0, 10 }),
		icon_button = {
			min_width = 34,
			min_height = 34,
			radius = 10,
			color = fg,
			icon = { size = 18 },
			hover = flat_hover,
			active = flat_active,
			checked = { background = alpha(accent, 0.12), color = accent },
			disabled = { opacity = 0.5 },
		},
		segmented = { item = { font = { family = "Outfit", weight = 500 } } },
		field = {
			background = alpha(fg, 0.10),
			color = fg,
			border = false,
			radius = 12,
			font = outfit(16),
			min_height = 34,
			padding = { 0, 9 },
			placeholder = alpha(fg, 0.5),
			selection = alpha(accent, 0.3),
			caret = fg,
			focus = { border = { width = 2, color = alpha(accent, 0.5) } },
			error = { border = { width = 2, color = alpha(s.hot_a, 0.5) }, color = s.hot_a },
		},
		row = { radius = 9, hover = flat_hover, active = flat_active, checked = { background = alpha(fg, 0.10) } },
		section = { font = outfit(13, 600, 0.04), color = alpha(fg, 0.6), margin = { 6, 0, 0, 0 } },
		popover = {
			padding = 8,
			radius = 18,
			background = down(alpha(base, 0.98), alpha(base, 0.95)),
			border = false,
			shadow = flyout_shadow,
		},

		panel = {
			background = panel_fill,
			radius = 20,
			border = false,
			shadow = panel_shadow,
			padding = { 10, 6, 6, 6 },
			sheet = { background = down(alpha(base, 0.97), alpha(base, 0.94)) },
			tabs = { stretch = false },
			title = { font = outfit(15, 600), color = fg, margin = { 6, 10, 2, 10 } },
			row = {
				radius = 14,
				margin = { 2, 4 },
				padding = { 8, 12 },
				transition = { background = 150 },
				hover = { background = alpha(fg, 0.06) },
				active = { background = alpha(fg, 0.06) },
			},
			expression = { font = outfit(13), color = alpha(fg, 0.55) },
			result = { font = outfit(24, 350), color = fg },
			delete = {
				min_width = 22,
				min_height = 22,
				radius = 999,
				color = alpha(fg, 0.78),
				hover = flat_hover,
				active = flat_active,
				transition = { opacity = 160 },
			},
			delete_reveal = "hover",
			memory_button = {
				min_height = 24,
				padding = { 0, 8 },
				radius = 10,
				font = outfit(11, 600, 0.04),
				color = alpha(fg, 0.78),
				background = alpha(fg, 0.06),
				hover = { background = alpha(fg, 0.07), color = fg },
				active = { background = alpha(fg, 0.07), color = fg },
				transition = { background = 150, color = 150, opacity = 160 },
			},
			memory_reveal = "hover",
			empty = { font = { size = 14 }, color = alpha(fg, 0.5) },
		},

		programmer = {
			radix = {
				row = {
					radius = 10,
					padding = { 3, 10 },
					min_height = 26,
					transition = { background = 180, shadow = 220 },
					hover = { background = alpha(fg, 0.06) },
					active = { background = alpha(fg, 0.06) },
					checked = {
						background = across(alpha(accent, 0.22), alpha(accent, 0)),
						shadow = { { inset = true, x = 3, color = accent } },
					},
				},
				name = { font = outfit(12, 650, 0.08), color = alpha(fg, 0.6) },
				name_checked = accent,
				value = { font = { family = "Outfit", size = 15, weight = 400, features = { "tnum" } }, color = fg },
			},
			bit = {
				min_width = 14,
				min_height = 30,
				padding = 0,
				radius = 8,
				font = outfit(15),
				color = alpha(fg, 0.55),
				background = alpha(fg, 0.04),
				shadow = { { inset = true, spread = 1, color = alpha(fg, 0.06) } },
				transition = { background = 200, color = 200, shadow = 200, opacity = 200 },
				hover = { background = alpha(fg, 0.10) },
				active = { background = alpha(fg, 0.10) },
				checked = {
					color = s.on_hot,
					background = hot,
					shadow = bit_glow,
					font = { weight = 400 },
				},
				disabled = { opacity = 0.25 },
			},
			bit_index = { font = outfit(10), color = alpha(fg, 0.4), margin = { 0, 0, 0, 2 } },
		},

		date = {
			label = { font = outfit(13, 600, 0.04), color = alpha(fg, 0.6), margin = { 6, 0, 0, 0 } },
			field = {
				min_height = 44,
				padding = { 0, 14 },
				radius = 14,
				font = outfit(16),
				color = fg,
				background = down(glass_hi, glass),
				border = false,
				shadow = { highlight, rim },
				-- Over the glass, not instead of it.
				hover = { overlay = alpha(fg, 0.05) },
				active = { overlay = alpha(fg, 0.12) },
			},
			result_panel = {
				background = panel_fill,
				radius = 20,
				border = false,
				shadow = panel_shadow,
				padding = { 10, 6, 14, 6 },
				margin = { 10, 0, 0, 0 },
			},
			result = { size = 34, font = { family = "Outfit", weight = 300 } },
			add_result = { size = 30, font = { family = "Outfit", weight = 300 } },
			subresult = { font = outfit(17), color = alpha(fg, 0.65), margin = { 0, 0, 0, 14 } },
			calendar = {
				title = { font = outfit(15, 600), color = fg },
				weekday = { font = { size = 12 }, color = alpha(fg, 0.62) },
				day = {
					min_width = 34,
					min_height = 34,
					radius = 999,
					padding = 0,
					font = { size = system.size, features = { "tnum" } },
					color = fg,
					hover = flat_hover,
					active = flat_active,
					checked = { background = accent, color = on_accent },
				},
				other_month = alpha(fg, 0.38),
				today = { width = 1, color = accent },
				focus = { width = 2, color = alpha(accent, 0.9), offset = 1 },
			},
		},

		converter = {
			field = {
				radius = 18,
				padding = { 6, 4, 8, 4 },
				transition = { background = 200, shadow = 240 },
				checked = {
					background = across(alpha(accent, 0.14), alpha(accent, 0.02)),
					shadow = { { inset = true, x = 3, color = accent } },
				},
			},
			marker = false,
			value = {
				size = 44,
				font = { family = "Outfit", weight = 300 },
				checked_weight = 300,
				focus = { width = 2, color = alpha(accent, 0.9), offset = 2, radius = 12 },
			},
			symbol = { font = outfit(26, 300), color = alpha(fg, 0.6), margin = { 0, 0, 0, 14 } },
			unit = {
				margin = { 0, 0, 0, 8 },
				radius = 12,
				padding = { 2, 10 },
				min_height = 34,
				font = outfit(15),
				color = fg,
				background = alpha(fg, 0.05),
				hover = { background = alpha(fg, 0.10) },
				active = { background = alpha(fg, 0.16) },
				checked = { background = alpha(fg, 0.16) },
			},
			swap = {
				margin = { 0, 0, 0, 12 },
				min_width = 36,
				min_height = 36,
				radius = 999,
				color = fg,
				background = slant(alpha(s.hot_a, 0.25), alpha(s.hot_b, 0.25)),
				shadow = { rim },
				transition = { transform = { ms = 380, ease = "spring" } },
				active = { rotate = 180, scale = 0.92 },
			},
			caption = { font = { size = system.size }, color = fg },
			chip = {
				padding = { 4, 10 },
				radius = 999,
				min_height = 0,
				font = outfit(13),
				color = fg,
				background = alpha(fg, 0.06),
				shadow = { { inset = true, spread = 1, color = alpha(fg, 0.06) } },
			},
			whimsy = { color = s.on_hot, background = hot_soft },
			whimsy_icon = true,
			ratio = { font = outfit(15), color = fg, margin = { 4, 14, 0, 14 } },
			timestamp = { font = { size = 12 }, color = alpha(fg, 0.6), margin = { 0, 0, 0, 14 } },
			link = {
				font = { size = 12 },
				padding = { 0, 6 },
				min_height = 22,
				color = accent,
				hover = flat_hover,
				active = flat_active,
			},
		},

		graph = {
			side = {
				background = panel_fill,
				radius = 20,
				border = false,
				shadow = panel_shadow,
				padding = 10,
			},
			divider = false,
			row = { padding = 4, radius = 14 },
			row_error = { background = alpha(s.hot_a, 0.08) },
			error = { font = { size = 12 }, color = s.hot_a, margin = { 0, 0, 0, 40 } },
			entry = {
				min_height = 38,
				radius = 12,
				font = outfit(16),
				background = alpha(fg, 0.05),
			},
			swatch = {
				size = 18,
				hidden_opacity = 0.35,
				glow = { blur = 12, spread = -2 },
				active_scale = 0.8,
				transition = { opacity = 200, transform = { ms = 300, ease = "spring" } },
			},
			add = {
				min_height = 40,
				radius = 12,
				color = alpha(fg, 0.6),
				background = gmnb.transparent,
				shadow = { { inset = true, spread = 1, color = alpha(fg, 0.12) } },
				hover = { color = fg, background = alpha(fg, 0.05) },
				active = { color = fg, background = alpha(fg, 0.05) },
			},
			variable = { font = { family = "Outfit", size = 17, weight = 400, italic = true }, color = fg },
			analysis = {
				title = { font = outfit(18), color = fg, margin = { 2, 8, 6, 8 } },
				card = { padding = { 8, 12 }, radius = 14, background = alpha(fg, 0.04), border = false },
				card_title = { font = outfit(12, 600, 0.06), color = alpha(fg, 0.6) },
				value = { font = outfit(16), color = fg },
				text = { font = { size = 14 }, color = alpha(fg, 0.8) },
				note = { font = { size = 12, italic = true }, color = alpha(fg, 0.6) },
			},
			toolbar = {
				padding = 4,
				radius = 14,
				background = alpha(base, 0.72),
				border = false,
				shadow = toolbar_shadow,
			},
			toolbar_button = { min_width = 30, min_height = 30, icon = { size = 16 } },
			row_button = { min_width = 30, min_height = 30, icon = { size = 16 } },
			canvas = {
				background = if dark
					then down(alpha(black, 0.20), alpha(black, 0.30))
					else down(alpha(white, 0.55), alpha(white, 0.42)),
				radius = 20,
				border = { width = 1, color = alpha(fg, if dark then 0.12 else 0.18) },
				grid_minor = alpha(fg, if dark then 0.045 else 0.06),
				grid_major = alpha(fg, if dark then 0.10 else 0.12),
				axis = { color = alpha(fg, 0.55), width = 1.4 },
				label = { font = outfit(11), color = alpha(fg, 0.62) },
				fill_opacity = 0.18,
				-- Neon in the dark.
				glow = if dark then { blur = 6, width = 3.2, opacity = 0.55 } else false,
				trace = {
					hair_opacity = 0.45,
					hair_dash = { 4, 4 },
					halo = { grow = 3, color = "series", opacity = 0.25 },
					ring = { width = 1.5, color = alpha(white, 0.9) },
					bubble = {
						background = if dark then alpha(s.base_bottom, 0.92) else alpha(white, 0.95),
						color = alpha(fg, 0.95),
						radius = 10,
						font = outfit(13),
						border = false,
						shadow = { { y = 4, blur = 12, color = alpha(black, 0.3) } },
					},
				},
			},
		},
	}
end

---------------------------------------------------------------------------
-- 3. Background: the aurora
---------------------------------------------------------------------------

-- The blobs: where each rests (as fractions of the window), how far it
-- wanders, how fast in x and in y (radians a second), where in its swing it
-- starts, and its size (a fraction of the window's longer side).
local BLOBS = {
	{ x = 0.10, y = 0.08, wander = 0.20, speed_x = 0.071, speed_y = 0.053, phase = 0.0, size = 0.62 },
	{ x = 0.98, y = 0.34, wander = 0.16, speed_x = 0.047, speed_y = 0.067, phase = 1.7, size = 0.52 },
	{ x = 0.22, y = 0.98, wander = 0.18, speed_x = 0.059, speed_y = 0.043, phase = 3.1, size = 0.58 },
	{ x = 0.88, y = 0.96, wander = 0.14, speed_x = 0.038, speed_y = 0.061, phase = 4.4, size = 0.46 },
}

-- The blobs move about 5 px a second: 15 frames a second is smooth.
local DRIFT_FRAME = 0.066
-- How long a pulse lasts, in seconds.
local PULSE_LIFE = 1.15

-- Film grain: it hides the gradients' banding and makes them feel like a
-- material. White specks in the dark, black ones in the light.
local GRAIN = 192
local grain_dark = gmnb.noise({ size = GRAIN, seed = 0x9e3779b9, color = white, opacity = 0.07 })
local grain_light = gmnb.noise({ size = GRAIN, seed = 0x9e3779b9, color = black, opacity = 0.05 })

-- How far the aurora has drifted, in seconds. It starts at the frame 0.2
-- showed when it stood still, and only moves on while "Living background"
-- is on (`frame.dt` is 0 whenever the app isn't animating).
local drift = 37.0

local function pulse_at(draw: Draw, w: number, h: number, x: number, y: number, color: Rgba, strength: number, age: number)
	local life = age / PULSE_LIFE
	local grow = 1 - (1 - life) ^ 3
	local radius = 40 + grow * 340
	local a = (1 - life) ^ 1.6 * 0.30 * strength
	draw:rect(0, 0, w, h, gmnb.radial({
		center = { x, y },
		radius = radius,
		colors = { alpha(color, a * 0.6), alpha(color, a), alpha(color, 0) },
		stops = { 0, 0.55, 1 },
	}))
end

-- The colour and strength of the pulse a press makes.
local function press_pulse(e: Event, s: Scheme): (Rgba, number)
	if e.source == "key" and e.key then
		local key = e.key
		if e.page == "graphing" then
			return s.accent, 0.4
		elseif e.via == "keyboard" then
			if e.page == "converter" then
				return s.accent, 0.5
			end
			return if key.kind == "equals" then s.hot_a else s.accent, 0.6
		elseif e.page == "converter" then
			return s.blobs[key.id * 5 % 4 + 1], 0.55
		elseif key.kind == "equals" then
			return s.hot_a, 1.4
		end
		return s.blobs[key.id * 7 % 4 + 1], 0.55
	elseif e.source == "swap" then
		return s.hot_a, 1.0
	elseif e.source == "date" then
		return s.hot_a, 0.9
	end
	-- The memory row, F-E.
	return s.accent, 0.5
end

local function background(draw: Draw, frame: Frame): Again
	local s = scheme(frame.ctx)
	local w, h = frame.width, frame.height
	local living = frame.ctx.options.living == true
	if living then
		drift += frame.dt
	end
	local t = drift

	-- The base: a gradient leaning a little to the right.
	draw:rect(0, 0, w, h, gmnb.linear({
		from = { 0, 0 },
		to = { w * 0.35, h },
		colors = { s.base_top, s.base_bottom },
	}))

	-- Four blobs, each swinging around its resting place and breathing.
	local span = math.max(w, h)
	for i, b in BLOBS do
		local cx = (b.x + b.wander * math.sin(t * b.speed_x + b.phase)) * w
		local cy = (b.y + b.wander * 0.8 * math.cos(t * b.speed_y + b.phase * 1.3)) * h
		local breathe = 1 + 0.08 * math.sin(t * 0.11 + (i - 1))
		local r = span * b.size * breathe
		local c, a = s.blobs[i], s.blob_alpha
		draw:rect(0, 0, w, h, gmnb.radial({
			center = { cx, cy },
			radius = r,
			radius_y = r * 0.82,
			colors = { alpha(c, a), alpha(c, a * 0.62), alpha(c, a * 0.18), alpha(c, 0) },
			stops = { 0, 0.3, 0.62, 1 },
		}))
	end

	-- Pulses: a soft bloom wherever something just happened.
	local pulsing = false
	for _, e in frame.events do
		if e.age < PULSE_LIFE then
			if e.kind == "press" then
				local color, strength = press_pulse(e, s)
				pulsing = true
				pulse_at(draw, w, h, e.x, e.y, color, strength, e.age)
			elseif (e.kind == "result" or e.kind == "error") and e.page == "calculator" then
				pulsing = true
				local color = if e.kind == "error" then s.hot_a else s.hot_b
				pulse_at(draw, w, h, e.rect.x + e.rect.w * 0.8, e.rect.y + e.rect.h * 0.65, color, 1.1, e.age)
			elseif e.kind == "option" and e.id == "palette" then
				-- A new palette introduces itself.
				pulsing = true
				for i, c in s.blobs do
					pulse_at(draw, w, h, w * (0.2 + 0.2 * (i - 1)), h * 0.5, c, 1.4, e.age)
				end
			end
		end
	end

	-- Vignette: depth in the dark, a soft halo in the light.
	local edge = if s.dark then alpha(black, 0.38) else alpha(white, 0.30)
	draw:rect(0, 0, w, h, gmnb.radial({
		center = { w * 0.5, h * 0.42 },
		radius = math.sqrt(w * w + h * h) * 0.62,
		colors = { alpha(edge, 0), edge },
		stops = { 0.55, 1 },
	}))

	draw:tile(if s.dark then grain_dark else grain_light, 0, 0, w, h)

	if pulsing then
		return 0
	elseif living then
		return DRIFT_FRAME
	end
	return nil
end

---------------------------------------------------------------------------
-- 4. Keypad: ripples, the pointer's light, the cascade
---------------------------------------------------------------------------

local RIPPLE_LIFE = 0.55
-- The cascade: each diagonal starts this much later, and takes this long.
local CASCADE_STEP = 0.022
local CASCADE_TIME = 0.46
-- How far the pointer's light reaches.
local REVEAL_RADIUS = 120

local function newest(events: { Event }, kind: string): Event?
	for i = #events, 1, -1 do
		if events[i].kind == kind then
			return events[i]
		end
	end
	return nil
end

-- Keys spring in along the diagonals whenever a keypad comes into view.
local function place(key: KeyCell, frame: Frame): Placement?
	local show = newest(frame.events, "show")
	if not show then
		return nil
	end
	local p = math.clamp((show.age - (key.row + key.col) * CASCADE_STEP) / CASCADE_TIME, 0, 1)
	if p >= 1 then
		return nil
	elseif p <= 0 then
		return { opacity = 0 }
	end
	local rise = ease.out_cubic(p)
	return { dy = (1 - rise) * 16, scale = 0.86 + 0.14 * ease.out_back(p), opacity = rise }
end

local function keypad(draw: Draw, frame: Frame): Again
	local s = scheme(frame.ctx)
	local keys: { KeyCell } = frame.keys or {}
	local light = if s.dark then white else s.accent
	local busy = false

	local show = newest(frame.events, "show")
	if show then
		local last = 0
		for _, k in keys do
			last = math.max(last, k.row + k.col)
		end
		busy = show.age <= last * CASCADE_STEP + CASCADE_TIME
	end

	-- Ripples spread from where a key was pressed, and stay inside it.
	for _, e in frame.events do
		if e.kind == "press" and e.source == "key" and e.key and e.page ~= "graphing" and e.age < RIPPLE_LIFE then
			busy = true
			local life = e.age / RIPPLE_LIFE
			local r = e.rect
			local reach = math.sqrt(r.w * r.w + r.h * r.h) * 1.1
			local a = (1 - life) ^ 1.4 * (if s.dark then 0.26 else 0.30)
			local color = if e.key.kind == "equals" then s.on_hot else light
			draw:push_clip(r.x, r.y, r.w, r.h, KEY_RADIUS)
			draw:rect(r.x, r.y, r.w, r.h, gmnb.radial({
				center = { e.x, e.y },
				radius = 6 + ease.out_cubic(life) * reach,
				colors = { alpha(color, a * 0.5), alpha(color, a), alpha(color, 0) },
				stops = { 0, 0.8, 1 },
			}))
			draw:pop()
		end
	end

	-- Reveal: the borders of the keys near the pointer light up, and the
	-- key under it glows.
	local p = frame.pointer
	if p then
		local border = if s.dark then 0.55 else 0.75
		local glow = if s.dark then 0.12 else 0.16
		for _, k in keys do
			local r = k.rect
			local nx = math.clamp(p.x, r.x, r.x + r.w)
			local ny = math.clamp(p.y, r.y, r.y + r.h)
			local away = math.sqrt((p.x - nx) ^ 2 + (p.y - ny) ^ 2)
			if k.enabled and away <= REVEAL_RADIUS then
				local edge = gmnb.path():rounded_rect(r.x + 0.5, r.y + 0.5, r.w - 1, r.h - 1, KEY_RADIUS - 0.5)
				draw:stroke(edge, 1.25, gmnb.radial({
					center = { p.x, p.y },
					radius = REVEAL_RADIUS,
					colors = { alpha(light, border), alpha(light, 0) },
				}))
				if away == 0 then
					draw:push_clip(r.x, r.y, r.w, r.h, KEY_RADIUS)
					draw:rect(r.x, r.y, r.w, r.h, gmnb.radial({
						center = { p.x, p.y },
						radius = math.max(r.w, r.h) * 0.95,
						colors = { alpha(light, glow), alpha(light, 0) },
					}))
					draw:pop()
				end
			end
		end
	end

	return if busy then 0 else nil
end

---------------------------------------------------------------------------
-- 5. Display: gradient digits, glow, and how a new value arrives
---------------------------------------------------------------------------

-- How long each kind of change plays for, in seconds.
local function change_time(kind: string, digits: number): number
	if kind == "typing" then
		return 0.26
	elseif kind == "result" then
		return 0.5 + digits * 0.028 + 0.35
	elseif kind == "replace" then
		return 0.32
	end
	return 0.5 -- error
end

local function display(draw: Draw, frame: Frame): Again
	local v = frame.value
	if not v then
		return nil
	end
	local s = scheme(frame.ctx)
	local digits = #v.glyphs
	local change = v.change
	if change and change.age > change_time(change.kind, digits) then
		change = nil
	end
	local kind = if change then change.kind else "none"
	local t = if change then change.age else 0
	local from = if change then change.from else 1

	-- An error shakes its head.
	local shake = 0
	if kind == "error" then
		local p = math.min(t / 0.5, 1)
		shake = math.sin(t * 46) * 9 * (1 - p) ^ 2
	end

	-- Replace: the old value drifts up and fades.
	local old = v.previous
	if kind == "replace" and old then
		local p = math.min(t / 0.22, 1)
		draw:save()
		draw:translate(0, -14 * p)
		draw:push_opacity((1 - p) * 0.8)
		for _, g in old.glyphs do
			draw:glyph(g, s.fg)
		end
		draw:pop()
		draw:restore()
	end

	-- Where one glyph is in the change being played: how far below its
	-- place, how opaque, how large.
	local function pose(g: Glyph): (number, number, number)
		if kind == "typing" and g.index >= from then
			-- New digits pop in with a little overshoot.
			local p = math.min(t / 0.26, 1)
			local back = 1 + 2.7 * (p - 1) ^ 3 + 1.7 * (p - 1) ^ 2
			return 0, math.sqrt(p), 0.55 + 0.45 * back
		elseif kind == "result" then
			-- The number rises in as a wave, left to right.
			local p = math.clamp((t - (g.index - 1) * 0.028) / 0.5, 0, 1)
			local e = ease.out_quart(p)
			return (1 - e) * v.size * 0.45, e, 1
		elseif kind == "replace" then
			local e = ease.out_cubic(math.min(t / 0.32, 1))
			return (1 - e) * 12, e, 1
		end
		return 0, 1, 1
	end

	local function glyphs(color: Rgba)
		for _, g in v.glyphs do
			local dy, opacity, size = pose(g)
			if opacity > 0.001 then
				local cx, cy = g.x + g.w / 2, g.y + g.h / 2
				draw:save()
				draw:translate(cx + shake, cy + dy)
				draw:scale(size)
				draw:translate(-cx, -cy)
				draw:glyph(g, alpha(color, opacity))
				draw:restore()
			end
		end
	end

	-- A glow in the dark.
	if s.dark and digits > 0 then
		draw:push_blur(v.size * 0.30)
		glyphs(alpha(s.accent, 0.42))
		draw:pop()
	end

	-- The digits are a window onto a gradient: text colour at the top,
	-- leaning to the accent at the bottom (or to the hot colour, in error).
	local top = if v.error then alpha(s.fg, 0.92) else s.fg
	local bottom = if v.error then alpha(s.hot_a, 0.95) else mix(s.fg, s.accent, if s.dark then 0.55 else 0.45)
	local band = v.band
	draw:push_mask("alpha")
	glyphs(white)
	draw:pop()
	draw:rect(band.x, band.y, band.w, band.h, gmnb.linear({
		from = { 0, band.y },
		to = { 0, band.y + band.h },
		colors = { top, bottom },
		stops = { 0.15, 1 },
	}))
	if kind == "result" then
		-- A band of light sweeps across once the wave has landed.
		local p = math.clamp((t - (0.25 + digits * 0.028)) / 0.55, 0, 1)
		if p > 0 and p < 1 then
			local x = v.rect.x - 80 + (v.rect.w + 160) * p - 70
			draw:rect(x, band.y, 140, band.h, gmnb.linear({
				from = { x, band.y },
				to = { x + 140, band.y + band.h * 0.4 },
				colors = { alpha(s.hot_a, 0), alpha(white, 0.95), alpha(s.hot_b, 0) },
			}))
		end
	end
	draw:pop()

	return if change then 0 else nil
end

---------------------------------------------------------------------------
-- 6. Options
---------------------------------------------------------------------------

-- A palette's tile in Settings: its background, its four blobs, and a dot
-- of its hot gradient.
local function tile(draw: Draw, frame: Preview, choice: string)
	local s = scheme_for(choice, frame.ctx)
	local w, h = frame.width, frame.height
	draw:push_clip(0, 0, w, h, 12)
	draw:rect(0, 0, w, h, gmnb.linear({
		from = { 0, 0 },
		to = { w * 0.3, h },
		colors = { s.base_top, s.base_bottom },
	}))
	local spots = { { 0.2, 0.2 }, { 0.85, 0.35 }, { 0.35, 0.95 }, { 0.9, 0.95 } }
	for i, spot in spots do
		local c = s.blobs[i]
		draw:rect(0, 0, w, h, gmnb.radial({
			center = { spot[1] * w, spot[2] * h },
			radius = w * 0.6,
			colors = { alpha(c, math.min(s.blob_alpha * 1.4, 1)), alpha(c, 0) },
		}))
	end
	draw:circle(w - 20, h - 18, 9, gmnb.linear({
		from = { w - 34, h - 26 },
		to = { w - 10, h - 8 },
		colors = { s.hot_a, s.hot_b },
	}))
	draw:pop()
end

local function is(palette_id: string)
	return function(options: { [string]: OptionValue }, _ctx: Ctx): boolean
		return options.palette == palette_id
	end
end

local options: { Option } = {
	{
		id = "palette",
		type = "choice",
		group = "Palette",
		label = "Palette",
		description = "Colours for the aurora, keys and graphs",
		default = "aurora",
		choices = {
			{ id = "aurora", label = "Aurora" },
			{ id = "ember", label = "Ember" },
			{ id = "abyss", label = "Abyss" },
			{ id = "orchard", label = "Orchard" },
			{ id = "graphite", label = "Graphite" },
			{ id = "system", label = "System" },
			{ id = "custom", label = "Freestyle" },
		},
		preview = tile,
	},
	{
		id = "system_note",
		type = "note",
		group = "Palette",
		label = "Uses your desktop's accent colour (the freedesktop portal setting your desktop or shell publishes) and its complement, and follows it live.",
		visible = is("system"),
	},
	{
		id = "primary",
		type = "color",
		group = "Palette",
		label = "Primary colour",
		description = "Accent, keys and the aurora's main hue",
		default = DEFAULT_PRIMARY,
		visible = is("custom"),
	},
	{
		id = "secondary",
		type = "color",
		group = "Palette",
		label = "Secondary colour",
		description = "The second glow and the = key's gradient",
		default = DEFAULT_SECONDARY,
		visible = is("custom"),
		actions = {
			{
				label = "Complement",
				tooltip = "Use the colour opposite the primary on the colour wheel",
				run = function(o: { [string]: OptionValue }, _ctx: Ctx): { [string]: OptionValue }
					return { secondary = gmnb.to_hex(gmnb.complement(colour(o.primary, DEFAULT_PRIMARY))) }
				end,
			},
		},
	},
	{
		id = "living",
		type = "toggle",
		group = "Motion",
		label = "Living background",
		description = "Let the aurora drift. Pauses whenever the window is in the background",
		default = true,
	},
	{
		id = "motion_note",
		type = "note",
		group = "Motion",
		label = "Key animations follow the system's “reduce animations” setting.",
	},
}

local theme: Theme = {
	api = 1,
	name = "Aurora",
	description = "A drifting aurora behind glass keys, in seven palettes. GMNB's look up to 0.2.",
	author = "GMNB contributors",
	license = "MIT",
	fonts = { "fonts/Outfit-Variable.ttf" },
	credits = { { title = "Outfit typeface", file = "fonts/OFL-Outfit.txt" } },
	options = options,
	style = style,
	layers = {
		background = { draw = background, events = { press = PULSE_LIFE, result = PULSE_LIFE, error = PULSE_LIFE, option = PULSE_LIFE } },
		keypad = { draw = keypad, place = place, pointer = true, events = { press = RIPPLE_LIFE, show = 1.5 } },
		display = { draw = display },
	},
}

return theme
