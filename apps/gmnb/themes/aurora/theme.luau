--!strict
-- Aurora: an aurora behind glass keys, in seven palettes. Curtains of
-- light over a night sky, drifting.
--
-- This was GMNB's whole look up to version 0.2 (its aurora was four soft
-- blobs of colour then). It is a theme now, written against theme API 1
-- (docs/theming.md) like any other: nothing in the app knows it by name.
-- It uses nearly every part of the API, so it doubles as the worked
-- example. Copy what you like.
--
-- The file reads top to bottom:
--   1. Palettes        the colours of each palette, light and dark
--   2. Inks, glass and frost
--                      what keeps every palette readable over an aurora
--                      that moves, see-through or not
--   3. Style           how every part of the window looks
--   4. Background      the sky, its stars, the aurora and its pulses
--   5. Keypad          ripples, the light that follows the pointer, the cascade
--   6. Display         gradient digits, their glow and their animations
--   7. Options         what Settings offers, and the palette tiles
--   8. Sound           glass under the keys, a chime for a result, a chord per palette

local hex, hsl, mix, alpha, over = gmnb.hex, gmnb.hsl, gmnb.mix, gmnb.alpha, gmnb.over
local white, black = gmnb.white, gmnb.black
local ease = gmnb.ease
-- 4.5 and 3: what WCAG asks of text, and of large text and lines.
local TEXT, UI = gmnb.TEXT_CONTRAST, gmnb.UI_CONTRAST

---------------------------------------------------------------------------
-- 1. Palettes
---------------------------------------------------------------------------

-- What a palette's author chooses.
type Palette = {
	dark: boolean,
	-- The sky, top to bottom.
	sky_top: Rgba,
	sky_bottom: Rgba,
	-- The aurora's light by height, lowest first: under the curtains'
	-- hems, at the hems, the curtains' body, and three colours higher and
	-- higher up, to the crown. As their author would have them: each is
	-- tempered as far as the text over it needs (section 2).
	lights: { Rgba },
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
	sky_top: string,
	sky_bottom: string,
	lights: { string },
	fg: string,
	accent: string,
	hot_a: string,
	hot_b: string,
	on_hot: string,
	series: { string }
): Palette
	return {
		dark = dark,
		sky_top = hex(sky_top),
		sky_bottom = hex(sky_bottom),
		lights = list(lights),
		fg = hex(fg),
		accent = hex(accent),
		hot_a = hex(hot_a),
		hot_b = hex(hot_b),
		on_hot = hex(on_hot),
		series = list(series),
	}
end

-- Each palette is an aurora of its own. In the dark it hangs in a night
-- sky. In the light it is the same aurora at dawn: a pale sky, and the
-- curtains in colours deep enough to show on it. (A black sky under the
-- light look's dark text and milk glass would be the dark look.)
local BUILT_IN: { [string]: (dark: boolean) -> Palette } = {
	-- The aurora most often seen: green, teal above it, violet and
	-- crimson at the crown.
	aurora = function(dark)
		if dark then
			return palette(true, "#02040b", "#071428",
				{ "#b6ff9c", "#3df58f", "#19d6b0", "#3f8cff", "#9a62ff", "#e6509c" },
				"#f2f6ff", "#7cf2c0", "#5cf2a8", "#22c8d8", "#03221c",
				{ "#5ef2b0", "#ff7ad9", "#ffd36e", "#8fb4ff", "#b79bff", "#ff8f6b" })
		end
		return palette(false, "#f5f4fd", "#fdf6ef",
			{ "#8fe08a", "#2fcf8a", "#22bfae", "#5a9cf5", "#9a7af0", "#f07ab4" },
			"#14213a", "#0b7d5c", "#19b97c", "#0f9fb3", "#ffffff",
			{ "#0b8f6a", "#d6339a", "#b77a00", "#3b6fe0", "#7a4be0", "#e0582e" })
	end,
	-- The rare red aurora: amber at the hem, crimson, violet above.
	ember = function(dark)
		if dark then
			return palette(true, "#0b0406", "#1c0a10",
				{ "#ffd27a", "#ff9a4a", "#ff5a46", "#f03a6e", "#c23ad0", "#7a48f0" },
				"#fff3ee", "#ffb08a", "#ffc35a", "#ff4f6e", "#2a0b10",
				{ "#ffc35a", "#ff6a8d", "#7ee0ff", "#c69bff", "#ff9a6e", "#9cf08a" })
		end
		return palette(false, "#fffaf6", "#fff0e6",
			{ "#ffcf7a", "#ff9f5a", "#ff7a5c", "#f2608c", "#d06ad8", "#9a7af0" },
			"#3a1416", "#d94a26", "#f7a21b", "#e6365b", "#ffffff",
			{ "#d97a00", "#d6335e", "#0f8fb8", "#8a4fe0", "#d94a26", "#2f9e3a" })
	end,
	-- A blue one, over deep water.
	abyss = function(dark)
		if dark then
			return palette(true, "#01060f", "#04152b",
				{ "#7dffe0", "#2ee6d6", "#18b6f0", "#3d74ff", "#5d4dff", "#9a4dff" },
				"#eaf8ff", "#62dcff", "#3af0d0", "#3a7bff", "#02121f",
				{ "#3af0d0", "#7ab8ff", "#ffd36e", "#ff7ad9", "#b79bff", "#a4f07a" })
		end
		return palette(false, "#f4fbff", "#e9f4ff",
			{ "#8fead8", "#4fd6d0", "#3fb6ee", "#5f8ff5", "#7a7af2", "#a07af0" },
			"#0b2236", "#0a7fc2", "#10b9a0", "#2563eb", "#ffffff",
			{ "#0a9e86", "#2563eb", "#c98a00", "#d6339a", "#6d3fe8", "#3f8f1f" })
	end,
	-- Leaf green, the gold of fruit, blossom at the crown.
	orchard = function(dark)
		if dark then
			return palette(true, "#040a05", "#0b1a0d",
				{ "#6fe07a", "#9be35a", "#d4ea4a", "#ffd24a", "#ffa85a", "#ff7a9a" },
				"#f1fbec", "#aee88a", "#e8f05a", "#35c98a", "#0b1a0e",
				{ "#e8f05a", "#5ee0c0", "#ff9a6e", "#8fb4ff", "#ff7ad9", "#9be37a" })
		end
		return palette(false, "#f9fdf4", "#f3f8e6",
			{ "#8fdc7a", "#b4dc5a", "#dcdc4a", "#f2c84a", "#f5a86a", "#f08aa4" },
			"#142a12", "#2f8f2a", "#b7c71a", "#1d9f6a", "#ffffff",
			{ "#2f8f2a", "#0f8f80", "#d2691e", "#3b6fe0", "#c0338f", "#8a6d00" })
	end,
	-- Light with no colour in it.
	graphite = function(dark)
		if dark then
			return palette(true, "#07080a", "#131519",
				{ "#f2f3f5", "#d5d9e0", "#aab1bf", "#8b93a3", "#6c7585", "#525a68" },
				"#f2f3f5", "#dfe3ea", "#f2f3f5", "#aab1bf", "#111318",
				{ "#f2f3f5", "#9fb7ff", "#ffc58a", "#8ae0c8", "#ff9ac6", "#d5ff8a" })
		end
		return palette(false, "#fafafa", "#eff0f2",
			{ "#9aa0ab", "#a6acb7", "#b4b9c3", "#c2c6cf", "#cfd3da", "#dadde3" },
			"#16181d", "#2c313a", "#2c313a", "#4a515e", "#ffffff",
			{ "#2c313a", "#2563eb", "#d2691e", "#0f8f80", "#c0338f", "#6b8e00" })
	end,
}

local DEFAULT_PRIMARY = "#7c4dff"
local DEFAULT_SECONDARY = "#ff6fb5"
-- What System uses on a desktop that shares no accent colour.
local FALLBACK_ACCENT = "#3584e4"

-- A complete palette from two colours: a sky tinted with the first, an
-- aurora that is the first at its hem and the second at its crown, a hot
-- gradient running secondary to primary, and six well-spread graph
-- colours. System makes one from the desktop's accent and its
-- complement, Freestyle from the two colours you pick.
local function generate(primary: Rgba, secondary: Rgba, dark: boolean): Palette
	local ph, ps = gmnb.to_hsl(primary)
	local qh, qs = gmnb.to_hsl(secondary)
	-- Keep greys grey, but give real colours some life.
	local function vivid(sat: number): number
		return if sat < 0.08 then sat else math.clamp(sat, 0.55, 1.0)
	end
	local pv, qv = vivid(ps), vivid(qs)
	local hues = { ph, ph + 180, ph + 60, ph + 240, ph + 120, ph + 300 }
	local series = {}
	if dark then
		for i, h in hues do
			series[i] = hsl(h, 0.8, 0.66)
		end
		return {
			dark = true,
			sky_top = hsl(ph, math.min(ps * 0.7, 0.55), 0.025),
			sky_bottom = hsl(ph, math.min(ps * 0.75, 0.6), 0.085),
			lights = {
				hsl(ph - 22, pv, 0.74),
				hsl(ph, pv, 0.62),
				hsl(ph + 26, pv, 0.58),
				hsl(qh - 26, qv, 0.60),
				hsl(qh, qv, 0.62),
				hsl(qh + 22, qv, 0.64),
			},
			fg = hsl(ph, math.min(ps, 0.5), 0.965),
			accent = hsl(ph, pv, 0.76),
			hot_a = hsl(qh, qv, 0.66),
			hot_b = hsl(ph, pv, 0.60),
			on_hot = white,
			series = series,
		}
	end
	for i, h in hues do
		series[i] = hsl(h, 0.7, 0.42)
	end
	return {
		dark = false,
		sky_top = hsl(ph, math.min(ps, 0.6), 0.975),
		sky_bottom = hsl(ph, math.min(ps, 0.55), 0.955),
		lights = {
			hsl(ph - 22, math.min(pv, 0.8), 0.72),
			hsl(ph, math.min(pv, 0.8), 0.64),
			hsl(ph + 26, math.min(pv, 0.8), 0.62),
			hsl(qh - 26, math.min(qv, 0.8), 0.66),
			hsl(qh, math.min(qv, 0.8), 0.68),
			hsl(qh + 22, math.min(qv, 0.8), 0.72),
		},
		fg = hsl(ph, math.min(ps, 0.45), 0.15),
		accent = hsl(ph, math.min(pv, 0.9), 0.42),
		hot_a = hsl(qh, math.min(qv, 0.8), 0.58),
		hot_b = hsl(ph, math.min(pv, 0.75), 0.50),
		on_hot = white,
		series = series,
	}
end

---------------------------------------------------------------------------
-- 2. Inks, glass and frost
---------------------------------------------------------------------------
--
-- Text here lies on glass, and behind the glass is an aurora: night sky in
-- one place, a curtain's bright hem in the next, and the hem moves. A
-- palette can come from anywhere (a neon-yellow desktop accent, pure
-- grey, two colours somebody liked), and the window can be see-through
-- over any desktop. So nothing below trusts a colour to be readable, and
-- it is settled from both sides:
--
--   the light  each colour of the aurora is tempered (the same hue, as
--              saturated, nearer the sky's own lightness) until the text
--              reads on it through the thinnest pane there is;
--   the inks   every ink is then worked out against all that it can lie
--              on: each pane over the sky and over each of those lights.
--
-- So wherever a curtain wanders, under whatever text, that text reads.
-- crates/gmnb-theme/tests/css.rs goes through every colour the
-- background can make under every text there is.

local contrast = gmnb.contrast

-- A palette is finished with some ten thousand contrast checks, and the
-- frost is worked out again for every step of the opacity slider. So the
-- checks themselves are done in plain numbers: what `gmnb.over` and
-- `gmnb.contrast` do, without a colour made for each (a seventh of the
-- time).

-- How light a colour is, as the contrast rules count it (WCAG's relative
-- luminance), from its three parts.
local function lightness(r: number, g: number, b: number): number
	local function part(c: number): number
		return if c <= 0.04045 then c / 12.92 else ((c + 0.055) / 1.055) ^ 2.4
	end
	return 0.2126 * part(r) + 0.7152 * part(g) + 0.0722 * part(b)
end

-- The contrast of two such.
local function ratio(a: number, b: number): number
	return (math.max(a, b) + 0.05) / (math.min(a, b) + 0.05)
end

-- `top`, which may be translucent, laid over an opaque colour's three
-- parts.
local function laid(top: Rgba, r: number, g: number, b: number): (number, number, number)
	local a = top.a or 1
	return top.r * a + r * (1 - a), top.g * a + g * (1 - a), top.b * a + b * (1 - a)
end

-- How `ink`, which may be translucent, reads on an opaque `ground`.
local function reads_at(ink: Rgba, ground: Rgba): number
	local r, g, b = ground.r, ground.g, ground.b
	return ratio(lightness(laid(ink, r, g, b)), lightness(r, g, b))
end

-- Glass. A key's is thin (`top` to `bottom`; a number key's ends at
-- `number`, a little more solid), so that the aurora shows through the
-- keypad, which is most of the window. What carries smaller and fainter
-- text is thicker: a panel's glass, and the veil, the pane under text
-- that lies straight on the page (the readout, the memory row, the
-- header).
--
-- In the dark it is smoked, with the night's own colour: glass dims the
-- aurora behind it and keeps its hue, and the sky behind it stays black.
-- (White glass, as 0.2 had it, turns a night sky grey.) In the light it
-- is milk glass.
type Glass = {
	top: Rgba,
	bottom: Rgba,
	number: Rgba,
	panel_top: Rgba,
	panel_bottom: Rgba,
	rim: Rgba,
	veil: number,
}

local function glass_for(dark: boolean, sky: Rgba): Glass
	if dark then
		return {
			top = alpha(sky, 0.22),
			bottom = alpha(sky, 0.32),
			number = alpha(sky, 0.40),
			panel_top = alpha(sky, 0.48),
			panel_bottom = alpha(sky, 0.56),
			rim = alpha(white, 0.12),
			veil = 0.5,
		}
	end
	return {
		top = alpha(white, 0.68),
		bottom = alpha(white, 0.46),
		number = alpha(white, 0.64),
		panel_top = alpha(white, 0.80),
		panel_bottom = alpha(white, 0.64),
		rim = alpha(white, 0.85),
		veil = 0.625,
	}
end

-- The graph's canvas is a pane of smoked glass in the dark and of milk
-- glass in the light, thicker than a key's: top and bottom.
local function canvas_tint(dark: boolean): (Rgba, Rgba)
	if dark then
		return alpha(black, 0.40), alpha(black, 0.50)
	end
	return alpha(white, 0.66), alpha(white, 0.56)
end

-- Whether `ink`, which may be translucent, reaches `target` on every one
-- of `grounds`.
local function reads(ink: Rgba, grounds: { Rgba }, target: number): boolean
	for _, ground in grounds do
		if reads_at(ink, ground) < target then
			return false
		end
	end
	return true
end

-- Text that steps back: `ink` at the lowest opacity, from `floor` up, at
-- which it still reads on every ground.
local function recede(ink: Rgba, grounds: { Rgba }, floor: number): Rgba
	if reads(alpha(ink, floor), grounds, TEXT) then
		return alpha(ink, floor)
	end
	-- More opaque always reads better: halve the range six times.
	local low, high = floor, 1
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
	-- (Twice: moved for a later ground, it may have left an earlier one.)
	for _ = 1, 2 do
		for _, ground in grounds do
			if reads_at(color, ground) < target then
				color = gmnb.with_contrast(color, ground, target)
			end
		end
	end
	return color
end

-- What the lights give way to. On a key: its text at the opacity function
-- keys have. On a panel and on the veil: the text colour at `STEP_BACK`
-- (the lower, the dimmer the aurora there and the further captions can
-- step back), and the palette's accent moved `KEPT` of the way to white
-- (to black, in the light) and no further: so that what is written in
-- the accent is still the accent.
local ON_KEYS = 0.86
local STEP_BACK = 0.70
local KEPT = 0.3

-- One thing that has to stay readable on a see-through window: an ink, the
-- contrast it needs, and what it lies on above the frost (bottom-most
-- first): a pane of glass, the tint of a lit button.
type Check = { ink: Rgba, target: number, on: { Rgba } }

-- A palette, and everything worked out from it.
type Scheme = {
	dark: boolean,
	sky_top: Rgba,
	sky_bottom: Rgba,
	-- The aurora's colours, each as strong as the text lets it be, and the
	-- brightest a star may be.
	lights: { Rgba },
	star: Rgba,
	glass: Glass,
	fg: Rgba,
	accent: Rgba,
	-- The accent as the palette has it: what tints are made of.
	tint: Rgba,
	hot_a: Rgba,
	hot_b: Rgba,
	on_hot: Rgba,
	series: { Rgba },
	-- The text colour stepping back, three steps: function keys, the
	-- buttons that are only text, and captions.
	strong: Rgba,
	soft: Rgba,
	dim: Rgba,
	-- The accent as text on something tinted with the accent, and as an
	-- operator key's glyph on its tinted glass.
	accent_ink: Rgba,
	operator_ink: Rgba,
	-- Errors.
	danger: Rgba,
	-- The hot gradient deep enough for small text.
	lit_a: Rgba,
	lit_b: Rgba,
	-- What has to hold on a see-through window: for text straight on the
	-- frost, for text on glass over it, and on the graph's canvas.
	bare: { Check },
	under_glass: { Check },
	plotted: { Check },
	-- The frost worked out so far, by the window's opacity.
	frosts: { [number]: { number } },
	frosts_count: number,
}

-- Generated palettes also pick white or `dark_text` for the equals key's
-- glyph; the built-in ones keep the colour their author chose.
local function finish(p: Palette, dark_text: Rgba?): Scheme
	local top, bottom = p.sky_top, p.sky_bottom
	local fg = p.fg
	local glass = glass_for(p.dark, bottom)
	local veil = alpha(bottom, glass.veil)
	-- The farthest an ink can be moved: what `legible` ends at.
	local extreme = if p.dark then white else black

	-- The equals key's glyph against its whole gradient (a large glyph: 3),
	-- and a deeper gradient for the small text of whatever is lit: a toggle
	-- key that is on, a set bit, the playful chip.
	local on_hot, hot_a, hot_b = p.on_hot, p.hot_a, p.hot_b
	if dark_text then
		on_hot, hot_a, hot_b = gmnb.readable_gradient(hot_a, hot_b, white, dark_text, UI)
	else
		hot_a, hot_b = gmnb.gradient_for_text(on_hot, hot_a, hot_b, UI)
	end
	local lit_a, lit_b = gmnb.gradient_for_text(on_hot, hot_a, hot_b, TEXT)

	-- What text lies on: a key's glass, top to bottom; and the panes that
	-- carry everything else, the veil and a panel's glass. Under some of
	-- that text the pane is a little lighter or more coloured: the wash of
	-- a field, a chip or a hovered row, the tint of whatever is chosen.
	local key_panes = { glass.top, glass.bottom, glass.number }
	local text_panes = { veil, glass.panel_top, glass.panel_bottom }
	local wash = alpha(fg, 0.10)
	local tint = alpha(p.accent, 0.24)
	local key_tint = alpha(p.accent, 0.20)
	local kept = mix(p.accent, extreme, KEPT)

	-- The lights. Each is brought toward the sky's lightness, by
	-- thirty-seconds of the way, until all that is written over it reads.
	local _, _, level = gmnb.to_hsl(bottom)
	local on_keys_ink, step_back_ink = alpha(fg, ON_KEYS), alpha(fg, STEP_BACK)
	local furthest = lightness(extreme.r, extreme.g, extreme.b)
	local kept_light = lightness(kept.r, kept.g, kept.b)
	local function bearable(light: Rgba): boolean
		for _, pane in key_panes do
			local r, g, b = laid(pane, light.r, light.g, light.b)
			if ratio(lightness(laid(on_keys_ink, r, g, b)), lightness(r, g, b)) < TEXT * 1.03 then
				return false
			end
			-- An operator's glyph needs 3, on its tint.
			if ratio(furthest, lightness(laid(key_tint, r, g, b))) < UI * 1.05 then
				return false
			end
		end
		for _, pane in text_panes do
			local r, g, b = laid(pane, light.r, light.g, light.b)
			local ground = lightness(r, g, b)
			if ratio(lightness(laid(step_back_ink, r, g, b)), ground) < TEXT * 1.03 then
				return false
			end
			if ratio(kept_light, ground) < TEXT then
				return false
			end
			local wr, wg, wb = laid(wash, r, g, b)
			if ratio(lightness(laid(fg, wr, wg, wb)), lightness(wr, wg, wb)) < TEXT * 1.03 then
				return false
			end
			local tr, tg, tb = laid(tint, r, g, b)
			if ratio(lightness(laid(fg, tr, tg, tb)), lightness(tr, tg, tb)) < TEXT * 1.03 then
				return false
			end
		end
		return true
	end
	local function tempered(light: Rgba): Rgba
		local h, s, l = gmnb.to_hsl(light)
		local function at(step: number): Rgba
			return hsl(h, s, level + (l - level) * step / 32)
		end
		if bearable(at(32)) then
			return at(32)
		end
		-- Nearer the sky always reads better: halve the range five times.
		-- (At the sky's own lightness, step 0, the text reads as it does
		-- on the sky.)
		local fits, too_much = 0, 32
		while too_much - fits > 1 do
			local middle = (fits + too_much) // 2
			if bearable(at(middle)) then
				fits = middle
			else
				too_much = middle
			end
		end
		return at(fits)
	end
	local lights = {}
	for i, light in p.lights do
		lights[i] = tempered(light)
	end
	local star = tempered(hex("#ececec"))

	-- What the window shows at full opacity: the sky's two colours, the
	-- lights, and in the dark a star. And what text lies on: each pane over
	-- each of those.
	local backs = { top, bottom }
	for _, light in lights do
		table.insert(backs, light)
	end
	if p.dark then
		table.insert(backs, star)
	end
	local function under(panes: { Rgba }, more: Rgba?): { Rgba }
		local out = {}
		for _, back in backs do
			for _, pane in panes do
				local ground = over(pane, back)
				table.insert(out, if more then over(more, ground) else ground)
			end
		end
		return out
	end
	local on_keys, on_panes = under(key_panes), under(text_panes)
	local function both(a: { Rgba }, b: { Rgba }): { Rgba }
		local out = table.clone(a)
		table.move(b, 1, #b, #out + 1, out)
		return out
	end

	-- 0.2 had these at .86, .78 and around .6 of the text colour whatever
	-- the palette. Those are the floors now: where a curtain is bright
	-- behind them they need more. Function keys' text lies on keys; the
	-- rest on panes, some of it on a wash (a bit, a field, a small button
	-- on a panel).
	local strong = recede(fg, both(on_keys, on_panes), ON_KEYS)
	local soft = recede(fg, both(on_panes, under(text_panes, alpha(fg, 0.07))), 0.78)
	local dim = recede(fg, both(on_panes, under(text_panes, alpha(fg, 0.07))), 0.62)

	-- The accent as text: on the panes, and on the sky itself. As text on
	-- a tint of itself (a lit text button, the current number base). And
	-- as an operator's glyph, which is a sign and not text to read: 3.
	local open = both(on_panes, { top, bottom })
	local accent = legible(p.accent, open, TEXT * 1.03)
	local accent_ink = legible(accent, under(text_panes, tint), TEXT * 1.03)
	-- (6% over: more than the see-through checks below ask, so that the
	-- frost doesn't come in with a step at the first hundredth of clear.)
	local operator_ink = legible(accent_ink, under(key_panes, key_tint), UI * 1.06)
	-- Errors. 0.2 wrote them in the hot colour as it came: 2:1 in the light
	-- Aurora, and teal in Abyss, where it didn't look like an error at all.
	-- A rose reads as one in every palette.
	local danger = legible(hsl(348, 0.9, if p.dark then 0.72 else 0.46), open, TEXT * 1.03)

	-- Graph lines: on the canvas's own glass over all of it, and on the sky.
	local canvas_top, canvas_bottom = canvas_tint(p.dark)
	local plot = { top, bottom }
	for _, back in backs do
		table.insert(plot, over(canvas_top, back))
		table.insert(plot, over(canvas_bottom, back))
	end
	local series = {}
	for i, c in p.series do
		series[i] = legible(c, plot, UI * 1.02)
	end

	-- See-through: what has to hold wherever the desktop shows through.
	-- The text colour keeps the 4.5 it has on an opaque window; everything
	-- fainter, and everything coloured, keeps at least 3.
	local margin = 1.05
	local function check(ink: Rgba, target: number, on: { Rgba }): Check
		return { ink = ink, target = target * margin, on = on }
	end
	local bare: { Check } = {
		check(fg, TEXT, {}),
		check(dim, UI, {}),
		check(accent, UI, {}),
		check(danger, UI, {}),
		-- On a tint: lit buttons; fields, bits and chips.
		check(accent_ink, UI, { alpha(p.accent, 0.12) }),
		check(fg, UI, { tint }),
		check(dim, UI, { wash }),
	}
	-- Under glass: keys, the panels, and the small buttons on them.
	local under_glass: { Check } = {
		check(fg, TEXT, { glass.top }),
		check(fg, TEXT, { glass.number }),
		check(fg, TEXT, { glass.panel_top }),
		check(fg, TEXT, { glass.panel_bottom }),
		check(strong, UI, { glass.top }),
		check(strong, UI, { glass.bottom }),
		-- An operator key: its tint thins from top to bottom.
		check(operator_ink, UI, { glass.top, key_tint }),
		check(operator_ink, UI, { glass.bottom, alpha(p.accent, 0.07) }),
		check(dim, UI, { glass.panel_top }),
		check(dim, UI, { glass.panel_bottom }),
		check(dim, UI, { glass.panel_top, alpha(fg, 0.07) }),
		check(dim, UI, { glass.panel_bottom, alpha(fg, 0.07) }),
	}
	-- The graph: every curve, the axes and the labels, under the canvas's
	-- own glass. Curves only just reach 3 on the opaque background, so
	-- this comes out close to 1: a graph needs to know what it is on.
	local plotted: { Check } = {}
	for _, smoke in { canvas_top, canvas_bottom } do
		table.insert(plotted, { ink = dim, target = UI * 1.01, on = { smoke } })
		for _, c in series do
			table.insert(plotted, { ink = c, target = UI * 1.01, on = { smoke } })
		end
	end

	return {
		dark = p.dark,
		sky_top = top,
		sky_bottom = bottom,
		lights = lights,
		star = star,
		glass = glass,
		fg = fg,
		accent = accent,
		tint = p.accent,
		hot_a = hot_a,
		hot_b = hot_b,
		on_hot = on_hot,
		series = series,
		strong = strong,
		soft = soft,
		dim = dim,
		accent_ink = accent_ink,
		operator_ink = operator_ink,
		danger = danger,
		lit_a = lit_a,
		lit_b = lit_b,
		bare = bare,
		under_glass = under_glass,
		plotted = plotted,
		frosts = {},
		frosts_count = 0,
	}
end

-- A colour option's saved text, or `fallback` if someone edited it into nonsense.
local function colour(value: OptionValue?, fallback: string): Rgba
	if type(value) == "string" and (#value == 7 or #value == 9) and string.match(value, "^#%x+$") then
		return hex(value)
	end
	return hex(fallback)
end

local function build(id: string, ctx: Ctx): Scheme
	local function generated(primary: Rgba, secondary: Rgba): Scheme
		local hue = gmnb.to_hsl(primary)
		return finish(generate(primary, secondary, ctx.dark), hsl(hue, 0.4, 0.10))
	end
	if id == "system" then
		local primary = ctx.accent or hex(FALLBACK_ACCENT)
		return generated(primary, gmnb.complement(primary))
	elseif id == "custom" then
		return generated(
			colour(ctx.options.primary, DEFAULT_PRIMARY),
			colour(ctx.options.secondary, DEFAULT_SECONDARY)
		)
	end
	local make = BUILT_IN[id] or BUILT_IN.aurora
	return finish(make(ctx.dark), nil)
end

-- Finishing a palette takes a few thousand contrast checks, and the draw
-- hooks ask for the scheme every frame: remember what was built.
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

-- On a see-through window the sky thins to clear air, as thin as the
-- window's Background opacity says, and the desktop shows through it. The
-- aurora stays: its lights hang over the desktop itself, a little clearer
-- than they are in the night sky (this much of them is left when the sky
-- is all but gone), so that the desktop shows through them as the stars
-- show through an aurora.
local SEE_THROUGH = 0.7

-- And as the sky clears the glass frosts over, whatever the text needs:
-- by this much of the way to solid when the sky is gone.
local FROSTS_OVER = 0.4

-- How much of each light is drawn at this opacity of the window.
local function light_strength(opacity: number): number
	return if opacity >= 1 then 1 else SEE_THROUGH + (1 - SEE_THROUGH) * math.max(opacity, 0)
end

-- How strong the frost under `checks` has to be for each of them to hold
-- over every one of `backs`: the sky's own colour, in sixty-fourths, and
-- `least` at least.
local function frost_needed(base: Rgba, backs: { Rgba }, checks: { Check }, least: number): number
	local base_r, base_g, base_b = base.r, base.g, base.b
	local function holds(strength: number): boolean
		local clear = 1 - strength
		for _, back in backs do
			local fr = base_r * strength + back.r * clear
			local fg = base_g * strength + back.g * clear
			local fb = base_b * strength + back.b * clear
			for _, c in checks do
				local r, g, b = fr, fg, fb
				for _, layer in c.on do
					r, g, b = laid(layer, r, g, b)
				end
				if ratio(lightness(laid(c.ink, r, g, b)), lightness(r, g, b)) < c.target then
					return false
				end
			end
		end
		return true
	end
	-- Down from solid to the last step that holds: by sixteenths, then by
	-- sixty-fourths within the last of those. (Not a search by halves:
	-- over a desktop darker than a dark ink the ink reads again once the
	-- frost is nearly gone, and that is not the answer.)
	local floor = math.ceil(least * 64 - 0.001)
	local step = 64
	for _, by in { 4, 1 } do
		while step - by >= floor and holds((step - by) / 64) do
			step -= by
		end
	end
	return step / 64
end

-- The frost a see-through window needs: a layer of the sky's own colour
-- under whatever carries text, as strong as this palette needs at this
-- opacity and no stronger. Three of them: the veil under text straight
-- on the page, what goes under glass, and what the graph's canvas needs.
--
-- What such a window shows is the desktop, which may be black or white
-- or anything between, through the thinned sky; and the lights over
-- that. An opaque window needs none but the veil it always has (the
-- lights were tempered for that). The frost comes in by sixty-fourths as
-- the window clears: dragging the slider, nothing jumps.
local function frost_for(s: Scheme, opacity: number): (number, number, number)
	local veil = s.glass.veil
	if opacity >= 1 then
		return veil, 0, 0
	end
	local known = s.frosts[opacity]
	if known then
		return known[1], known[2], known[3]
	end
	local strength = light_strength(opacity)
	local all: { { light: number, colour: Rgba } } = {}
	for _, desktop in { black, white } do
		for _, sky in { s.sky_top, s.sky_bottom } do
			local air = over(alpha(sky, opacity), desktop)
			table.insert(all, { light = contrast(air, black), colour = air })
			for _, light in s.lights do
				local lit = over(alpha(light, strength), air)
				table.insert(all, { light = contrast(lit, black), colour = lit })
			end
			if s.dark then
				-- A star, going out with the sky.
				local lit = over(alpha(s.star, opacity * opacity), air)
				table.insert(all, { light = contrast(lit, black), colour = lit })
			end
		end
	end
	-- What decides is the lightest of those under light text and the
	-- darkest under dark text: the two at each end are gone through.
	table.sort(all, function(a, b)
		return a.light < b.light
	end)
	local backs: { Rgba } = {}
	for i, back in all do
		if i <= 2 or i > #all - 2 then
			table.insert(backs, back.colour)
		end
	end
	local base = s.sky_bottom
	-- And never less than this, the clearer the window: what shows through
	-- may be a photograph, busy behind a key's sign in a way no contrast
	-- figure knows about. The glass frosts over as the sky clears.
	local clear = 1 - math.max(opacity, 0)
	local found = {
		frost_needed(base, backs, s.bare, veil + (1 - veil) * FROSTS_OVER * clear),
		frost_needed(base, backs, s.under_glass, FROSTS_OVER * clear),
		frost_needed(base, backs, s.plotted, 0),
	}
	-- (Dragging the opacity slider asks for a new one each step.)
	if s.frosts_count >= 64 then
		s.frosts, s.frosts_count = {}, 0
	end
	s.frosts[opacity] = found
	s.frosts_count += 1
	return found[1], found[2], found[3]
end

---------------------------------------------------------------------------
-- 3. Style
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
	local fg, accent, accent_ink = s.fg, s.accent, s.accent_ink
	local strong, soft, dim = s.strong, s.soft, s.dim
	-- What tints are made of: the accent as the palette has it.
	local tint = s.tint
	local base = s.sky_bottom
	-- 0.2 left whatever it didn't set in Outfit to the desktop's own font.
	local system = ctx.font

	-- What carries text has something of the sky's own colour under it
	-- (section 2): the veil, under text that lies straight on the page;
	-- and on a see-through window, where the desktop shows through the
	-- aurora, the frost, under glass too. Each as strong as this palette
	-- needs at this opacity and no stronger.
	local see = ctx.opacity < 1
	local veil_alpha, backing_alpha, canvas_alpha = frost_for(s, ctx.opacity)
	local frost = alpha(base, veil_alpha)
	local backing = alpha(base, backing_alpha)
	-- `paint` over the frost.
	local function on_frost(paint: Paint): Fill
		local layers: { Paint } = { paint, backing }
		return if backing_alpha > 0 then layers else paint
	end
	-- The backdrop's colour for a surface that was translucent anyway: at
	-- its own strength, or the veil's if that is more.
	local function sheet(opacity: number): Rgba
		return alpha(base, math.max(opacity, veil_alpha))
	end
	-- A halo in the backdrop's colour around text that lies straight on it,
	-- for the busy wallpaper no contrast figure knows about.
	-- (It comes in over the first quarter of the way to clear.)
	local clear = math.min((1 - ctx.opacity) * 4, 1)
	local halo: { Shadow } = if see
		then { { blur = 3, color = alpha(base, 0.9 * clear) }, { blur = 10, color = alpha(base, 0.75 * clear) } }
		else {}

	-- Glass.
	local glass = s.glass
	local pane = down(glass.top, glass.bottom)
	local panel_pane = down(glass.panel_top, glass.panel_bottom)
	-- A tint of the accent on a key's glass.
	local function on_glass(paint: Paint): Fill
		local layers: { Paint } = { paint, pane }
		if backing_alpha > 0 then
			table.insert(layers, backing)
		end
		return layers
	end
	local shade = if dark then alpha(black, 0.32) else alpha(hex("#3a2a6a"), 0.10)
	local highlight: Shadow = { inset = true, y = 1, color = alpha(white, 0.10) }
	local rim: Shadow = { inset = true, spread = 1, color = glass.rim }

	local hot = slant(s.hot_a, s.hot_b)
	-- Whatever is lit and carries small text: a toggle key that is on, a
	-- set bit, the playful chip. 0.2 gave the first and the last the hot
	-- gradient at 0.85, on which their text fell to 2.5:1 in the light
	-- palettes; this one is solid, and deepened until the text reads.
	local lit = slant(s.lit_a, s.lit_b)

	-- What the toolkit's own widgets are coloured with: dialogs, menus,
	-- lists, cards and the text on the accent colour.
	local window = if dark then s.sky_bottom else s.sky_top
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
	-- Something chosen: the accent as text, on a tint of it.
	local chosen: State = { background = alpha(tint, 0.12), color = accent_ink }

	-- Shadows, listed topmost first: a highlight along the top edge, a
	-- hairline rim, and a shade along the bottom edge that gives the glass
	-- its thickness.
	--
	-- (0.2's keys also had a soft drop shadow under them: 18 px of blur,
	-- drawn 12 px in, so faint that pictures with it and without can't be
	-- told apart. But every key's was blurred again for every frame of the
	-- drift, since the sky moves behind them all: half of what a living
	-- Aurora cost the software renderer, 31% of a core against 17%. A pane
	-- of glass over the sky casts no shadow on it anyway.)
	local thickness: Shadow = { inset = true, y = -1, color = alpha(black, if dark then 0.22 else 0.05) }
	local key_shadow: { Shadow } = { highlight, rim, thickness }
	local operator_shadow: { Shadow } = {
		highlight,
		{ inset = true, spread = 1, color = alpha(accent, 0.22) },
		thickness,
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

	-- The memory row's buttons, DEG, F-E and their like: text that lights up.
	local function text_button(padding: Spacing): Control
		return {
			min_width = 30,
			min_height = 30,
			padding = padding,
			radius = 10,
			font = outfit(13, 600, 0.04),
			color = soft,
			background = gmnb.transparent,
			transition = { background = 150, color = 150 },
			hover = { background = alpha(fg, 0.07), color = fg },
			active = { background = alpha(fg, 0.07), color = fg },
			checked = chosen,
			disabled = { color = alpha(fg, 0.26) },
		}
	end

	-- The graph's canvas: its glass, with the frost a graph needs mixed
	-- in (it takes one paint, not layers).
	local canvas_top, canvas_bottom = canvas_tint(dark)
	local canvas_frost = alpha(base, canvas_alpha)

	return {
		-- Tokens. Anything not styled below is derived from these.
		colors = {
			bg = base,
			surface = popover,
			surface2 = alpha(fg, 0.06),
			border = alpha(fg, 0.12),
			fg = fg,
			fg_dim = dim,
			fg_faint = alpha(fg, 0.38),
			accent = accent,
			on_accent = on_accent,
			accent_text = accent_ink,
			hover = alpha(fg, 0.06),
			press = alpha(fg, 0.12),
			danger = s.danger,
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
		-- Drawn by the toolkit, as in 0.2, in the `stock` colours. (Not the
		-- focus ring: the toolkit's is the accent at half opacity, 2:1 on
		-- the page and on the keys. Aurora's is below.)
		native = { "segmented", "slider", "switch", "scrollbar", "menu", "toast", "tooltip", "button", "card" },
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
			-- The sky. The app thins it to the window's Background opacity,
			-- and the background layer draws the aurora over it.
			background = down(s.sky_top, s.sky_bottom),
			color = fg,
			font = { size = system.size, weight = 400 },
			text_halo = halo,
		},
		-- Whatever holds text straight on the page (the memory row, the
		-- number bases, the bits, a converter's values, a date's fields)
		-- lies on a pane: the veil, and on a see-through window as much
		-- frost as that needs. 0.2 had nothing here, and such text sank to
		-- 1:1 over a desktop of its own colour.
		plain = { background = frost, radius = KEY_RADIUS },
		-- The keyboard's focus: the accent, solid, a pixel out from what
		-- holds it. Round a key it lies on the aurora itself, sky in one
		-- place and a curtain in the next, and on whatever desktop shows
		-- through: so a line of the sky's own colour runs along each side
		-- of it, and the accent is moved 9:1 from that line. Then one of
		-- the two reads, 3:1 or more, on any colour there is.
		focus = {
			width = 2,
			color = gmnb.with_contrast(accent, base, UI * UI * 1.01),
			offset = 3,
			edge = { width = 1, color = base },
		},
		header = {
			background = frost,
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
			background = down(sheet(0.94), sheet(0.88)),
			shadow = { { inset = true, x = -1, color = alpha(fg, 0.08) } },
			width = 300,
			show_title = false,
			header = { font = outfit(12, 600, 0.12), color = dim, margin = { 14, 18, 4, 18 } },
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
					background = across(alpha(tint, 0.24), alpha(tint, 0.06)),
					shadow = { { inset = true, x = 3, color = accent } },
					color = fg,
					icon = { color = accent_ink },
				},
			},
			settings = { padding = { 10, 12 }, radius = 12, min_height = 0, hover = flat_hover, active = flat_active },
		},

		display = {
			background = frost,
			radius = 18,
			padding_x = 14,
			heights = { standard = "auto", scientific = "auto", programmer = "auto", compact = "auto" },
			sizes = { standard = 64, scientific = 54, programmer = 46, compact = 44 },
			min_size = 18,
			value = {
				font = { family = "Outfit", weight = 250, features = { "tnum", "lnum" } },
				-- The display layer below paints the value; these are what it
				-- would be without it.
				color = fg,
				error_color = s.danger,
				error_scale = 0.62,
				glow = false,
			},
			expression = { font = outfit(15), color = dim },
			expression_min_size = 15,
			halo = halo,
		},

		keys = {
			gap = 5,
			gap_compact = 3,
			radius = KEY_RADIUS,
			-- No least size: in the least window, 300 × 400, the keys take
			-- what room is left, as the base's do (and so do the bits).
			min_width = 0,
			min_height = 0,
			padding = { 0, 2 },
			icon = { size = 22, stroke = 1.9 * (0.6 + 0.4 * 22 / 24) },
			-- (320 ms: the keypad layer's SPRING_BACK.)
			transition = { background = 160, shadow = 220, transform = { ms = 320, ease = "spring" }, filter = 160 },
			hover = { brightness = 1.12 },
			active = { scale = 0.93, brightness = 1.22, transition = { transform = 70, filter = 70 } },
			-- A key that can't be used now (A to F outside hexadecimal) is
			-- thinner glass with a faint glyph. 0.2 faded the whole key, which
			-- on a see-through window would fade its frost away too.
			disabled = {
				background = on_frost(down(alpha(glass.top, 0.4), alpha(glass.bottom, 0.4))),
				shadow = { { inset = true, spread = 1, color = alpha(glass.rim, 0.4) } },
				fade = 0.3,
			},
			number = {
				font = outfit(22, 450),
				color = fg,
				background = on_frost(down(glass.top, glass.number)),
				border = false,
				shadow = key_shadow,
			},
			func = {
				font = outfit(16),
				color = strong,
				background = on_frost(pane),
				border = false,
				shadow = key_shadow,
			},
			operator = {
				font = outfit(22, 350),
				color = s.operator_ink,
				background = on_glass(down(alpha(tint, 0.20), alpha(tint, 0.07))),
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
			on = { color = s.on_hot, background = lit },
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
			checked = chosen,
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
			placeholder = dim,
			selection = alpha(accent, 0.3),
			caret = fg,
			focus = { border = { width = 2, color = alpha(accent, 0.5) } },
			error = { border = { width = 2, color = alpha(s.danger, 0.5) }, color = s.danger },
		},
		row = { radius = 9, hover = flat_hover, active = flat_active, checked = { background = alpha(fg, 0.10) } },
		section = { font = outfit(13, 600, 0.04), color = dim, margin = { 6, 0, 0, 0 } },
		popover = {
			padding = 8,
			radius = 18,
			background = down(alpha(base, 0.98), alpha(base, 0.95)),
			border = false,
			shadow = flyout_shadow,
		},

		panel = {
			background = on_frost(panel_pane),
			radius = 20,
			border = false,
			shadow = panel_shadow,
			padding = { 10, 6, 6, 6 },
			sheet = { background = down(alpha(base, 0.97), alpha(base, 0.94)) },
			tabs = { stretch = false },
			row = {
				radius = 14,
				margin = { 2, 4 },
				padding = { 8, 12 },
				transition = { background = 150 },
				hover = { background = alpha(fg, 0.06) },
				active = { background = alpha(fg, 0.06) },
			},
			expression = { font = outfit(13), color = dim },
			result = { font = outfit(24, 350), color = fg },
			delete = {
				min_width = 22,
				min_height = 22,
				radius = 999,
				color = soft,
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
				color = soft,
				background = alpha(fg, 0.06),
				hover = { background = alpha(fg, 0.07), color = fg },
				active = { background = alpha(fg, 0.07), color = fg },
				transition = { background = 150, color = 150, opacity = 160 },
			},
			memory_reveal = "hover",
			empty = { font = { size = 14 }, color = dim },
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
						background = across(alpha(tint, 0.22), alpha(tint, 0)),
						shadow = { { inset = true, x = 3, color = accent } },
					},
				},
				name = { font = outfit(12, 650, 0.08), color = dim },
				name_checked = accent_ink,
				value = { font = { family = "Outfit", size = 15, weight = 400, features = { "tnum" } }, color = fg },
			},
			bit = {
				min_width = 0,
				min_height = 0,
				padding = 0,
				radius = 8,
				font = outfit(15),
				color = dim,
				background = alpha(fg, 0.04),
				shadow = { { inset = true, spread = 1, color = alpha(fg, 0.06) } },
				transition = { background = 200, color = 200, shadow = 200, opacity = 200 },
				hover = { background = alpha(fg, 0.10) },
				active = { background = alpha(fg, 0.10) },
				checked = {
					color = s.on_hot,
					background = lit,
					shadow = bit_glow,
					font = { weight = 400 },
				},
				disabled = { opacity = 0.25 },
			},
			bit_index = { font = outfit(10), color = dim, margin = { 0, 0, 0, 2 } },
		},

		date = {
			label = { font = outfit(13, 600, 0.04), color = dim, margin = { 6, 0, 0, 0 } },
			field = {
				min_height = 44,
				padding = { 0, 14 },
				radius = 14,
				font = outfit(16),
				color = fg,
				background = pane,
				border = false,
				shadow = { highlight, rim },
				-- Over the glass, not instead of it.
				hover = { overlay = alpha(fg, 0.05) },
				active = { overlay = alpha(fg, 0.12) },
			},
			-- (The page's column is a pane of frost already.)
			result_panel = {
				background = pane,
				radius = 20,
				border = false,
				shadow = panel_shadow,
				padding = { 12, 18, 16, 18 },
				margin = { 10, 0, 0, 0 },
			},
			result = { size = 34, font = { family = "Outfit", weight = 300 } },
			add_result = { size = 30, font = { family = "Outfit", weight = 300 } },
			subresult = { font = outfit(17), color = dim },
			calendar = {
				title = { font = outfit(15, 600), color = fg },
				weekday = { font = { size = 12 }, color = dim },
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
				focus = { width = 2, color = accent, offset = 1 },
			},
		},

		converter = {
			field = {
				radius = 18,
				padding = { 6, 4, 8, 4 },
				transition = { background = 200, shadow = 240 },
				checked = {
					background = across(alpha(tint, 0.14), alpha(tint, 0.02)),
					shadow = { { inset = true, x = 3, color = accent } },
				},
			},
			marker = false,
			value = {
				size = 44,
				font = { family = "Outfit", weight = 300 },
				checked_weight = 300,
				focus = { width = 2, color = accent, offset = 2, radius = 12 },
			},
			symbol = { font = outfit(26, 300), color = dim, margin = { 0, 0, 0, 14 } },
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
			whimsy = { color = s.on_hot, background = lit },
			whimsy_icon = true,
			ratio = { font = outfit(15), color = fg, margin = { 4, 14, 0, 14 } },
			timestamp = { font = { size = 12 }, color = dim, margin = { 0, 0, 0, 14 } },
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
				background = on_frost(panel_pane),
				radius = 20,
				border = false,
				shadow = panel_shadow,
				padding = 10,
			},
			divider = false,
			row = { padding = 4, radius = 14 },
			row_error = { background = alpha(s.danger, 0.08) },
			error = { font = { size = 12 }, color = s.danger, margin = { 0, 0, 0, 40 } },
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
				font = outfit(15),
				color = dim,
				background = gmnb.transparent,
				shadow = { { inset = true, spread = 1, color = alpha(fg, 0.12) } },
				hover = { color = fg, background = alpha(fg, 0.05) },
				active = { color = fg, background = alpha(fg, 0.05) },
			},
			variable = { font = { family = "Outfit", size = 17, weight = 400, italic = true }, color = fg },
			analysis = {
				title = { font = outfit(18), color = fg, margin = { 2, 8, 6, 8 } },
				card = { padding = { 8, 12 }, radius = 14, background = alpha(fg, 0.04), border = false },
				card_title = { font = outfit(12, 600, 0.06), color = dim },
				value = { font = outfit(16), color = fg },
				text = { font = { size = 14 }, color = soft },
				note = { font = { size = 12, italic = true }, color = dim },
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
				background = down(over(canvas_top, canvas_frost), over(canvas_bottom, canvas_frost)),
				radius = 20,
				border = { width = 1, color = alpha(fg, if dark then 0.12 else 0.18) },
				grid_minor = alpha(fg, if dark then 0.045 else 0.06),
				grid_major = alpha(fg, if dark then 0.10 else 0.12),
				axis = { color = dim, width = 1.4 },
				label = { font = outfit(11), color = dim },
				fill_opacity = 0.18,
				-- Neon in the dark.
				glow = if dark then { blur = 6, width = 3.2, opacity = 0.55 } else false,
				trace = {
					hair_opacity = 0.45,
					hair_dash = { 4, 4 },
					halo = { grow = 3, color = "series", opacity = 0.25 },
					ring = { width = 1.5, color = alpha(white, 0.9) },
					bubble = {
						background = if dark then alpha(s.sky_bottom, 0.92) else alpha(white, 0.95),
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
-- 4. Background: the aurora
---------------------------------------------------------------------------
--
-- Over the sky, which is the window's own background (section 3): its
-- stars, the aurora with a ray of light wherever something just
-- happened, and film grain.
--
-- An aurora is curtains of light. Each hangs along a line that wanders
-- across the sky, its hem: sharp and bright there, and fading upward in
-- rays, some tall and some short. Folds travel slowly along it. Its
-- colours go by height: green lowest, then teal, and violet and crimson
-- at the crown.
--
-- How bright the curtains are is a field (docs/theming-media.md,
-- "Fields"): one number a point on a grid of some 9,000, three times a
-- second; the app glides between two of those and makes the picture.
-- That picture is only ever white, more or less opaque. It is drawn as a
-- mask, and what shows through it is the palette's lights, a gradient by
-- height.
-- (The colours can't live in the field's ramp: a theme may make eight
-- ramps, and Freestyle's palettes are without number.)
--
-- So every pixel of the background is the sky, or one of the palette's
-- lights at its height, or something between the two: nothing brighter
-- than a light is ever drawn, which is what section 2 counts on.
--
-- On a see-through window the app thins the sky, and the layer thins
-- itself (`fade = false`): the stars go out with the sky, sooner than it
-- does, and the lights stay, hanging over the desktop (section 2,
-- `SEE_THROUGH`).

local floor, ceil, sin, cos, min, max, sqrt, clamp =
	math.floor, math.ceil, math.sin, math.cos, math.min, math.max, math.sqrt, math.clamp
local readf32, writef32 = buffer.readf32, buffer.writef32
local TAU = 2 * math.pi

-- How strong the light is: none is clear, all of it is white.
local LIGHT = gmnb.ramp({ colors = { alpha(white, 0), white } })

-- Where each of a palette's lights is, from the top of the window, as a
-- fraction of its height: the crown first. Below the last, the light
-- stays what it is there.
local HEIGHTS = { 0.04, 0.22, 0.35, 0.45, 0.58, 0.80 }

-- 256 numbers from 0 to 1, the same every time: what rays and stars are
-- made from.
local NOISE: { number } = {}
do
	local s = 0x2545f491
	for i = 1, 256 do
		s = bit32.bxor(s, bit32.lshift(s, 13))
		s = bit32.bxor(s, bit32.rshift(s, 17))
		s = bit32.bxor(s, bit32.lshift(s, 5))
		NOISE[i] = (s % 65536) / 65535
	end
end

-- Smooth noise along a line: 0 to 1, a new number every whole step.
local function noise(x: number): number
	local i = floor(x)
	local f = x - i
	local a, b = NOISE[i % 256 + 1], NOISE[(i + 1) % 256 + 1]
	return a + (b - a) * f * f * (3 - 2 * f)
end

-- A curtain. Heights are fractions of the window's; lengths along it are
-- in units of the window's height too (or of its width and a fifth, in a
-- narrow window), so that a wide window shows more of the sky and not a
-- stretched one.
--   y, lean   where its hem hangs, from the top, in the middle of the
--             window; and how much lower it is at the right edge than at
--             the left
--   waves     the folds of the hem, each { how high, how long, how fast
--             (radians a second), where it starts }
--   tall      how far up its rays reach
--   bright    how strong its light is, 0 to 1
--   fine      how wide a ray is; `broad`, a patch of them
--   drift     how fast the rays move along it (units a second)
type Curtain = {
	y: number,
	lean: number,
	waves: { { number } },
	tall: number,
	bright: number,
	fine: number,
	broad: number,
	drift: number,
	seed: number,
}
local CURTAINS: { Curtain } = {
	-- The near one: across the whole window, rising to the right.
	{
		y = 0.61,
		lean = -0.12,
		waves = { { 0.034, 1.70, 0.045, 0.9 }, { 0.013, 0.52, -0.075, 2.0 }, { 0.005, 0.17, 0.120, 4.1 } },
		tall = 0.50,
		bright = 1.0,
		fine = 0.020,
		broad = 0.17,
		drift = 0.004,
		seed = 0,
	},
	-- Where it folds back on itself: the same curtain a little further
	-- off, so a little higher, shorter and fainter.
	{
		y = 0.52,
		lean = -0.07,
		waves = { { 0.030, 1.55, 0.045, 2.3 }, { 0.012, 0.44, -0.070, 5.2 } },
		tall = 0.36,
		bright = 0.55,
		fine = 0.017,
		broad = 0.14,
		drift = 0.003,
		seed = 41,
	},
	-- A far one behind them, high up, small and faint.
	{
		y = 0.29,
		lean = 0.07,
		waves = { { 0.020, 1.10, -0.035, 4.0 }, { 0.007, 0.31, 0.065, 0.4 } },
		tall = 0.24,
		bright = 0.5,
		fine = 0.013,
		broad = 0.11,
		drift = -0.003,
		seed = 97,
	},
}
-- How much of the window's height the curtains can take up, from the top:
-- the field covers that much and no more, so its rows are spent where the
-- hems are.
local REACH = 0.80
-- A curtain is brightest in a band along its hem, this much of the
-- window's height deep.
local BAND = 0.075
-- Under its hem its light doesn't stop dead: this much of it glows on
-- below, for this much of the window's height.
local UNDERGLOW, UNDER = 0.27, 0.10

-- Fifteen pictures a second while it drifts (a fold moves about 5 px a
-- second), and the field three times a second: the app glides between
-- two snapshots of it.
local SHOWN_RATE = 15
local FIELD_RATE = 3
local SHOWN_PER_FIELD = SHOWN_RATE / FIELD_RATE
local DRIFT_FRAME = 0.066
-- How long a pulse lasts, in seconds; and how long till its next picture:
-- thirty a second. (A soft light needs no more, and every picture of the
-- background is a picture of the whole window. A little under a thirtieth,
-- so that the app draws it on the screen's own frames, every other one.)
local PULSE_LIFE = 1.15
local PULSE_FRAME = 0.032

-- How many points the grid has, about, in cells half again as tall as
-- wide (rays are thin and stand upright), none narrower than 3 px: a
-- bigger window gets a coarser grid, not more to work out. And about as
-- many pixels for the app to colour as a 760 × 640 window's aurora has at
-- half size.
--
-- (A field may have 16,384 points, and the aurora is a little finer with
-- them all. But the first frame at a window size works out a whole
-- snapshot, a hook has 8 ms, and on a machine busy with something else
-- the same work takes twice as long: with 9,000 that frame is 2 to 3 ms
-- where it was 4 to 5, and a picture of each side by side is hard to tell
-- apart.)
local POINTS = 9000
local FINEST = 3
local PIXELS = 100000

-- Film grain: it hides the gradients' banding and makes them feel like a
-- material. White specks in the dark, black ones in the light.
local GRAIN = 192
local grain_dark = gmnb.noise({ size = GRAIN, seed = 0x9e3779b9, color = white, opacity = 0.05 })
local grain_light = gmnb.noise({ size = GRAIN, seed = 0x9e3779b9, color = black, opacity = 0.04 })

-- How far the aurora has drifted, in seconds. It only moves on while
-- "Living background" is on (`frame.dt` is 0 whenever the app isn't
-- animating); where it starts is the picture it shows standing still.
local STILL = 37.0
local drift = STILL

-- How much coarser than `POINTS` and `PIXELS` this machine needs both:
-- 1, unless it is slow (`keep`, below, finds out).
local thin = 1

local function grid(width: number, height: number): (number, number)
	local step = max(FINEST, sqrt(width * height * thin / (POINTS * 1.5)))
	return clamp(ceil(width / step) + 1, 2, 256), clamp(ceil(height / (step * 1.5)) + 1, 2, 256)
end

-- How many pixels the app colours per pixel of the field, along a side.
local function sharpness(width: number, height: number): number
	return clamp(sqrt(PIXELS / thin / (width * height)), 0.1, 1)
end

-- What the background keeps between frames: two snapshots of the field
-- and the one that comes after them (worked out a few columns a frame,
-- so that no frame carries a whole snapshot), and each curtain's columns.
type Kept = {
	width: number,
	height: number,
	-- The field's own height: `REACH` of the window's.
	reach: number,
	cols: number,
	rows: number,
	a: buffer,
	b: buffer,
	c: buffer,
	-- Per curtain and column, for the snapshot being worked out: the
	-- hem's height in pixels, one over how far up its rays reach, and how
	-- bright it is at the hem and in its rays.
	hem: { { number } },
	inv: { { number } },
	crest: { { number } },
	body: { { number } },
	-- Which third of a second `a` is of, whether the snapshot after `b`
	-- has been begun, and how many of its columns are done.
	tick: number,
	begun: boolean,
	done: number,
	-- The fifteenth shown, and how far from `a` to `b` it is.
	shown: number,
	mix: number,
}
local kept: Kept? = nil

local function make(width: number, height: number): Kept
	local reach = height * REACH
	local cols, rows = grid(width, reach)
	local size = cols * rows * 4
	local hem, inv, crest, body = {}, {}, {}, {}
	for c = 1, #CURTAINS do
		hem[c], inv[c] = table.create(cols, 0), table.create(cols, 0)
		crest[c], body[c] = table.create(cols, 0), table.create(cols, 0)
	end
	return {
		width = width,
		height = height,
		reach = reach,
		cols = cols,
		rows = rows,
		a = buffer.create(size),
		b = buffer.create(size),
		c = buffer.create(size),
		hem = hem,
		inv = inv,
		crest = crest,
		body = body,
		tick = -1,
		begun = false,
		done = 0,
		shown = -1,
		mix = 0,
	}
end

-- Each curtain's columns at drift time `t`: where its hem is, how far up
-- its rays reach there and how bright they are.
local function columns(k: Kept, t: number)
	local width, height, cols = k.width, k.height, k.cols
	local unit = min(height, width * 1.2)
	for c, curtain in CURTAINS do
		local hem, inv, crest, body = k.hem[c], k.inv[c], k.crest[c], k.body[c]
		local waves = curtain.waves
		local slide = t * curtain.drift
		for i = 1, cols do
			local across = (i - 1) / (cols - 1)
			-- Along the curtain, from the window's middle.
			local x = (across - 0.5) * width / unit
			local y = curtain.y + curtain.lean * (across - 0.5)
			-- How steeply the hem runs: a fold seen edge-on is brighter.
			local slope = 0
			for _, wave in waves do
				local at = TAU * x / wave[2] + wave[3] * t + wave[4]
				y += wave[1] * sin(at)
				slope += wave[1] * TAU / wave[2] * cos(at)
			end
			slope *= height / unit
			-- Rays: fine ones, some tall and some short, in broad patches
			-- that come and go.
			local along = x + slide
			local fine = noise(along / curtain.fine + curtain.seed)
			local finer = noise(along / (curtain.fine * 0.37) + curtain.seed * 1.9 + 40)
			local patch = noise(along / curtain.broad + curtain.seed * 3.7 + t * 0.021)
			local tall = noise(along / (curtain.broad * 0.6) + curtain.seed * 5.3 - t * 0.017)
			local ray = 0.7 * fine + 0.3 * finer
			local fold = min(1 + 6 * slope * slope, 1.4)
			hem[i] = y * height
			inv[i] = 1 / (curtain.tall * height * (0.62 + 0.58 * tall) * (0.85 + 0.30 * ray))
			crest[i] = curtain.bright * min((0.66 + 0.34 * patch) * fold, 1)
			body[i] = curtain.bright * (0.34 + 0.66 * ray * ray) * (0.62 + 0.38 * patch) * fold
		end
	end
end

-- The light of columns `first` up to (not including) `last` (counted from
-- 0) of a snapshot that started out dark, from the columns worked out
-- last: each curtain's added to the others'. Column by column, and in
-- each only the rows the curtain lights there, from where its rays end
-- down to where its glow under the hem does: half the grid is dark, and
-- is never visited.
local function light_columns(k: Kept, buf: buffer, first: number, last: number)
	local cols, rows = k.cols, k.rows
	local pitch = k.reach / (rows - 1)
	local down = cols * 4
	-- Under its hem a curtain's light is all but gone within a row and a
	-- bit: an edge thinner than the grid would come out in steps.
	local edge = 1 / max(pitch * 1.3, 3)
	local glows = UNDER * k.height
	local under = 1 / glows
	local band = 1 / (BAND * k.height)
	for c = 1, #CURTAINS do
		local hems, invs, crests, bodies = k.hem[c], k.inv[c], k.crest[c], k.body[c]
		for i = first, last - 1 do
			local hem, inv, crest, body = hems[i + 1], invs[i + 1], crests[i + 1], bodies[i + 1]
			-- Above the hem: each ray fading as far up as it reaches, and
			-- a band along the hem where the curtain is brightest. A
			-- ray's light grows by the same step from row to row: most
			-- of the grid's points are a sum and no more.
			local top = max(ceil((hem - 1 / inv) / pitch), 0)
			local at = min(floor(hem / pitch), rows - 1)
			local banded = max(ceil((hem - 1 / band) / pitch), top)
			local step = body * pitch * inv
			local ray = body * (1 - hem * inv) + top * step
			local o = (top * cols + i) * 4
			for _ = top, min(banded - 1, at) do
				writef32(buf, o, readf32(buf, o) + ray)
				ray += step
				o += down
			end
			for j = banded, at do
				local b = 1 - (hem - j * pitch) * band
				writef32(buf, o, readf32(buf, o) + ray + (crest - ray) * b * b)
				ray += step
				o += down
			end
			-- Under it: the sharp edge, in the row or two it takes, and
			-- the glow, which fades by the same step from row to row.
			local from = max(at + 1, 0)
			local fade = crest * UNDERGLOW * pitch * under
			local glow = crest * UNDERGLOW * (1 + (hem - from * pitch) * under)
			o = (from * cols + i) * 4
			for j = from, min(floor((hem + glows) / pitch), rows - 1) do
				local u = 1 + (hem - j * pitch) * edge
				if u > 0 then
					writef32(buf, o, readf32(buf, o) + glow + crest * (1 - UNDERGLOW) * u)
				else
					writef32(buf, o, readf32(buf, o) + glow)
				end
				glow -= fade
				o += down
			end
		end
	end
end

-- A draw hook has 8 ms of the processor, and loses its place for good
-- after three frames that take longer. A whole snapshot takes about a
-- millisecond on the machine this was written on. Where one would take
-- over 2, the grid and the picture are made as much coarser as brings it
-- to 1.5. The first twenty-four columns tell, each time a window's size
-- is new: three batches of eight, of which the quickest counts (the
-- machine may have been busy with something else during the others).
-- (`os.clock` is the clock on the wall, so a machine that is busy for a
-- moment looks slow for that moment: a size that looks slow is timed
-- again before the grid is made coarser, and a size that is comfortably
-- quick makes it finer again, the next time the window's size is new.)
local SLOW, AFFORDED = 0.002, 0.0015

-- A whole snapshot's time at this size, as far as the first twenty-four
-- columns tell, and the snapshot with the columns that were worked out.
local function timed(width: number, height: number): (Kept, number, number)
	local made = make(width, height)
	local started = os.clock()
	columns(made, drift)
	local spent = os.clock() - started
	local column, quickest = 0, math.huge
	while column + 8 <= min(made.cols, 24) do
		started = os.clock()
		light_columns(made, made.a, column, column + 8)
		quickest = min(quickest, os.clock() - started)
		column += 8
	end
	return made, spent + quickest / 8 * made.cols, column
end

-- What is kept for a picture of this size, and whether it was just made:
-- then with one whole snapshot, of the aurora where it has drifted to,
-- which is both of its two until the next is ready.
local function keep(width: number, height: number): (Kept, boolean)
	local k = kept
	if k and k.width == width and k.height == height then
		return k, false
	end
	local made, whole, column = timed(width, height)
	if column > 0 and whole > SLOW and thin < 16 then
		-- Slow, or only busy for a moment: the quicker of two timings.
		local again, whole_again, column_again = timed(width, height)
		if whole_again < whole then
			made, whole, column = again, whole_again, column_again
		end
	end
	local steer = column > 0 and ((whole > SLOW and thin < 16) or (whole < AFFORDED / 2 and thin > 1))
	if steer then
		thin = clamp(thin * whole / AFFORDED, 1, 16)
		made = make(width, height)
		columns(made, drift)
		column = 0
	end
	light_columns(made, made.a, column, made.cols)
	buffer.copy(made.b, 0, made.a)
	kept = made
	return made, true
end

-- The field for the fifteenth of a second the aurora has drifted to, the
-- next snapshot a few columns further on.
local function advance(k: Kept, fresh: boolean)
	local shown = floor(drift * SHOWN_RATE + 0.03)
	if not fresh and shown == k.shown then
		-- Drawn again within the same fifteenth (a pulse plays, the colours
		-- changed): the same numbers, and the app has that picture already.
		return
	end
	local tick = floor(shown / SHOWN_PER_FIELD)
	if k.tick < 0 then
		k.tick = tick
	elseif tick ~= k.tick and k.begun then
		-- A new third of a second: the later snapshot becomes the earlier
		-- one, the one that was being worked out the later (what is left of
		-- it is done now, if frames were missed), and the next is begun.
		if k.done < k.cols then
			light_columns(k, k.c, k.done, k.cols)
		end
		k.a, k.b, k.c = k.b, k.c, k.a
		k.tick = tick
		k.begun = false
	end
	if not k.begun then
		columns(k, (k.tick + 2) / FIELD_RATE)
		buffer.fill(k.c, 0, 0)
		k.done = 0
		k.begun = true
	end
	-- A few columns of the next snapshot: all of it in four frames of five.
	if k.done < k.cols then
		local upto = min(k.cols, k.done + ceil(k.cols / (SHOWN_PER_FIELD - 1)))
		light_columns(k, k.c, k.done, upto)
		k.done = upto
	end
	k.mix = clamp((shown - k.tick * SHOWN_PER_FIELD) / SHOWN_PER_FIELD, 0, 1)
	k.shown = shown
end

-- The stars: where each is (fractions of the window), how big (pixels),
-- how bright (of the brightest a star may be). The same sky every night.
local STARS: { { number } } = {}
for i = 1, 160 do
	local size = NOISE[(i * 7) % 256 + 1]
	STARS[i] = {
		NOISE[(i * 3 + 11) % 256 + 1],
		NOISE[(i * 5 + 101) % 256 + 1],
		1.1 + 1.5 * size * size,
		0.45 + 0.55 * NOISE[(i * 13 + 57) % 256 + 1] ^ 2,
	}
end
-- A star to every so many square pixels of window.
local STAR_ROOM = 3000

-- Each star's colour for a palette at an opacity of the window, kept
-- from frame to frame: the brightest a star may be, more or less of it.
local starlight: { of: Rgba?, opacity: number, colours: { Rgba } } = { of = nil, opacity = -1, colours = {} }

local function star_colours(star: Rgba, opacity: number): { Rgba }
	if starlight.of ~= star or starlight.opacity ~= opacity then
		-- They go out as the sky clears, sooner than it does.
		local out = opacity * opacity
		local colours = {}
		for i, s in STARS do
			colours[i] = alpha(star, s[4] * out)
		end
		starlight = { of = star, opacity = opacity, colours = colours }
	end
	return starlight.colours
end

-- A soft light: a radial gradient centred on (cx, cy), painted only where
-- it reaches inside the window.
local function light(
	draw: Draw,
	w: number,
	h: number,
	cx: number,
	cy: number,
	rx: number,
	ry: number,
	colors: { Rgba },
	stops: { number }
)
	local left, top = max(cx - rx, 0), max(cy - ry, 0)
	local right, bottom = min(cx + rx, w), min(cy + ry, h)
	if right > left and bottom > top then
		draw:rect(left, top, right - left, bottom - top, gmnb.radial({
			center = { cx, cy },
			radius = rx,
			radius_y = ry,
			colors = colors,
			stops = stops,
		}))
	end
end

-- A pulse: a ray of light that rises from where something happened, as
-- far as `reach`, and fades. The strong ones (the equals key, a result,
-- the converter's swap) light the sky around them too. It is drawn into
-- the aurora's own mask: what it shows is the palette's lights where it
-- stands, and never anything brighter.
local function pulse_at(draw: Draw, w: number, h: number, x: number, y: number, strength: number, reach: number, age: number)
	local life = age / PULSE_LIFE
	-- It comes up over two frames instead of popping in.
	local rise = ease.out_quad(min(age / 0.06, 1))
	local grown = ease.out_cubic(life)
	local a = min((1 - life) ^ 1.6 * 0.6 * strength * rise, 1)
	-- An ellipse standing on the point.
	local ry = 26 + grown * reach * 0.5
	local rx = 24 + grown * (26 + reach * 0.09)
	light(draw, w, h, x, y + 14 - ry * 0.85, rx, ry, { alpha(white, a), alpha(white, a * 0.5), alpha(white, 0) }, { 0, 0.5, 1 })
	if strength >= 1 then
		local b = (1 - life) ^ 2 * 0.3 * strength * rise
		light(draw, w, h, x, y, 60 + grown * w * 0.45, 40 + grown * h * 0.32, { alpha(white, b), alpha(white, 0) }, { 0, 1 })
	end
end

-- How strong the pulse a press makes is, and how far up its ray reaches,
-- as a fraction of the window's height.
local function press_pulse(e: Event): (number, number)
	if e.source == "key" and e.key then
		local key = e.key
		-- Each key's ray is one of four heights: the choice the sounds
		-- make of its note (section 8), so the higher the note, the
		-- higher the light.
		local id = key.id or 0
		local reach = 0.30 + 0.12 * (id * (if e.page == "converter" then 5 else 7) % 4)
		if e.page == "graphing" then
			return 0.4, reach
		elseif e.via == "keyboard" then
			return if e.page == "converter" then 0.5 else 0.6, reach
		elseif e.page ~= "converter" and key.kind == "equals" then
			return 1.4, 0.7
		end
		return 0.55, reach
	elseif e.source == "swap" then
		return 1.0, 0.6
	elseif e.source == "date" then
		return 0.9, 0.5
	end
	-- The memory row, F-E.
	return 0.5, 0.36
end

local function background(draw: Draw, frame: Frame): Again
	local s = scheme(frame.ctx)
	local w, h = frame.width, frame.height
	local living = frame.ctx.options.living == true
	if living then
		drift += frame.dt
	end

	if w < 2 or h < 2 then
		return nil
	end
	-- How see-through the window is: the sky under this layer is that
	-- thin already.
	local opacity = clamp(frame.ctx.opacity, 0, 1)
	-- The sky's stars, in the dark: specks, none brighter than a light
	-- may be. They go out as the sky clears.
	if s.dark then
		local colours = star_colours(s.star, opacity)
		for i = 1, min(#STARS, floor(w * h / STAR_ROOM)) do
			local star = STARS[i]
			local size = star[3]
			draw:rect(star[1] * w - size / 2, star[2] * h - size / 2, size, size, colours[i])
		end
	end

	-- The aurora's light, and the light of whatever just happened.
	local k, fresh = keep(w, h)
	if living and frame.motion then
		advance(k, fresh)
	end
	draw:push_mask("alpha")
	draw:field(0, 0, w, k.reach, {
		data = k.a,
		next = k.b,
		mix = k.mix,
		cols = k.cols,
		rows = k.rows,
		channels = 1,
		ramp = LIGHT,
		quality = sharpness(w, k.reach),
	})
	local pulsing = false
	for _, e in frame.events do
		if e.age < PULSE_LIFE then
			if e.kind == "press" then
				local strength, reach = press_pulse(e)
				pulsing = true
				pulse_at(draw, w, h, e.x, e.y, strength, reach * h, e.age)
			elseif (e.kind == "result" or e.kind == "error") and e.page == "calculator" then
				-- From under the last digits of the readout.
				pulsing = true
				local strength = if e.kind == "error" then 0.6 else 1.1
				pulse_at(draw, w, h, e.rect.x + e.rect.w * 0.8, e.rect.y + e.rect.h * 0.65, strength, 0.3 * h, e.age)
			elseif e.kind == "option" and e.id == "palette" then
				-- A new palette introduces itself: four rays the height of
				-- the window, left to right, as its chord is played.
				pulsing = true
				for i = 0, 3 do
					local age = e.age - 0.09 * i
					if age > 0 then
						pulse_at(draw, w, h, w * (0.2 + 0.2 * i), h * 0.92, 0.9, h * 0.95, age)
					end
				end
			end
		end
	end
	draw:pop()
	-- The lights that show through it: all of each in the night sky, a
	-- little clearer over a desktop.
	local lights = s.lights
	local strength = light_strength(opacity)
	draw:rect(0, 0, w, h, gmnb.linear({
		from = { 0, 0 },
		to = { 0, h },
		colors = {
			alpha(lights[6], strength),
			alpha(lights[5], strength),
			alpha(lights[4], strength),
			alpha(lights[3], strength),
			alpha(lights[2], strength),
			alpha(lights[1], strength),
		},
		stops = HEIGHTS,
	}))
	draw:pop()

	-- The grain is the sky's: it thins with it.
	if opacity < 1 then
		draw:push_opacity(opacity)
	end
	draw:tile(if s.dark then grain_dark else grain_light, 0, 0, w, h)
	if opacity < 1 then
		draw:pop()
	end

	-- Thirty times a second while a pulse plays; fifteen while it only
	-- drifts; and not at all when it stands still.
	if pulsing then
		return PULSE_FRAME
	elseif living then
		return DRIFT_FRAME
	end
	return nil
end

---------------------------------------------------------------------------
-- 5. Keypad: ripples, the pointer's light, the cascade
---------------------------------------------------------------------------

local RIPPLE_LIFE = 0.55
-- How long a pressed key takes to spring back to its size (`keys.transition`
-- in the style), in seconds.
local SPRING_BACK = 0.32
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
	local glint = if s.dark then white else s.accent
	local busy = false

	-- `place` is moving keys for as long as the cascade runs.
	local show = newest(frame.events, "show")
	if show then
		local last = 0
		for _, k in keys do
			last = math.max(last, k.row + k.col)
		end
		busy = show.age <= last * CASCADE_STEP + CASCADE_TIME
	end

	-- Ripples spread from where a key was pressed, and stay inside it: a
	-- wash of light with a brighter rim.
	local springing: { [number]: boolean } = {}
	for _, e in frame.events do
		if e.kind == "press" and e.source == "key" and e.key and e.page ~= "graphing" and e.age < RIPPLE_LIFE then
			busy = true
			-- (The style shrinks a pressed key and springs it back.)
			if e.key.id then
				springing[e.key.id] = e.age < SPRING_BACK
			end
			local life = e.age / RIPPLE_LIFE
			local r = e.rect
			local reach = math.sqrt(r.w * r.w + r.h * r.h) * 1.1
			local a = (1 - life) ^ 1.4 * (if s.dark then 0.26 else 0.30)
			local color = if e.key.kind == "equals" then s.on_hot else glint
			draw:push_clip(r.x, r.y, r.w, r.h, KEY_RADIUS)
			draw:rect(r.x, r.y, r.w, r.h, gmnb.radial({
				center = { e.x, e.y },
				radius = 6 + ease.out_cubic(life) * reach,
				colors = { alpha(color, a * 0.4), alpha(color, a * 0.7), alpha(color, a), alpha(color, 0) },
				stops = { 0, 0.6, 0.88, 1 },
			}))
			draw:pop()
		end
	end

	-- Reveal: the borders of the keys near the pointer catch its light, and
	-- the key under it glows.
	local p = frame.pointer
	if p then
		local border = if s.dark then 0.55 else 0.75
		local glow = if s.dark then 0.12 else 0.16
		for _, k in keys do
			local r = k.rect
			local nx = math.clamp(p.x, r.x, r.x + r.w)
			local ny = math.clamp(p.y, r.y, r.y + r.h)
			local away = math.sqrt((p.x - nx) ^ 2 + (p.y - ny) ^ 2)
			-- (Not a key that is pressed or springing back: its border
			-- would be drawn where the key, shrunk, no longer is.)
			local shrunk = k.pressed or (k.id ~= nil and springing[k.id])
			if k.enabled and not shrunk and away <= REVEAL_RADIUS then
				local outline = gmnb.path():rounded_rect(r.x + 0.5, r.y + 0.5, r.w - 1, r.h - 1, KEY_RADIUS - 0.5)
				draw:stroke(outline, 1.25, gmnb.radial({
					center = { p.x, p.y },
					radius = REVEAL_RADIUS,
					colors = { alpha(glint, border), alpha(glint, border * 0.3), alpha(glint, 0) },
					stops = { 0, 0.55, 1 },
				}))
				if away == 0 then
					draw:push_clip(r.x, r.y, r.w, r.h, KEY_RADIUS)
					draw:rect(r.x, r.y, r.w, r.h, gmnb.radial({
						center = { p.x, p.y },
						radius = math.max(r.w, r.h) * 0.95,
						colors = { alpha(glint, glow), alpha(glint, 0) },
					}))
					draw:pop()
				end
			end
		end
	end

	return if busy then 0 else nil
end

---------------------------------------------------------------------------
-- 6. Display: gradient digits, glow, and how a new value arrives
---------------------------------------------------------------------------
--
-- The app lays the value out and hands over its glyphs; this paints them.
-- The same hook paints the converter's two values and the date page's
-- results (`frame.value.role` says which).

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
			return 0, math.sqrt(p), 0.55 + 0.45 * ease.out_back(p)
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

	-- A glow in the dark: the digits again, blurred, in the accent (or in
	-- the error's colour).
	if s.dark and digits > 0 then
		draw:push_blur(v.size * 0.30)
		glyphs(alpha(if v.error then s.danger else s.accent, 0.42))
		draw:pop()
	end

	-- The digits are a window onto a gradient: text colour at the top,
	-- leaning to the accent at the bottom (or to the error's, in error).
	local top = if v.error then alpha(s.fg, 0.92) else s.fg
	local bottom = if v.error then s.danger else mix(s.fg, s.accent, if s.dark then 0.55 else 0.45)
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
-- 7. Options
---------------------------------------------------------------------------

-- The curtains in a window the size of a tile, standing still: one small
-- field serves every palette's tile.
local tile_kept: Kept? = nil

local function tile_light(w: number, h: number): Kept
	local k = tile_kept
	if k and k.width == w and k.height == h then
		return k
	end
	local made = make(w, h)
	columns(made, STILL)
	light_columns(made, made.a, 0, made.cols)
	tile_kept = made
	return made
end

-- A palette's tile in Settings: its sky, its aurora, and a dot of its hot
-- gradient.
local function tile(draw: Draw, frame: Preview, choice: string)
	local s = scheme_for(choice, frame.ctx)
	local w, h = frame.width, frame.height
	draw:push_clip(0, 0, w, h, 12)
	draw:rect(0, 0, w, h, gmnb.linear({ from = { 0, 0 }, to = { 0, h }, colors = { s.sky_top, s.sky_bottom } }))
	if w >= 2 and h >= 2 then
		local k = tile_light(w, h)
		draw:push_mask("alpha")
		draw:field(0, 0, w, k.reach, { data = k.a, cols = k.cols, rows = k.rows, channels = 1, ramp = LIGHT, quality = 1 })
		draw:pop()
		local lights = s.lights
		draw:rect(0, 0, w, h, gmnb.linear({
			from = { 0, 0 },
			to = { 0, h },
			colors = { lights[6], lights[5], lights[4], lights[3], lights[2], lights[1] },
			stops = HEIGHTS,
		}))
		draw:pop()
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
		description = "Accent, keys and the aurora's hem",
		default = DEFAULT_PRIMARY,
		visible = is("custom"),
	},
	{
		id = "secondary",
		type = "color",
		group = "Palette",
		label = "Secondary colour",
		description = "The aurora's crown and the = key's gradient",
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

---------------------------------------------------------------------------
-- 8. Sound
---------------------------------------------------------------------------
--
-- Aurora is glass with light behind it, and sounds like it: a fingernail
-- on a pane under each key, a struck pane in a small room for a result, a
-- breath for what is cleared away. It is the theme to leave on, so every
-- sound is short, soft at the top and quieter than the sound chapter's
-- examples (docs/theming-media.md, "Designing sounds").
--
-- Two recipes make all of it:
--   a pane   a sine that rings, with a bright edge that is gone long before
--            the ring is, and a grain of noise for the touch. Short and dry
--            it is a key; long, doubled and in a room, a bell.
--   air      noise through a band that moves: clear, and a change of page.
--
-- And the notes. Every palette has a chord (`VOICES`). Number keys ring
-- its notes, each key its own, as each sends its ray of light so high in
-- the background; a result after = is two of them, rising; a palette newly
-- chosen plays all four. Every chord has an A in it, so the sounds that
-- don't change with the palette (they are on A and E) fit them all.

-- A pane's ring at `pitch`, `seconds` long. `shine` is how bright its edge
-- is (the index of an FM pair at `ratio`: 2.76 and 3.5 are glass, 2 is
-- hollow, like wood) and `edge` how many seconds that takes to go.
local function ring(pitch: Pitch | PitchGlide, seconds: number, shine: number, ratio: number, edge: number): SynthLayer
	local index: Glide = { shine, 0.02, time = edge }
	return {
		freq = pitch,
		fm = { ratio = ratio, index = index },
		env = { attack = 0.0015, decay = seconds },
	}
end

-- The touch itself: a grain of noise around `freq` hertz.
local function grain(seed: number, freq: number, level: number, seconds: number): SynthLayer
	return {
		wave = "noise",
		seed = seed,
		filter = { type = "bandpass", freq = freq, q = 1.2 },
		env = { attack = 0, decay = seconds },
		gain = level,
	}
end

-- `layer` with what `more` says instead.
local function with(layer: SynthLayer, more: SynthLayer): SynthLayer
	local out = table.clone(layer)
	for key, value in more :: { [string]: any } do
		(out :: { [string]: any })[key] = value
	end
	return out
end

-- Number keys: two takes of one tick, played in turn so that no two
-- presses are the same sound.
local tick_a = gmnb.synth({
	gain = 0.27,
	layers = { ring("E6", 0.04, 1.1, 2.76, 0.014), grain(11, 4200, 0.10, 0.006) },
})
local tick_b = gmnb.synth({
	gain = 0.27,
	layers = { ring("E6", 0.042, 0.8, 2.76, 0.018), grain(12, 4800, 0.08, 0.006) },
})

-- Operators: rounder. Lower than any key's note (on A5), a softer attack,
-- a hollow edge.
local tock = gmnb.synth({
	gain = 0.26,
	layers = {
		with(ring("A5", 0.05, 0.6, 2, 0.02), { env = { attack = 0.003, decay = 0.05 } }),
		grain(13, 2400, 0.05, 0.008),
	},
})

-- Function keys and buttons: drier and shorter, more touch than ring.
local tap = gmnb.synth({
	gain = 0.24,
	layers = { ring("E6", 0.03, 0.5, 2.76, 0.008), grain(14, 3000, 0.16, 0.007) },
})

-- The equals key: the tick and the tock at once, on A. Dry: the chime
-- that follows is the result's.
local set = gmnb.synth({
	gain = 0.32,
	layers = {
		ring("A5", 0.062, 0.7, 2, 0.02),
		with(ring("A6", 0.05, 1.0, 2.76, 0.012), { gain = 0.6 }),
		grain(15, 3600, 0.10, 0.006),
	},
})

-- 2nd and hyp: two ticks, the second a fourth up.
local flip = gmnb.synth({
	gain = 0.24,
	layers = {
		ring("E6", 0.028, 0.8, 2.76, 0.01),
		with(ring("A6", 0.036, 0.6, 2.76, 0.01), { at = 0.034, gain = 0.8 }),
		grain(16, 4200, 0.08, 0.006),
	},
})

-- Backspace: a tick that falls away.
local back = gmnb.synth({
	gain = 0.22,
	layers = {
		ring({ "B6", "E6", time = 0.028 } :: PitchGlide, 0.04, 0.4, 2.76, 0.01),
		grain(17, 3200, 0.07, 0.006),
	},
})

-- Clear: a breath that falls, and a thread of glass falling with it.
local sweep = gmnb.synth({
	gain = 0.26,
	layers = {
		{
			wave = "noise",
			seed = 21,
			filter = { type = "bandpass", freq = { 3400, 650, time = 0.07 } :: Glide, q = 1.5 },
			env = { attack = 0.008, decay = 0.066 },
		},
		{ freq = { "A6", "A5", time = 0.06 } :: PitchGlide, env = { attack = 0.004, decay = 0.06 }, gain = 0.12 },
	},
})

-- A change of page or mode: air rising, and a fifth of glass coming up
-- under it and going.
local swell_fx: { SynthEffect } = {
	{ type = "highpass", freq = 200 },
	{ type = "reverb", size = 0.5, damp = 0.4, mix = 0.22 },
}
local swell = gmnb.synth({
	gain = 0.15,
	length = 0.8,
	layers = {
		{
			wave = "noise",
			seed = 22,
			filter = { type = "bandpass", freq = { 500, 2600, time = 0.18 } :: Glide, q = 1.3 },
			env = { attack = 0.08, decay = 0.2 },
			gain = 0.8,
		},
		{ freq = "A5", detune = -4, pan = -0.4, env = { attack = 0.09, decay = 0.3 }, gain = 0.5 },
		{ freq = "E6", detune = 4, pan = 0.4, env = { attack = 0.11, decay = 0.28 }, gain = 0.35 },
	},
	effects = swell_fx,
})

-- An error: two soft knocks, low, each falling, the second lower and
-- fainter. They are as far apart as the swings of the readout's shaken
-- head (46 radians a second there: 137 ms a swing). Triangles, so that
-- small speakers, which have nothing this low, still get their overtones;
-- and nothing under 120 Hz for the speakers that do.
local function knock(from: string, to: string, at: number, level: number, seed: number): (SynthLayer, SynthLayer)
	return {
		wave = "triangle",
		freq = { from, to, time = 0.1 } :: PitchGlide,
		env = { attack = 0.004, decay = 0.22 },
		at = at,
		gain = level,
	}, {
		wave = "noise",
		seed = seed,
		filter = { type = "lowpass", freq = 700, q = 0.7 },
		env = { attack = 0.001, decay = 0.03 },
		at = at,
		gain = level * 0.3,
	}
end
local knock_1, thump_1 = knock("E4", "B3", 0, 1, 23)
local knock_2, thump_2 = knock("B3", "F#3", 0.137, 0.7, 24)
local thud_fx: { SynthEffect } = { { type = "highpass", freq = 140 } }
local thud = gmnb.synth({
	gain = 0.22,
	layers = { knock_1, thump_1, knock_2, thump_2 },
	effects = thud_fx,
})

-- A bell: two panes on A5 a few cents apart, left and right, so that the
-- ring beats slowly and shimmers; one more partial, `colour`, that says
-- what it is made of; in a small room. One note: chords and chimes are
-- made by playing it at other speeds (`rate`), which also makes high
-- notes short and low ones long, as struck things are. `level` is its
-- gain: the purer a bell, the lower, to sound as loud as the others.
local bell_fx: { SynthEffect } = {
	{ type = "highpass", freq = 250 },
	{ type = "reverb", size = 0.55, damp = 0.35, mix = 0.25 },
}
local function bell(level: number, shine: number, ratio: number, attack: number, seed: number, colour: SynthLayer): Sound
	local function pane(detune: number, pan: number): SynthLayer
		return with(ring("A5", 1.0, shine, ratio, 0.25), {
			detune = detune,
			pan = pan,
			env = { attack = attack, decay = 1.0 },
		})
	end
	return gmnb.synth({
		gain = level,
		length = 1.25,
		layers = { pane(-3, -0.6), pane(3, 0.6), colour, grain(seed, 5200, 0.06, 0.008) },
		effects = bell_fx,
	})
end
-- Glass: a bright edge, and the partial a struck bar or pane has at 2.76
-- times its note, which is no note of any scale.
local bell_glass = bell(0.16, 1.6, 3.5, 0.002, 31, {
	freq = 880 * 2.76,
	env = { attack = 0.002, decay = 0.45 },
	gain = 0.15,
})
-- Warm: a hollow edge and a hum an octave under, like a wooden bar.
local bell_warm = bell(0.165, 0.9, 2, 0.004, 32, {
	freq = "A4",
	env = { attack = 0.006, decay = 0.6 },
	gain = 0.45,
})
-- A bowl: almost no edge, a slow start, a quiet octave above. A rubbed rim.
local bell_bowl = bell(0.14, 0.25, 3.5, 0.02, 33, {
	freq = "A6",
	detune = 5,
	env = { attack = 0.03, decay = 0.7 },
	gain = 0.2,
})

-- A palette's voice: its bell, the chord it introduces itself with, the
-- two notes of its chime, and the notes its number keys ring.
type Voice = { bell: Sound, chord: { number }, chime: { number }, keys: { number } }

-- Notes as speeds: of the bell (it is on A5), and of the ticks (on E6).
local function speeds(notes: { string }, home: string): { number }
	local out = {}
	for i, note in notes do
		out[i] = gmnb.note(note) / gmnb.note(home)
	end
	return out
end

local function voice(bell_sound: Sound, chord: { string }, chime: { string }, keys: { string }): Voice
	return {
		bell = bell_sound,
		chord = speeds(chord, "A5"),
		chime = speeds(chime, "A5"),
		keys = speeds(keys, "E6"),
	}
end

-- System and Freestyle are whatever colours you bring: an open chord.
local YOURS = voice(bell_glass, { "A5", "B5", "E6", "F#6" }, { "B5", "E6" }, { "E6", "F#6", "A6", "B6" })
local VOICES: { [string]: Voice } = {
	-- A major seventh, close: luminous, unresolved.
	aurora = voice(bell_glass, { "A5", "C#6", "E6", "G#6" }, { "E6", "A6" }, { "C#6", "E6", "G#6", "A6" }),
	-- F sharp minor seventh, low and warm.
	ember = voice(bell_warm, { "F#5", "A5", "C#6", "E6" }, { "C#6", "F#6" }, { "C#6", "E6", "F#6", "A6" }),
	-- Fifths and fourths, wide apart: depth.
	abyss = voice(bell_bowl, { "G5", "D6", "G6", "A6" }, { "G5", "D6" }, { "D6", "E6", "G6", "A6" }),
	-- From D major's five-note scale: sunny.
	orchard = voice(bell_glass, { "A5", "B5", "D6", "F#6" }, { "D6", "F#6" }, { "D6", "F#6", "A6", "B6" }),
	-- No third at all: neither major nor minor, as grey is no colour.
	graphite = voice(bell_bowl, { "A5", "D6", "E6", "A6" }, { "A5", "A6" }, { "D6", "E6", "A6" }),
	system = YOURS,
	custom = YOURS,
}

local function voice_for(ctx: Ctx): Voice
	local id = ctx.options.palette
	return (if type(id) == "string" then VOICES[id] else nil) or VOICES.aurora
end

-- Which sound for which event. This table is the whole of it for most
-- events, and all of it if `sound` below is ever switched off.
local sounds: Sounds = {
	-- Each key keeps its pitch and its place across the window.
	digit = { sound = { tick_a, tick_b }, pick = "key", rate = { 0.92, 1.09 }, pan = "position" },
	operator = { sound = tock, rate = { 0.985, 1.015 }, pan = "position" },
	func = { sound = tap, pick = "key", rate = { 0.95, 1.05 }, pan = "position" },
	equals = { sound = set, pan = "position" },
	toggle = { sound = flip, pan = "position" },
	clear = sweep,
	backspace = { sound = back, rate = { 0.97, 1.03 }, pan = "position" },
	-- The memory row: the operators' tock, a fifth up.
	memory = { sound = tock, rate = 1.498, volume = 0.9, pan = "position" },
	button = { sound = tap, volume = 0.9, pan = "position" },
	-- Any result, when `sound` isn't there to tell one after = from one
	-- after +: a single soft note, high.
	result = { sound = bell_glass, rate = 2, volume = 0.5, delay = 0.05 },
	-- It starts as the readout reaches the end of its first swing.
	error = { sound = thud, delay = 0.034 },
	mode = swell,
}

-- What the table can't know.
--
-- The app calls a result whatever gives a new value: = does, but so does
-- + after a number, and so does sin. A chime for each would be a lot of
-- chiming, so `sound` remembers the key pressed last.
local last_key = ""
-- Results in a row from = alone (pressed again, it repeats the operation).
local repeats = 0
-- Number keys pressed so far.
local turn = 0

-- One note of a palette's bell.
local function note(v: Voice, speed: number, volume: number, pan: number, delay: number): Play
	return { sound = v.bell, rate = speed, volume = volume, pan = pan, delay = delay }
end

local function sound(event: SoundEvent, ctx: Ctx): SoundChoice
	if event.kind == "press" then
		local key = event.key
		last_key = if event.action then event.action elseif key then key.kind else "button"
		if last_key ~= "equals" then
			repeats = 0
		end
		if key and last_key == "number" then
			-- The key's note in this palette's chord: picked as the
			-- background picks how high the key's ray of light reaches.
			local keys = voice_for(ctx).keys
			local id = key.id or 0
			local speed = keys[id * (if event.page == "converter" then 5 else 7) % #keys + 1]
			-- Never the same sound twice running, even on one key: the two
			-- takes in turn, and a breath of difference in pitch (a tenth
			-- of a semitone at most) and in loudness.
			turn += 1
			local play: Play = {
				sound = if turn % 2 == 0 then tick_a else tick_b,
				rate = speed * (0.994 + 0.012 * (turn * 0.618034 % 1)),
				volume = 0.86 + 0.14 * (turn * 0.414214 % 1),
				pan = event.pan * 0.6,
			}
			return play
		end
		return nil
	elseif event.kind == "result" and event.page == "calculator" then
		local v = voice_for(ctx)
		if last_key == "equals" then
			repeats += 1
			if repeats == 1 then
				-- The chime: two notes, rising. The first comes just after
				-- the key's own sound, the second while the light the key
				-- sent up is at its widest.
				local chime: { Play } = {
					note(v, v.chime[1], 0.8, -0.25, 0.05),
					note(v, v.chime[2], 0.95, 0.25, 0.19),
				}
				return chime
			end
			-- = again and again: one soft note each time, up the chord.
			return note(v, v.chord[(repeats - 2) % #v.chord + 1], 0.5, 0, 0.05)
		elseif last_key == "operator" then
			-- The operator's own tock said it.
			return false
		end
		-- A function's result (sin, a square root): one soft note.
		return note(v, v.chime[2], 0.5, 0, 0.05)
	elseif event.kind == "error" then
		repeats = 0
	elseif event.kind == "option" and event.id == "palette" then
		-- A new palette introduces itself: its chord, left to right, as
		-- the background sends up its four rays.
		local value = event.value
		local v = (if type(value) == "string" then VOICES[value] else nil) or VOICES.aurora
		local chord: { Play } = {}
		for i, speed in v.chord do
			chord[i] = note(v, speed, 0.6, (0.4 * i - 1) * 0.6, 0.09 * (i - 1))
		end
		return chord
	end
	return nil
end

local theme: Theme = {
	api = 1,
	name = "Aurora",
	description = "An aurora behind glass keys, in seven palettes: curtains of light drifting over a night sky. The look GMNB was born with.",
	author = "GMNB contributors",
	license = "MIT",
	fonts = { "fonts/Outfit-Variable.ttf" },
	credits = { { title = "Outfit typeface", file = "fonts/OFL-Outfit.txt" } },
	options = options,
	style = style,
	sounds = sounds,
	sound = sound,
	layers = {
		-- Which events each layer is told of, and for how many seconds it
		-- keeps them. (The sounds of section 8 hang from the same events.)
		-- (`fade = false`: on a see-through window the background thins
		-- itself, section 4.)
		background = {
			draw = background,
			fade = false,
			events = { press = PULSE_LIFE, result = PULSE_LIFE, error = PULSE_LIFE, option = PULSE_LIFE },
		},
		keypad = { draw = keypad, place = place, pointer = true, events = { press = RIPPLE_LIFE, show = 1.5 } },
		display = { draw = display },
	},
}

return theme
