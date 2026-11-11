--!strict
-- Sprinkles: an 8-bit doughnut shop that glazes your numbers.
--
-- The concept. GMNB is "Glaze My Numbers, Baby", and this theme takes the
-- name at its word. The window is a small pixel-art doughnut shop: a tiled
-- wall under a striped awning, keys like iced biscuits that sink under a
-- finger and stack themselves when the keypad appears, and a display case
-- for a readout. Type a sum and the digits are plain dough. Press = and
-- icing runs down them and sprinkles land on it, to a jingle in the
-- flavour's own scale, while the doughnut that lives on the shelf jumps
-- for joy. (Its mouth is its hole: it is always impressed.) Five flavours,
-- and two of your own: System, iced in the desktop's accent colour, and
-- Freestyle, in colours you pick.
--
-- Where Aurora is light behind glass and all of it computed, Sprinkles is
-- made of pictures and of matter: stills drawn by a script (the wall, the
-- awning's hem), a GIF loop (sprinkles falling), pictures worked out pixel
-- by pixel here (the icing on the digits, the doughnut), keys moved by
-- `place`, a pixel typeface (Jersey 10), and chip-tune sound. Nothing
-- eases: things step, hop, drop and land, thirty times a second at most.
--
-- The file reads top to bottom:
--   1. Flavours     the colours of each flavour, light and dark
--   2. Style        how every part of the window looks
--   3. Art pixels   the grid everything is drawn on, and a little chance
--   4. Background   the wall, the awning, sprinkles falling
--   5. Fountain     sprinkles thrown over the window for a result
--   6. Keypad       keys that sink, hop and stack; crumbs
--   7. Display      the case, the doughnut, digits plain and glazed
--   8. Options      what Settings offers, and the flavours' tiles
--   9. Sound        chip-tune: a note under every key, a jingle for a result

local hex, hsl, mix, alpha = gmnb.hex, gmnb.hsl, gmnb.mix, gmnb.alpha
local white, black = gmnb.white, gmnb.black
-- 4.5 and 3: what WCAG asks of text, and of large text and lines.
local TEXT, UI = gmnb.TEXT_CONTRAST, gmnb.UI_CONTRAST
local floor, ceil = math.floor, math.ceil

---------------------------------------------------------------------------
-- 1. Flavours
---------------------------------------------------------------------------

-- What a flavour chooses: its icing, and a second colour that goes with it
-- (the operator keys).
type Flavour = { icing: string, second: string }

local FLAVOURS: { [string]: Flavour } = {
	strawberry = { icing = "#ff6fae", second = "#62d9b5" },
	blueberry = { icing = "#8491ff", second = "#ffd84d" },
	matcha = { icing = "#8ed061", second = "#ff8fbf" },
	lemon = { icing = "#ffd43b", second = "#6cbcff" },
	caramel = { icing = "#ffa94d", second = "#b79bff" },
}

-- And two flavours of your own. System is iced in the desktop's accent
-- colour (this blue on a desktop that shares none), Freestyle in the
-- colours you pick (grape and lime until you do). Either can be any
-- colour at all, so everything below is worked out to read whatever it is.
local FALLBACK_ACCENT = "#3584e4"
local DEFAULT_ICING = "#c58cff"
local DEFAULT_SECOND = "#9be36a"

-- A colour option's saved text, or `fallback` if someone edited it into
-- nonsense. (Icing is solid: an opacity written into it is dropped.)
local function colour(value: OptionValue?, fallback: string): Rgba
	if type(value) == "string" and (#value == 7 or #value == 9) and string.match(value, "^#%x+$") then
		return hex(string.sub(value, 1, 7))
	end
	return hex(fallback)
end

-- Any colour as icing: its hue and its strength as they were given, at a
-- lightness icing has. (Black icing would hide the doughnut's line, white
-- its shine.)
local function iced(c: Rgba): Rgba
	local h, s, l = gmnb.to_hsl(c)
	return hsl(h, s, math.clamp(l, 0.34, 0.82))
end

-- What goes with an icing: a colour most of the way round the wheel from
-- it, as mint goes with strawberry and lemon with blueberry, as light as
-- theirs. A grey icing gets a lighter grey.
local function second_for(icing: Rgba): Rgba
	local h, s = gmnb.to_hsl(icing)
	if s < 0.08 then
		return hsl(h, s, 0.74)
	end
	return hsl(h + 160, math.clamp(s, 0.55, 0.9), 0.7)
end

-- A flavour's two colours for `ctx`, and a name that is the same exactly
-- when they are.
local function flavour_colours(id: string, ctx: Ctx): (Rgba, Rgba, string)
	if id == "system" then
		local icing = iced(ctx.accent or hex(FALLBACK_ACCENT))
		return icing, second_for(icing), "system " .. gmnb.to_hex(icing)
	elseif id == "custom" then
		local icing = iced(colour(ctx.options.icing, DEFAULT_ICING))
		local second = iced(colour(ctx.options.second, DEFAULT_SECOND))
		return icing, second, "custom " .. gmnb.to_hex(icing) .. " " .. gmnb.to_hex(second)
	end
	local flavour = FLAVOURS[id] or FLAVOURS.strawberry
	return hex(flavour.icing), hex(flavour.second), id
end

-- Sprinkles, wherever they are only decoration: the loop's own colours
-- (make_assets.py, SPRINKLE_COLOURS).
local SPRINKLES: { Rgba } = {
	hex("#ff6fae"),
	hex("#ffd23f"),
	hex("#5fd6b4"),
	hex("#6aa5ff"),
	hex("#ff9b54"),
	hex("#c58cff"),
}

-- A flavour in one mode, and everything worked out from it.
type Scheme = {
	-- Which flavour, in which colours and mode: the same text exactly when
	-- the scheme is the same (the pictures worked out from it are kept by it).
	key: string,
	dark: boolean,
	-- The wall, what stands on it (panels, the display case), and what
	-- stands on that (buttons, chips).
	wall: Rgba,
	card: Rgba,
	card2: Rgba,
	-- The line around things, and a soft line where that would shout.
	line: Rgba,
	soft: Rgba,
	-- Text, and text that steps back.
	ink: Rgba,
	dim: Rgba,
	faint: Rgba,
	-- The icing as a surface, its darker edge, the text on it, and the
	-- icing as text on the wall and the cards.
	icing: Rgba,
	icing_edge: Rgba,
	on_icing: Rgba,
	icing_ink: Rgba,
	-- The keys' faces and the lip under each.
	number: Rgba,
	number_edge: Rgba,
	func: Rgba,
	func_edge: Rgba,
	operator: Rgba,
	operator_edge: Rgba,
	operator_ink: Rgba,
	-- The awning's two stripes and what is written on them.
	stripe: Rgba,
	stripe_other: Rgba,
	awning_ink: Rgba,
	danger: Rgba,
	-- The icing on a glazed number, its shine and its rim, and the
	-- sprinkles on it: all of them are the number, so all of them read
	-- on the display case.
	glaze: Rgba,
	glaze_shine: Rgba,
	glaze_rim: Rgba,
	glaze_sprinkles: { Rgba },
	-- The doughnut's own colours: the shine on its icing, and its dough.
	icing_shine: Rgba,
	dough: Rgba,
	dough_shade: Rgba,
	dough_hole: Rgba,
	series: { Rgba },
	-- A hard shadow under whatever is raised, the glint along its top,
	-- and what is laid over it under the pointer and under a finger.
	shadow: Rgba,
	glint: Rgba,
	hover: Rgba,
	press: Rgba,
}

-- Whether `ink`, which may be translucent, reaches `target` on every one
-- of `grounds`.
local function reads(ink: Rgba, grounds: { Rgba }, target: number): boolean
	for _, ground in grounds do
		if gmnb.contrast(gmnb.over(ink, ground), ground) < target then
			return false
		end
	end
	return true
end

-- Text that steps back: `ink` at the lowest opacity, from `least` up, at
-- which it still reads on every ground.
local function recede(ink: Rgba, grounds: { Rgba }, least: number): Rgba
	local a = least
	while a < 1 and not reads(alpha(ink, a), grounds, TEXT) do
		a += 0.02
	end
	return alpha(ink, math.min(a, 1))
end

-- `color` with its lightness moved, as little as possible, until it
-- reaches `target` on every ground.
local function legible(color: Rgba, grounds: { Rgba }, target: number): Rgba
	for _, ground in grounds do
		color = gmnb.with_contrast(color, ground, target)
	end
	return color
end

local CHOCOLATE = hex("#3a2014")
local CREAM = hex("#fff1dc")

-- `spare`: how far over 4.5 the awning's stripes are taken. A colour of
-- one's own can be anything, and one taken to 4.5 exactly can come out
-- under it once rounded to eight bits on its way to the screen; the
-- built-in flavours' stripes stand as they were drawn.
local function build(key: string, raw_icing: Rgba, second: Rgba, dark: boolean, spare: number): Scheme
	local wall, card, card2, line, soft, ink
	local number, number_edge, func, func_edge
	if dark then
		-- After hours: plum tiles, the lights down.
		wall, card, card2 = hex("#2b1a22"), hex("#3b2631"), hex("#4d3542")
		line, soft, ink = hex("#140b10"), hex("#5d4250"), CREAM
		number, number_edge = hex("#553c49"), hex("#35232d")
		func, func_edge = hex("#452f3b"), hex("#2a1b24")
	else
		-- Morning: cream tiles, chocolate ink.
		wall, card, card2 = hex("#fff4df"), hex("#fffaf0"), hex("#f6e3c3")
		line, soft, ink = CHOCOLATE, hex("#e3cda4"), CHOCOLATE
		number, number_edge = hex("#fffdf8"), hex("#e6d1aa")
		func, func_edge = hex("#f8e6c5"), hex("#d9bf90")
	end

	-- The icing carries small text (a lit button's): chocolate or cream on
	-- it, whichever reads better, and the icing moved until it does.
	local icing = raw_icing
	local on_icing = if gmnb.contrast(CHOCOLATE, icing) >= gmnb.contrast(CREAM, icing) then CHOCOLATE else CREAM
	icing = gmnb.with_contrast(icing, on_icing, TEXT)
	local icing_edge = mix(icing, if dark then black else CHOCOLATE, 0.36)

	-- Everything text lies on.
	local grounds = { wall, card, card2, number, func }
	local dim = recede(ink, grounds, 0.72)
	local icing_ink = legible(mix(icing, if dark then white else CHOCOLATE, if dark then 0.2 else 0.4), grounds, TEXT)
	local danger = legible(hex(if dark then "#ff8f85" else "#b3261e"), grounds, TEXT)

	-- The operator keys: the second colour as a pastel in the morning, as
	-- a deep tint after hours.
	local operator, operator_edge, operator_ink
	if dark then
		operator = mix(second, wall, 0.66)
		operator_edge = mix(operator, black, 0.4)
		operator_ink = legible(mix(second, white, 0.45), { operator }, TEXT)
	else
		operator = mix(second, white, 0.42)
		operator_edge = mix(second, CHOCOLATE, 0.32)
		operator_ink = legible(CHOCOLATE, { operator }, TEXT)
	end

	-- The awning. In the morning its stripes are the icing and cream; after
	-- hours the icing is deepened until cream reads on it.
	local stripe, stripe_other, awning_ink
	if dark then
		awning_ink = CREAM
		stripe = legible(mix(icing, black, 0.45), { awning_ink }, TEXT * spare)
		stripe_other = card
	else
		awning_ink = CHOCOLATE
		stripe = legible(raw_icing, { awning_ink }, TEXT * spare)
		stripe_other = card
	end

	-- A glazed number is two colours, the dough's (the text colour) and
	-- the icing's: large text, so 3, with a little to spare (a colour is
	-- rounded to eight bits on its way to the screen).
	local large = UI * 1.04
	local glaze = legible(if dark then icing else mix(icing, CHOCOLATE, 0.1), { card }, UI * 1.12)
	local glaze_shine = legible(mix(glaze, white, 0.5), { card }, large)
	local glaze_rim = legible(mix(glaze, if dark then black else CHOCOLATE, 0.3), { card }, large)
	-- Sprinkles on the number: they are part of it, so each reads on the
	-- case; and each shows on the icing.
	local candidates: { Rgba } = if dark
		then { CREAM, hex("#ffe27a"), hex("#9df0d8"), hex("#bcd3ff"), hex("#ffb3d1") }
		else { CHOCOLATE, hex("#0b5d52"), hex("#213f9e"), hex("#5b2386"), hex("#8f1d1d") }
	local glaze_sprinkles = {}
	for _, c in candidates do
		c = legible(c, { card }, large)
		if gmnb.contrast(c, glaze) >= 1.3 then
			table.insert(glaze_sprinkles, c)
		end
	end
	if #glaze_sprinkles == 0 then
		table.insert(glaze_sprinkles, ink)
	end

	local series = {}
	local graph: { string } = if dark
		then { "#ff8fbf", "#74c0fc", "#8ce99a", "#ffc078", "#b197fc", "#66d9e8" }
		else { "#c2255c", "#1c7ed6", "#2f9e44", "#e8590c", "#7048e8", "#0c8599" }
	for i, code in graph do
		series[i] = legible(hex(code), { card, wall }, UI)
	end

	return {
		key = key,
		dark = dark,
		wall = wall,
		card = card,
		card2 = card2,
		line = line,
		soft = soft,
		ink = ink,
		dim = dim,
		faint = alpha(ink, 0.45),
		icing = icing,
		icing_edge = icing_edge,
		on_icing = on_icing,
		icing_ink = icing_ink,
		number = number,
		number_edge = number_edge,
		func = func,
		func_edge = func_edge,
		operator = operator,
		operator_edge = operator_edge,
		operator_ink = operator_ink,
		stripe = stripe,
		stripe_other = stripe_other,
		awning_ink = awning_ink,
		danger = danger,
		glaze = glaze,
		glaze_shine = glaze_shine,
		glaze_rim = glaze_rim,
		glaze_sprinkles = glaze_sprinkles,
		icing_shine = mix(icing, white, 0.6),
		dough = hex("#f2b866"),
		dough_shade = hex("#d48c42"),
		dough_hole = hex("#8a4b25"),
		series = series,
		shadow = alpha(black, if dark then 0.5 else 0.2),
		glint = alpha(white, if dark then 0.1 else 0.6),
		hover = if dark then alpha(white, 0.08) else alpha(CHOCOLATE, 0.07),
		press = alpha(black, if dark then 0.14 else 0.06),
	}
end

-- A scheme takes a few hundred contrast checks, and the draw hooks ask for
-- it every frame: remember what was built. (Dragging through the colour
-- chooser makes a new one each step: a few dozen at most are kept.)
local schemes: { [string]: Scheme } = {}
local scheme_count = 0

local function scheme_for(id: string, ctx: Ctx): Scheme
	local icing, second, name = flavour_colours(id, ctx)
	local key = name .. (if ctx.dark then "|dark" else "|light")
	local s = schemes[key]
	if not s then
		if scheme_count >= 32 then
			schemes, scheme_count = {}, 0
		end
		s = build(key, icing, second, ctx.dark, if FLAVOURS[id] then 1 else 1.02)
		schemes[key] = s
		scheme_count += 1
	end
	return s
end

local function scheme(ctx: Ctx): Scheme
	return scheme_for(tostring(ctx.options.flavour), ctx)
end

---------------------------------------------------------------------------
-- 2. Style
---------------------------------------------------------------------------

-- Jersey 10 is drawn on a grid of its own: one of its pixels is one real
-- pixel at this size (its capitals are then ten high), so these are the
-- sizes it is sharp at: 1, 2, 3 and 4 real pixels to each of its own.
-- One and a half is soft at the edges, and still the right size for
-- the words on a key.
local JERSEY = { "Jersey 10", "Inter Variable", "Noto Sans Math" }
local JERSEY_PIXEL = 56 / 3
-- Where its capitals' top and its baseline are, as parts of the size
-- below the top of a line (its ascent, and its capitals' height).
local JERSEY_ASCENT, JERSEY_CAPS = 1125 / 1400, 750 / 1400

local function jersey(pixels: number): Font
	return { family = JERSEY, size = JERSEY_PIXEL * pixels, weight = 400, spacing = 0 }
end

-- The header's height: the awning's stripes, and its hem of scallops.
local HEM = 16
local HEADER = 52
local AWNING = HEADER - HEM

-- The keys' corners.
local KEY_RADIUS = 6
-- Below this height a number's capitals (twenty pixels) no longer fit
-- between its key's two lines, and the key is a short key (section 2).
local SHORT_KEY = 24
-- The display case's shelf: how much of the readout's foot it takes.
local SHELF = 10

-- A see-through window is the shop window. The wall thins with the
-- window's opacity, and as it goes the glass of the shop's front comes in:
-- its frame and a shine across it (section 4, which draws them at full
-- strength, `fade = false`). The awning stays, solid at any opacity: the
-- header itself is the awning there. The counter (the display case on its
-- shelf, the keys) was always solid. All of it moves smoothly with the
-- opacity slider.

local function smooth(t: number): number
	t = math.clamp(t, 0, 1)
	return t * t * (3 - 2 * t)
end

-- How far the header has become the awning: none of it at full opacity,
-- all of it from 0.95 down. (Only there do the stripes the background
-- layer paints and the header's own show together.)
local function awning_shown(opacity: number): number
	return 1 - smooth((opacity - 0.95) / 0.05)
end

-- How far the shop's glass has come in: none of it from 0.95 up, all of it
-- from 0.6 down.
local function glazing(opacity: number): number
	return smooth((0.95 - opacity) / 0.35)
end

-- The awning as the header's own fill, at `shown`: sixteen stripes across
-- the header, starting as the drawn awning does. (A style knows no width,
-- so they are sixteenths of it; a gradient holds sixteen colours, so the
-- stripes are three gradients' runs, each clear outside its own.)
local AWNING_STRIPES = 16
local function awning_fill(s: Scheme, shown: number): Fill
	local n = AWNING_STRIPES
	local stripe, other = alpha(s.stripe, shown), alpha(s.stripe_other, shown)
	local clear = alpha(s.stripe_other, 0)
	local layers: { Paint } = {}
	for _, run in { { 0, 5 }, { 6, 10 }, { 11, n - 1 } } do
		local first, last = run[1], run[2]
		local colors: { Color }, stops: { number } = {}, {}
		local function stop(c: Rgba, at: number)
			table.insert(colors, c)
			table.insert(stops, at)
		end
		if first > 0 then
			stop(clear, 0)
			stop(clear, first / n)
		end
		for i = first, last do
			local c = if i % 2 == 0 then stripe else other
			stop(c, i / n)
			stop(c, (i + 1) / n)
		end
		if last < n - 1 then
			stop(clear, (last + 1) / n)
			stop(clear, 1)
		end
		table.insert(layers, gmnb.linear({ angle = 90, colors = colors, stops = stops }))
	end
	return layers
end

local function style(ctx: Ctx): Style
	local s = scheme(ctx)
	local ink, dim, line = s.ink, s.dim, s.line
	-- On a see-through window the wall thins and the desktop shows between
	-- the keys. Everything that carries text stands on something solid at
	-- any opacity, and the header is the awning (above).
	local awning = awning_shown(ctx.opacity)

	-- Raised things: a line around, a glint under its top, a lip along the
	-- foot and a hard shadow beneath. Pressed, the lip is gone.
	local outline: Border = { width = 2, color = line }
	local soft_line: Border = { width = 2, color = s.soft }
	local drop: Shadow = { y = 2, color = s.shadow }
	local glint: Shadow = { inset = true, y = 3, color = s.glint }
	local function raised(edge: Rgba): { Shadow }
		return { glint, { inset = true, y = -5, color = edge }, drop }
	end
	local function sunk(edge: Rgba): { Shadow }
		return { { inset = true, y = -3, color = edge } }
	end
	local hover: State = { overlay = s.hover }
	local press: State = { overlay = s.press }
	-- Something switched on is iced.
	local lit: State = { background = s.icing, color = s.on_icing, icon = { color = s.on_icing } }

	local function key(face: Rgba, edge: Rgba, color: Rgba, font: Font): Control
		return {
			background = face,
			color = color,
			font = font,
			border = outline,
			shadow = raised(edge),
			active = { shadow = sunk(edge), overlay = s.press },
		}
	end

	-- A short key (a window near its least, 300 by 400, where Scientific's
	-- keys are some fourteen pixels high and Programmer's twelve): the same
	-- block with a thin line, its lip under it instead of inside it, and
	-- its sign one Jersey pixel to the pixel, which is what it has room
	-- for. Pressed, it sinks onto its lip.
	local thin: Border = { width = 1, color = line }
	local function short_key(face: Rgba, edge: Rgba, color: Rgba): Control
		return {
			background = face,
			color = color,
			font = jersey(1),
			radius = 4,
			border = thin,
			shadow = { { y = 2, color = edge } },
			active = { shadow = {}, overlay = s.press },
		}
	end

	-- A block to press that isn't a key: the date and unit buttons, the
	-- segmented switches, the dialogs' buttons.
	local function block(font: Font): Control
		return {
			background = s.card2,
			color = ink,
			font = font,
			radius = 4,
			border = outline,
			shadow = { glint, { inset = true, y = -4, color = s.func_edge } },
			hover = hover,
			active = { shadow = { { inset = true, y = -2, color = s.func_edge } }, overlay = s.press },
			checked = { background = s.icing, color = s.on_icing, shadow = { glint, { inset = true, y = -4, color = s.icing_edge } } },
		}
	end

	-- Words that light up: the memory row, DEG, F-E, the programmer's bar.
	local function words(padding: Spacing): Control
		return {
			min_height = 30,
			padding = padding,
			radius = 4,
			font = jersey(1),
			color = ink,
			background = gmnb.transparent,
			hover = { background = alpha(ink, 0.08) },
			active = { background = alpha(ink, 0.16) },
			checked = lit,
			disabled = { color = s.faint },
		}
	end

	local flat: Control = {
		radius = 4,
		color = ink,
		hover = { background = alpha(ink, 0.08) },
		active = { background = alpha(ink, 0.16) },
		checked = lit,
	}

	return {
		colors = {
			bg = s.wall,
			surface = s.card,
			surface2 = s.card2,
			border = s.soft,
			fg = ink,
			fg_dim = dim,
			fg_faint = s.faint,
			accent = s.icing,
			on_accent = s.on_icing,
			accent_text = s.icing_ink,
			hover = alpha(ink, 0.07),
			press = alpha(ink, 0.14),
			danger = s.danger,
			series = s.series,
		},
		fonts = { display = JERSEY },
		text = {
			strong = jersey(1),
			caption = jersey(1),
			key = jersey(2),
			title = jersey(2),
		},
		shape = { radius = 6, row_radius = 4, card_radius = 6 },
		icons = { stroke = 2.2, taper = 0.5 },
		layout = {
			calculator = { margin = { 6, 8, 6, 8 }, gap = 6, panel_margin = { 6, 8, 8, 0 } },
			converter = { margin = { 8, 12, 8, 12 }, gap = 8 },
			date = { margin = { 10, 16, 10, 16 } },
			graph = { margin = { 6, 8, 6, 8 } },
		},

		-- The wall's own colour. On a see-through window the background
		-- layer paints the wall at the window's opacity itself, and this
		-- goes as the header becomes the awning (it is what the header's
		-- title lies on until then).
		window = { background = if awning > 0 then alpha(s.wall, 1 - awning) else s.wall, color = ink },
		-- Whatever holds text straight on the page (the memory row, the
		-- number bases, a converter's values, a date's fields) is a card
		-- on the wall.
		plain = { background = s.card, radius = 6 },
		header = {
			-- Clear over the awning the background layer paints; on a
			-- see-through window, the awning.
			background = if awning > 0 then awning_fill(s, awning) else gmnb.transparent,
			height = HEADER,
			-- (At one and a half the longest page's name, "Weight and mass",
			-- fits a new window's 360 pixels beside the buttons.)
			title = { font = jersey(1.5), color = s.awning_ink },
			button = {
				min_width = 32,
				min_height = 32,
				radius = 4,
				color = s.awning_ink,
				icon = { size = 18 },
				hover = { background = alpha(s.awning_ink, 0.12) },
				active = { background = alpha(s.awning_ink, 0.22) },
				checked = { background = alpha(s.awning_ink, 0.18) },
			},
			controls = {
				min_width = 28,
				min_height = 28,
				radius = 4,
				color = s.awning_ink,
				icon = { size = 16 },
				hover = { background = alpha(s.awning_ink, 0.12) },
				active = { background = alpha(s.awning_ink, 0.22) },
			},
		},
		nav = {
			background = s.card,
			shadow = { { inset = true, x = -2, color = line } },
			title = { font = jersey(2), color = ink },
			header = { font = jersey(1), color = dim },
			uppercase_headers = true,
			row = {
				radius = 4,
				font = jersey(1.5),
				color = ink,
				icon = { size = 18 },
				hover = { background = alpha(ink, 0.08) },
				active = { background = alpha(ink, 0.16) },
				checked = lit,
			},
			settings = { radius = 4, font = jersey(1.5), hover = { background = alpha(ink, 0.08) }, active = { background = alpha(ink, 0.16) } },
		},

		-- The display case. Its line, its shelf and whoever stands on it
		-- are the display layer's (section 7).
		display = {
			background = s.card,
			radius = KEY_RADIUS,
			padding_x = 14,
			heights = { standard = 122, scientific = 110, programmer = 94, compact = 90 },
			sizes = { standard = JERSEY_PIXEL * 4, scientific = JERSEY_PIXEL * 4, programmer = JERSEY_PIXEL * 3, compact = JERSEY_PIXEL * 3 },
			min_size = 20,
			value = {
				font = { family = JERSEY, weight = 400 },
				-- The display layer paints the value. Without it (a hook that
				-- failed) the number is plain dough.
				color = ink,
				error_color = s.danger,
				error_scale = 0.5,
				glow = false,
			},
			expression = { font = jersey(1), color = dim },
			expression_min_size = 12,
		},

		keys = {
			gap = 5,
			gap_compact = 3,
			radius = KEY_RADIUS,
			-- Keys shrink with the window, down to its least (300 by 400):
			-- below SHORT_KEY high they are short keys.
			icon = { size = 22, stroke = 2.8 },
			hover = hover,
			disabled = { fade = 0.3 },
			number = key(s.number, s.number_edge, ink, jersey(2)),
			func = key(s.func, s.func_edge, ink, jersey(1.5)),
			operator = key(s.operator, s.operator_edge, s.operator_ink, jersey(2)),
			equals = key(s.icing, s.icing_edge, s.on_icing, jersey(2)),
			on = { background = s.icing, color = s.on_icing, shadow = raised(s.icing_edge) },
			short = {
				below = SHORT_KEY,
				gap = 3,
				icon = { size = 14, stroke = 2 },
				number = short_key(s.number, s.number_edge, ink),
				func = short_key(s.func, s.func_edge, ink),
				operator = short_key(s.operator, s.operator_edge, s.operator_ink),
				equals = short_key(s.icing, s.icing_edge, s.on_icing),
				on = { background = s.icing, color = s.on_icing, shadow = { { y = 2, color = s.icing_edge } } },
			},
			pad = { gap = 4, radius = 4, font = jersey(1.5) },
		},

		text_button = words({ 0, 6 }),
		flyout_button = words({ 0, 8 }),
		icon_button = flat,
		button = block(jersey(1)),
		segmented = { gap = 4, item = block(jersey(1)) },
		field = {
			background = s.card,
			color = ink,
			border = outline,
			radius = 4,
			placeholder = s.faint,
			selection = alpha(s.icing, 0.4),
			caret = ink,
			focus = { border = { width = 2, color = s.icing_ink } },
			error = { border = { width = 2, color = s.danger } },
		},
		row = { radius = 4, hover = { background = alpha(ink, 0.08) }, active = { background = alpha(ink, 0.16) }, checked = { background = alpha(s.icing, 0.3) } },
		section = { font = jersey(1), color = dim },
		slider = {
			track = { height = 6, color = s.soft, radius = 0 },
			fill = s.icing_ink,
			knob = { size = 18, color = s.icing_ink, halo = alpha(s.icing, 0.3), halo_size = 26 },
		},
		switch = { off = s.soft, on = s.icing, knob = s.card, knob_on = s.on_icing },
		card = { background = s.card, radius = 6 },
		popover = { background = s.card, border = outline, radius = 6, shadow = { { y = 3, color = s.shadow } } },
		menu = { background = s.card, border = outline, radius = 6, shadow = { { y = 3, color = s.shadow } } },
		toast = { background = ink, color = s.card, radius = 6, font = jersey(1) },
		tooltip = { background = ink, color = s.card, radius = 4 },
		-- In the icing's ink (`accent_text`), as the base look's: on a
		-- see-through window that is moved 9:1 from the wall's colour,
		-- with a line of it round a key, so that the ring reads on any
		-- desktop.
		focus = { width = 2, offset = 3 },

		panel = {
			background = s.card,
			border = outline,
			radius = 6,
			shadow = { drop },
			tabs = { gap = 4, item = block(jersey(1)) },
			row = { radius = 4, hover = { background = alpha(ink, 0.08) }, active = { background = alpha(ink, 0.16) } },
			expression = { font = jersey(1), color = dim },
			result = { font = jersey(1.5), color = ink },
			delete = { radius = 4, color = dim, hover = { background = alpha(ink, 0.08) }, active = { background = alpha(ink, 0.16) } },
			memory_button = {
				background = s.card2,
				border = soft_line,
				radius = 4,
				font = jersey(1),
				color = ink,
				hover = hover,
				active = press,
			},
			empty = { color = dim },
			-- The narrow window's sheet: its switch clear of the handle the
			-- toolkit draws at its top, which would lie across the icing.
			sheet = {
				background = s.card,
				border = outline,
				radius = 6,
				shadow = { { y = -2, color = s.shadow } },
				padding = { 24, 8, 8, 8 },
			},
		},

		programmer = {
			radix = {
				row = { radius = 4, hover = { background = alpha(ink, 0.08) }, active = { background = alpha(ink, 0.16) }, checked = { background = alpha(s.icing, 0.3) } },
				name = { font = jersey(1), color = dim },
				name_checked = s.icing_ink,
				value = { font = jersey(1), color = ink },
			},
			bit = {
				radius = 4,
				font = jersey(1),
				color = dim,
				hover = { background = alpha(ink, 0.08) },
				active = { background = alpha(ink, 0.16) },
				checked = { background = s.icing, color = s.on_icing, font = { weight = 400 } },
			},
			-- (The bits' indexes have a row sixteen pixels high: the base look's own small face.)
			bit_index = { font = ctx.base.text.caption, color = dim },
		},

		date = {
			label = { font = jersey(1), color = dim },
			field = block(jersey(1)),
			result = { size = JERSEY_PIXEL * 2, font = { family = JERSEY, weight = 400 } },
			add_result = { size = JERSEY_PIXEL * 2, font = { family = JERSEY, weight = 400 } },
			subresult = { color = dim },
			calendar = {
				title = { font = jersey(1.5), color = ink },
				weekday = { font = jersey(1), color = dim },
				-- Numbers, so the pixel face, as everywhere else.
				day = {
					radius = 4,
					font = jersey(1),
					color = ink,
					hover = { background = alpha(ink, 0.08) },
					active = { background = alpha(ink, 0.16) },
					checked = lit,
				},
				other_month = s.faint,
				today = { width = 2, color = s.icing_ink },
				focus = { width = 2, color = s.icing_ink, offset = 1 },
			},
		},

		converter = {
			field = { radius = 4, padding = { 4, 6 }, checked = { background = alpha(s.icing, 0.22) } },
			marker = { color = s.icing_ink, width = 36, height = 4 },
			value = { size = JERSEY_PIXEL * 3, font = { family = JERSEY, weight = 400 }, checked_weight = 400, focus = { width = 2, color = s.icing_ink, offset = 2 } },
			symbol = { font = jersey(1.5), color = dim },
			unit = block(jersey(1)),
			swap = { radius = 4, color = ink, background = s.card2, border = outline, hover = hover, active = press },
			caption = { font = jersey(1), color = dim },
			chip = { radius = 4, background = s.card2, color = ink, border = soft_line },
			whimsy = { background = s.icing, color = s.on_icing, border = outline },
			whimsy_icon = true,
			link = { color = s.icing_ink, radius = 4, hover = { background = alpha(ink, 0.08) }, active = { background = alpha(ink, 0.16) } },
			timestamp = { color = dim },
		},

		graph = {
			-- The equations, and a function's analysis: a card of their own.
			side = { background = s.card, border = outline, radius = 6, padding = 6 },
			divider = s.soft,
			error = { color = s.danger },
			entry = { background = s.card, border = soft_line, radius = 4 },
			row_button = flat,
			add = { radius = 4, color = dim, border = soft_line, hover = { background = alpha(ink, 0.08) }, active = { background = alpha(ink, 0.16) } },
			variables = { font = jersey(1), color = ink },
			variable = { font = jersey(1.5), color = ink },
			analysis = {
				title = { font = jersey(1.5), color = ink },
				card = { border = soft_line, radius = 4, padding = { 8, 12 } },
				card_title = { font = jersey(1), color = dim },
				note = { color = dim },
			},
			toolbar = { background = s.card2, border = outline, radius = 6 },
			toolbar_button = flat,
			swatch = { size = 18 },
			canvas = {
				background = s.card,
				radius = 6,
				border = outline,
				grid_minor = alpha(ink, 0.06),
				grid_major = alpha(ink, 0.14),
				axis = { color = dim, width = 2 },
				label = { color = dim },
				trace = {
					halo = { grow = 2, color = s.card },
					ring = outline,
					bubble = { background = s.card, color = ink, radius = 4, font = jersey(1), border = outline, shadow = { { y = 2, color = s.shadow } } },
				},
			},
		},
	}
end

---------------------------------------------------------------------------
-- 3. Art pixels
---------------------------------------------------------------------------
--
-- Everything here is drawn on a grid of art pixels, each PX real pixels a
-- side, and moves a whole art pixel at a time.

local PX = 2
local NEAREST: ImageOptions = { filter = "nearest" }

local function snap(v: number): number
	return floor(v / PX + 0.5) * PX
end

-- A 32-bit xorshift: the same mixing as the app's own noise.
local function scramble(n: number): number
	n = bit32.bxor(n, bit32.lshift(n, 13))
	n = bit32.bxor(n, bit32.rshift(n, 17))
	n = bit32.bxor(n, bit32.lshift(n, 5))
	return n
end

-- A number from 0 up to 1 that is always the same for the same `n` and
-- `salt`.
local function chance(n: number, salt: number): number
	return scramble(scramble(scramble(n * 7919 + salt * 104729 + 12345))) / 4294967296
end

local function byte(v: number): number
	return floor(math.clamp(v, 0, 1) * 255 + 0.5)
end

-- A colour as `buffer.writeu32` wants it: 0xAABBGGRR. A scheme's colours
-- are the same tables from frame to frame, so each is worked out once.
local packed: { [Rgba]: number } = {}
local function pack(c: Rgba): number
	local n = packed[c]
	if not n then
		n = byte(c.a or 1) * 0x1000000 + byte(c.b) * 0x10000 + byte(c.g) * 0x100 + byte(c.r)
		packed[c] = n
	end
	return n
end

-- Events have no names, and what is drawn of one (a burst of sprinkles)
-- has to be the same from frame to frame. This gives every event of
-- `kind` a number of its own for as long as the layer keeps it: an event
-- is the one seen before at the same place, grown older.
type Seen = { x: number, y: number, age: number, seed: number }
local serial = 0

local function seeds(store: { Seen }, events: { Event }, kind: string): { [number]: number }
	local out: { [number]: number } = {}
	local kept: { Seen } = {}
	local from = 1
	for i, e in events do
		if e.kind == kind then
			local found: Seen? = nil
			for j = from, #store do
				local was = store[j]
				if was.x == e.x and was.y == e.y and was.age <= e.age then
					found = was
					from = j + 1
					break
				end
			end
			if not found then
				serial += 1
				found = { x = e.x, y = e.y, age = e.age, seed = serial }
			end
			local seen = found :: Seen
			seen.age = e.age
			table.insert(kept, seen)
			out[i] = seen.seed
		end
	end
	table.clear(store)
	table.move(kept, 1, #kept, 1, store)
	return out
end

-- One sprinkle: a rod three art pixels long, lying or standing.
local function rod(draw: Draw, x: number, y: number, standing: boolean, color: Rgba)
	if standing then
		draw:rect(snap(x) - PX / 2, snap(y) - PX * 1.5, PX, PX * 3, color)
	else
		draw:rect(snap(x) - PX * 1.5, snap(y) - PX / 2, PX * 3, PX, color)
	end
end

---------------------------------------------------------------------------
-- 4. Background: the wall, the awning, sprinkles falling
---------------------------------------------------------------------------
--
-- Bottom to top: the tiled wall (a still, repeated), the sprinkles (a GIF
-- loop that repeats in space as well as in time, drawn side by side), and
-- the awning: stripes, and a hem of scallops cut out of three masks and
-- filled with the flavour's colours. On a see-through window the wall and
-- the sprinkles thin with the window (the layer thins them itself), the
-- shop's glass comes in where the wall was (a frame, a shine), and the
-- awning stays solid until the header, which is the awning there, covers it.

local wall_light = gmnb.image("images/wall-light.png")
local wall_dark = gmnb.image("images/wall-dark.png")
local hem_odd = gmnb.image("images/scallop-odd.png")
local hem_even = gmnb.image("images/scallop-even.png")
local hem_line = gmnb.image("images/scallop-line.png")
local falling = gmnb.animation("images/sprinkles.gif")

-- A stripe is as wide as a scallop.
local STRIPE = 32

-- `mask` repeated over the rectangle, filled with `color`.
local function cut_out(draw: Draw, mask: Image, x: number, y: number, w: number, h: number, color: Rgba)
	draw:push_mask("alpha")
	draw:tile(mask, x, y, w, h, NEAREST)
	draw:pop()
	draw:rect(x, y, w, h, color)
	draw:pop()
end

local function awning(draw: Draw, w: number, s: Scheme)
	draw:rect(0, 0, w, AWNING, s.stripe_other)
	for x = 0, w, STRIPE * 2 do
		draw:rect(x, 0, STRIPE, AWNING, s.stripe)
	end
	cut_out(draw, hem_odd, 0, AWNING, w, HEM, s.stripe)
	cut_out(draw, hem_even, 0, AWNING, w, HEM, s.stripe_other)
	cut_out(draw, hem_line, 0, AWNING, w, HEM, s.line)
end

-- The shop's glass, as far as it has come in (`glass`, 0 to 1), over the
-- whole window: a frame round it in the line's colour, and two streaks of
-- shine across it, stepped as everything here is.
local SHINE_STEP = 2 * PX
local function glazed_front(draw: Draw, w: number, h: number, s: Scheme, glass: number)
	local shine = alpha(white, glass * (if s.dark then 0.22 else 0.4))
	for _, streak in { { 0.3, 6 }, { 0.42, 2 } } do
		-- A band rising to the right, `streak[2]` art pixels across.
		local from, wide = streak[1] * (w + h), streak[2] * SHINE_STEP
		for y = HEADER, h, SHINE_STEP do
			local x = snap(from - y)
			if x < w and x + wide > 0 then
				draw:rect(x, y, wide, SHINE_STEP, shine)
			end
		end
	end
	-- The frame, in the margin round the counter: its sides, its sill, and
	-- the lintel under the awning.
	local frame_line = alpha(s.line, glass)
	local thick = 3 * PX
	draw:rect(0, HEADER, thick, h - HEADER, frame_line)
	draw:rect(w - thick, HEADER, thick, h - HEADER, frame_line)
	draw:rect(0, h - thick, w, thick, frame_line)
	draw:rect(0, HEADER, w, PX, frame_line)
end

local function background(draw: Draw, frame: Frame): Again
	local s = scheme(frame.ctx)
	local w, h = frame.width, frame.height
	local opacity = frame.ctx.opacity
	-- The wall and what falls down it thin with the window (this layer is
	-- drawn at full strength: it says `fade = false`).
	local thin = opacity < 1
	if thin then
		draw:push_opacity(opacity)
	end
	draw:tile(if s.dark then wall_dark else wall_light, 0, 0, w, h, NEAREST)
	if frame.ctx.options.living == true then
		-- The loop plays on this layer's clock, and the app draws the layer
		-- again whenever its next frame is due: eight times a second. Under
		-- reduced motion its first frame stands.
		local side = falling.width * PX
		if side > 0 then
			for y = AWNING, h, side do
				for x = 0, w, side do
					draw:image(falling, x, y, side, side, NEAREST)
				end
			end
		end
	end
	if thin then
		draw:pop()
	end
	-- The shop's glass where the wall was, and the awning, solid: until the
	-- header is the awning, the awning it lies on.
	local glass = glazing(opacity)
	if glass >= 1 / 255 then
		glazed_front(draw, w, h, s, glass)
	end
	if awning_shown(opacity) < 1 then
		awning(draw, w, s)
	end
	return nil
end

---------------------------------------------------------------------------
-- 5. Fountain: sprinkles thrown over the window
---------------------------------------------------------------------------
--
-- The overlay. A result after = throws a fountain of sprinkles up from the
-- equals key; a new flavour shakes some out from under the awning. Each
-- sprinkle flies as a thrown thing does, a whole art pixel at a time, and
-- none goes as high as the readout.

local FOUNTAIN_LIFE = 1.1
local RAIN_LIFE = 1.6
local GRAVITY = 1500
local fountain_seen: { Seen } = {}
local rain_seen: { Seen } = {}

local function fountain(draw: Draw, frame: Frame): Again
	if frame.ctx.options.fountain ~= true then
		return nil
	end
	local busy = false
	local burst = seeds(fountain_seen, frame.events, "result")
	local rain = seeds(rain_seen, frame.events, "option")
	for i, e in frame.events do
		if e.kind == "result" and e.page == "calculator" and e.cause == "equals" and e.age < FOUNTAIN_LIFE then
			busy = true
			-- From the equals key pressed just before it; else from under
			-- the readout.
			local ox, oy = e.x, e.rect.y + e.rect.h
			for _, p in frame.events do
				if p.kind == "press" and p.key and p.key.kind == "equals" and p.age >= e.age and p.age - e.age < 0.3 then
					ox, oy = p.rect.x + p.rect.w / 2, p.rect.y
				end
			end
			local seed = burst[i] or 0
			-- Thrown no harder than keeps the highest of them under the
			-- readout: the number is never rained on.
			local room = math.max(oy - (e.rect.y + e.rect.h) - 12, 16)
			local throw = math.min(math.sqrt(2 * GRAVITY * room) / 640, 1)
			for k = 1, 30 do
				local life = 0.7 + 0.4 * chance(k, seed + 2)
				if e.age < life then
					-- A fan, straight up and to both sides.
					local angle = math.rad(-155 + 130 * chance(k, seed))
					local speed = (280 + 360 * chance(k, seed + 1)) * throw
					local x = ox + math.cos(angle) * speed * e.age
					local y = oy + math.sin(angle) * speed * e.age + GRAVITY / 2 * e.age * e.age
					rod(draw, x, y, floor(e.age * 10 + k) % 2 == 0, SPRINKLES[k % #SPRINKLES + 1])
				end
			end
		elseif e.kind == "option" and e.id == "flavour" and e.age < RAIN_LIFE then
			busy = true
			local seed = rain[i] or 0
			for k = 1, 36 do
				local wait = 0.5 * chance(k, seed + 1)
				local t = e.age - wait
				if t > 0 then
					local x = frame.width * chance(k, seed)
					local y = HEADER + 240 * t + 300 * t * t
					if y < frame.height then
						rod(draw, x, y, floor(t * 8 + k) % 2 == 0, SPRINKLES[k % #SPRINKLES + 1])
					end
				end
			end
		end
	end
	return if busy then 1 / 30 else nil
end

---------------------------------------------------------------------------
-- 6. Keypad: keys that sink, hop and stack; crumbs
---------------------------------------------------------------------------

-- A key falls into place in this long, and each row starts this much
-- after the one under it.
local DROP = 0.14
local ROW_STEP = 0.05
-- How long the crumbs a press knocks loose fly for.
local CRUMB_LIFE = 0.32
-- How fast the hop a result sends through the keys travels, in pixels a
-- second.
local WAVE = 1100

-- Where a key is, apart from its place: across, down, how much of it shows,
-- and whether it is still on its way.
local function moved(key: KeyCell, frame: Frame): (number, number, number, boolean)
	local dx, dy, opacity = 0, 0, 1
	local busy = false
	if not frame.motion then
		return dx, dy, opacity, busy
	end
	local cx, cy = key.rect.x + key.rect.w / 2, key.rect.y + key.rect.h / 2
	-- The equals key's middle, as far as the events tell: where a
	-- result's hop starts from.
	local ex, ey = frame.width, frame.height
	local pressed: number? = nil
	local typed = false
	for _, e in frame.events do
		if e.kind == "press" and e.key then
			if e.key.kind == "equals" then
				ex, ey = e.rect.x + e.rect.w / 2, e.rect.y + e.rect.h / 2
			end
			if key.id ~= nil and e.key.id == key.id then
				pressed, typed = e.age, e.via == "keyboard"
			end
		end
	end
	for _, e in frame.events do
		if e.kind == "show" then
			-- The keypad stacks itself: the bottom row lands first.
			local rows = 0
			local keys: { KeyCell } = frame.keys or {}
			for _, k in keys do
				rows = math.max(rows, k.row)
			end
			local t = e.age - (rows - key.row) * ROW_STEP - key.col * 0.012
			if t < 0 then
				return 0, 0, 0, true
			elseif t < DROP then
				local p = t / DROP
				dy = -snap((1 - p * p) * 44)
				busy = true
			elseif t < DROP + 0.05 then
				-- It lands, and the ones under it give a little.
				dy = PX
				busy = true
			end
		elseif e.kind == "result" and e.page == "calculator" and e.cause == "equals" then
			-- A hop runs out through the keys from the equals key.
			local away = math.sqrt((cx - ex) ^ 2 + (cy - ey) ^ 2)
			local t = e.age - away / WAVE
			if t >= 0 and t < 0.06 then
				dy -= PX * 2
			elseif t >= 0.06 and t < 0.12 then
				dy -= PX
			end
			busy = busy or t < 0.12
		elseif e.kind == "error" and e.age < 0.3 then
			-- Something is wrong: the whole pad shakes its head.
			dx += if floor(e.age / 0.05) % 2 == 0 then -PX else PX
			busy = true
		end
	end
	if key.pressed then
		dy += PX
	elseif pressed then
		-- Let go, it hops back up. (A key typed on the keyboard is held
		-- down for 110 ms first.)
		local t = pressed - (if typed then 0.11 else 0)
		if t >= 0 and t < 0.05 then
			dy -= PX * 2
		elseif t >= 0.05 and t < 0.1 then
			dy -= PX
		end
		busy = busy or t < 0.1
	end
	return dx, dy, opacity, busy
end

local function place(key: KeyCell, frame: Frame): Placement?
	local dx, dy, opacity = moved(key, frame)
	if dx == 0 and dy == 0 and opacity == 1 then
		return nil
	end
	return { dx = dx, dy = dy, opacity = opacity }
end

local crumbs_seen: { Seen } = {}

local function keypad(draw: Draw, frame: Frame): Again
	local s = scheme(frame.ctx)
	local busy = false
	local keys: { KeyCell } = frame.keys or {}
	for _, k in keys do
		local dx, dy, opacity, moving = moved(k, frame)
		busy = busy or moving
		if k.kind == "equals" and opacity > 0 and k.rect.w >= 44 and k.rect.h >= 32 then
			-- The equals key is iced: a shine, and three sprinkles in its
			-- corners, clear of its sign.
			local x, y, w, h = k.rect.x + dx, k.rect.y + dy, k.rect.w, k.rect.h
			draw:rect(x + 6, y + 6, PX * 3, PX, alpha(white, 0.55))
			draw:rect(x + 6, y + 6 + PX, PX, PX, alpha(white, 0.55))
			rod(draw, x + w - 13, y + 11, false, s.on_icing)
			rod(draw, x + 13, y + h - 15, true, alpha(white, 0.8))
			rod(draw, x + w - 13, y + h - 15, true, s.icing_edge)
		end
	end
	-- Crumbs: a press knocks a few loose.
	local crumb = seeds(crumbs_seen, frame.events, "press")
	for i, e in frame.events do
		if e.kind == "press" and e.source == "key" and e.page ~= "graphing" and e.age < CRUMB_LIFE then
			busy = true
			local seed = crumb[i] or 0
			for n = 1, 5 do
				local angle = math.rad(-160 + 140 * chance(n, seed))
				local speed = 90 + 110 * chance(n, seed + 1)
				local x = e.x + math.cos(angle) * speed * e.age
				local y = e.y + math.sin(angle) * speed * e.age + 500 * e.age * e.age
				draw:rect(snap(x), snap(y), PX, PX, SPRINKLES[(n + seed) % #SPRINKLES + 1])
			end
		end
	end
	return if busy then 1 / 30 else nil
end

---------------------------------------------------------------------------
-- 7. Display: the case, the doughnut, digits plain and glazed
---------------------------------------------------------------------------
--
-- The app lays the value out and hands over its glyphs. This paints them:
-- plain dough while a number is being typed, and glazed when it is a
-- result. The icing is a picture worked out here, pixel by pixel, and
-- shown through the digits' own shapes; so is the doughnut on the shelf.
-- The converter's values and the date page's results come through here
-- too, as plain digits.

-- The icing: one picture, as wide as two hundred and fifty-six art pixels
-- and as tall as the tallest digits, drawn over the number as often as
-- it takes to cover it.
local ICING_W, ICING_H = 256, 28
local icing_picture = gmnb.pixels(ICING_W, ICING_H)
local icing_buffer = buffer.create(ICING_W * ICING_H * 4)
local icing_made = ""
-- How long the icing takes to run down, and how long after it the last
-- sprinkle lands.
local POUR = 0.4
local SPRINKLE_TIME = 0.45
-- How far down a digit the icing comes, as a part of its height.
local ICED = 0.44
-- The icing is a second colour of the number's, and reads on the case as
-- large text has to (3, where small text needs 4.5). So only large digits
-- wear it: from this size up, where Jersey's capitals are fifteen pixels
-- high and its strokes three thick. A number shrunk further to fit its
-- readout (some twenty digits in the narrowest window) stays plain.
local ICED_FROM = 28

-- How far below the icing's line the icing hangs at column `x`: drips.
local function drip(x: number): number
	local p = x % 11
	if p == 2 or p == 3 then
		return 2
	elseif p == 1 or p == 4 or p == 8 then
		return 1
	end
	return 0
end

-- Fills the icing's picture: for digits `tall` art pixels high, `poured`
-- of the way down (0 to 1), with the first `sprinkled` of its sprinkles.
local function make_icing(s: Scheme, key: string, tall: number, poured: number, sprinkled: number)
	local made = string.format("%s|%d|%.2f|%.2f", key, tall, poured, sprinkled)
	if made == icing_made then
		return
	end
	local buf = icing_buffer
	buffer.fill(buf, 0, 0)
	local glaze, shine, rim = pack(s.glaze), pack(s.glaze_shine), pack(s.glaze_rim)
	-- The picture is laid one art pixel above the digits' tops, so that
	-- row 1 is their top row. The line the icing comes down to, and at
	-- every column how far it has come.
	local line = math.max(floor(tall * ICED), 2)
	local edge = table.create(ICING_W, 0)
	for x = 0, ICING_W - 1 do
		-- Some columns run ahead of the others.
		local start = 0.25 * chance(x // 3, 7)
		local p = math.clamp((poured - start) / (1 - 0.25), 0, 1)
		local reach = floor((line + drip(x)) * (1 - (1 - p) * (1 - p)) + 0.5)
		edge[x + 1] = reach
		for y = 0, math.min(reach, ICING_H) - 1 do
			local color = glaze
			if y == reach - 1 then
				color = rim
			elseif y == 1 and x % 9 < 5 then
				color = shine
			end
			buffer.writeu32(buf, (y * ICING_W + x) * 4, color)
		end
	end
	-- Sprinkles: little rods, two art pixels long, on the icing only.
	local count = floor(ICING_W * 0.7 * sprinkled)
	local colours = {}
	for i, c in s.glaze_sprinkles do
		colours[i] = pack(c)
	end
	for n = 1, count do
		local x = floor(chance(n, 21) * (ICING_W - 2))
		local reach = math.min(edge[x + 1], edge[x + 2])
		if reach >= 5 then
			local y = 2 + floor(chance(n, 22) * (reach - 4))
			-- Every other one is the first colour, the one that shows best.
			local color = if n % 2 == 0 then colours[1] else colours[(n // 2) % #colours + 1]
			buffer.writeu32(buf, (y * ICING_W + x) * 4, color)
			if chance(n, 23) < 0.5 then
				buffer.writeu32(buf, (y * ICING_W + x + 1) * 4, color)
			elseif y + 1 < reach - 1 then
				buffer.writeu32(buf, ((y + 1) * ICING_W + x) * 4, color)
			else
				buffer.writeu32(buf, ((y - 1) * ICING_W + x) * 4, color)
			end
		end
	end
	-- (A picture may change thirty times a second. Refused, this one is
	-- made again with the frame the app then owes the layer.)
	icing_made = if icing_picture:set(buf) then made else ""
end

-- The doughnut: twenty-four art pixels a side, worked out pixel by pixel
-- from a few numbers, so that it can squash, stretch and change its face.
local BUDDY = 24
local buddy_picture = gmnb.pixels(BUDDY, BUDDY)
local buddy_buffer = buffer.create(BUDDY * BUDDY * 4)
local buddy_made = ""

-- Where the icing ends on the doughnut at each column, in art pixels
-- below its middle.
local BUDDY_ICING = { 0, 0, 1, 2, 2, 1, 0, 0, 1, 1, 0, 0, 1, 2, 3, 2, 1, 0, 0, 1, 2, 1, 0, 0 }

type Face = "open" | "shut" | "happy" | "dizzy"

-- The colour of the doughnut's pixel at (x, y), or nil where there is none.
-- `squash` flattens it (1) or stretches it (-1); `look` is the side its
-- eyes' glints are on: where it is looking.
local function buddy_pixel(s: Scheme, x: number, y: number, face: Face, squash: number, look: number): Rgba?
	local line = if s.dark then s.line else CHOCOLATE
	local rx, ry = 10 + squash, 10 - squash
	local cx, cy = 11.5, 21.5 - ry
	-- Its feet, and the legs they are on.
	if (y >= 22 and ((x >= 6 and x <= 8) or (x >= 15 and x <= 17))) or (y == 21 and (x == 7 or x == 16)) then
		return line
	end
	local dx, dy = (x - cx) / rx, (y - cy) / ry
	local d = dx * dx + dy * dy
	if d > 1 then
		return nil
	end
	local ix, iy = (x - cx) / (rx - 1.1), (y - cy) / (ry - 1.1)
	local inner = ix * ix + iy * iy
	if inner > 1 then
		return line
	end
	-- The eyes, above the middle, either side of the hole.
	local ey = floor(cy) - 3
	for side = -1, 1, 2 do
		local ex = if side < 0 then 5 else 17
		local px, py = x - ex, y - ey
		if face == "open" then
			if px >= 0 and px <= 1 and py >= 0 and py <= 2 then
				return if py == 0 and px == (if look > 0 then 1 else 0) then white else line
			end
		elseif face == "shut" then
			if px >= -1 and px <= 2 and py == 2 then
				return line
			end
		elseif face == "happy" then
			-- An arch.
			if (py == 0 and px >= 0 and px <= 1) or (py == 1 and (px == -1 or px == 2)) then
				return line
			end
		elseif px >= -1 and px <= 1 and py >= 0 and py <= 2 and math.abs(px) == math.abs(py - 1) then
			-- A cross.
			return line
		end
	end
	-- The hole: its mouth.
	local hx, hy = (x - cx) / 3, (y - cy - 2) / (3 * ry / rx)
	local hole = hx * hx + hy * hy
	if hole <= 0.4 then
		return s.dough_hole
	elseif hole <= 1 then
		return line
	end
	-- Icing above, dough below.
	if y - cy < 1.5 + BUDDY_ICING[x + 1] then
		if y - cy >= 0.5 + BUDDY_ICING[x + 1] then
			return s.icing_edge
		elseif dx < -0.15 and dy < -0.3 and inner > 0.42 and inner < 0.68 then
			return s.icing_shine
		end
		-- Its own sprinkles.
		local n = (x * 7 + y * 13) % 31
		if n == 0 and dy < 0 then
			return SPRINKLES[(x + y) % #SPRINKLES + 1]
		end
		return s.icing
	end
	if dy > 0.35 and inner > 0.55 then
		return s.dough_shade
	end
	return s.dough
end

local function make_buddy(s: Scheme, key: string, face: Face, squash: number, look: number)
	local made = string.format("%s|%s|%d|%d", key, face, squash, look)
	if made == buddy_made then
		return
	end
	for y = 0, BUDDY - 1 do
		for x = 0, BUDDY - 1 do
			local c = buddy_pixel(s, x, y, face, squash, look)
			buffer.writeu32(buddy_buffer, (y * BUDDY + x) * 4, if c then pack(c) else 0)
		end
	end
	buddy_made = if buddy_picture:set(buddy_buffer) then made else ""
end

-- The calculator's number, as it stands, is a result: it is glazed.
local glazed = false

local function display(draw: Draw, frame: Frame): Again
	local v = frame.value
	if not v then
		return nil
	end
	local s = scheme(frame.ctx)
	local w, h = frame.width, frame.height
	-- (What its pictures are kept by: the flavour, its colours and the mode.)
	local key = s.key
	local change = v.change
	local kind = if change then change.kind else "none"
	local age = if change then change.age else 0
	local again: number? = nil
	local function sooner(seconds: number)
		again = if again then math.min(again, seconds) else seconds
	end

	-- A page's line (a converter's value, a date's result): plain digits
	-- that drop into place.
	if v.role ~= "calculator" then
		local drop_y = 0
		if kind ~= "none" and kind ~= "error" and age < 0.09 then
			drop_y = -PX * (if age < 0.03 then 3 else if age < 0.06 then 2 else 1)
			sooner(1 / 30)
		end
		for _, g in v.glyphs do
			if drop_y ~= 0 and change and g.index >= change.from then
				draw:save()
				draw:translate(0, drop_y)
				draw:glyph(g, if v.error then s.danger else s.ink)
				draw:restore()
			else
				draw:glyph(g, if v.error then s.danger else s.ink)
			end
		end
		return again
	end

	-- The case: its line, and its shelf.
	local shelf = h - SHELF
	draw:push_clip(0, 0, w, h, KEY_RADIUS)
	draw:rect(0, shelf, w, SHELF, s.card2)
	draw:rect(0, shelf, w, PX, s.line)
	draw:rect(0, shelf + PX, w, PX, s.glint)
	draw:pop()
	draw:border(0, 0, w, h, KEY_RADIUS, 2, s.line)

	-- What the events say: a result to cheer, an error, a key pressed; and
	-- of the newest result, whether = or a function key brought it about.
	local cheer: number? = nil
	local oops: number? = nil
	local tap: number? = nil
	local earned: boolean? = nil
	for _, e in frame.events do
		if e.page == "calculator" then
			if e.kind == "result" then
				earned = e.cause == "equals" or e.cause == "function"
				if earned then
					cheer, oops = e.age, nil
				end
			elseif e.kind == "error" then
				oops, cheer = e.age, nil
			elseif e.kind == "press" then
				tap = e.age
			end
		end
	end

	-- Whether what stands is a result to glaze: one that = or a function
	-- key gave, not the running total an operator shows on the way there,
	-- nor a number typed, pasted or recalled. Without motion nothing says
	-- which it is, and every number is shown at its best.
	if not frame.motion then
		glazed = true
	elseif kind == "result" then
		if earned ~= nil then
			glazed = earned
		end
	elseif kind ~= "none" then
		glazed = false
	end

	-- The doughnut on the shelf, left of the number. It ducks behind the
	-- shelf as the number grows towards it, and is gone when there is no
	-- room.
	if frame.ctx.options.buddy == true and h >= 84 then
		local size = BUDDY * PX
		local left = 12
		local room = v.rect.x - 10 - (left + size)
		local duck = if #v.glyphs == 0 then 0 else snap(math.clamp(-room, 0, size))
		if duck < size - 8 then
			local face: Face = "open"
			local squash, hop, sway = 0, 0, 0
			if oops and oops < 1.1 then
				face = "dizzy"
				if oops < 0.5 then
					sway = if floor(oops / 0.08) % 2 == 0 then -PX else PX
				end
				sooner(if oops < 0.5 then 1 / 30 else 1.1 - oops + 0.02)
			elseif cheer and cheer < 1.0 then
				-- A jump: it gathers itself, flies, lands flat, and beams.
				face = "happy"
				if cheer < 0.07 then
					squash = 1
				elseif cheer < 0.47 then
					local t = (cheer - 0.07) / 0.4
					hop = -snap(26 * 4 * t * (1 - t))
					squash = if t < 0.5 then -1 else 0
				elseif cheer < 0.55 then
					squash = 1
				end
				sooner(if cheer < 0.55 then 1 / 30 else 1.0 - cheer + 0.02)
			elseif tap and tap < 0.12 then
				-- It nods along with every key.
				hop = if tap < 0.06 then -PX * 2 else -PX
				sooner(1 / 30)
			elseif frame.motion and frame.ctx.options.living == true then
				-- It blinks now and then.
				local t = frame.time % 4.2
				if t < 0.14 then
					face = "shut"
					sooner(0.14 - t + 0.01)
				else
					sooner(4.2 - t + 0.01)
				end
			end
			make_buddy(s, key, face, squash, 1)
			draw:push_clip(0, 0, w, shelf)
			draw:image(buddy_picture, left + sway, shelf - size + PX + hop + duck, size, size, NEAREST)
			draw:pop()
		end
	end

	-- The number. An error shakes its head, in the error's colour.
	local shake = 0
	if kind == "error" and age < 0.36 then
		shake = (if floor(age / 0.045) % 2 == 0 then -1 else 1) * PX * (if age < 0.18 then 2 else 1)
		sooner(1 / 30)
	end
	-- New digits drop into place; a result lands all at once.
	local function fall(g: Glyph): number
		if kind == "typing" and change and g.index >= change.from and age < 0.09 then
			return -PX * (if age < 0.03 then 3 else if age < 0.06 then 2 else 1)
		elseif kind == "result" and age < 0.09 then
			return -PX * (if age < 0.03 then 3 else if age < 0.06 then 2 else 1)
		end
		return 0
	end
	if (kind == "typing" or kind == "result") and age < 0.09 then
		sooner(1 / 30)
	end
	local function glyphs(paint: Paint)
		for _, g in v.glyphs do
			local dy = fall(g)
			if dy ~= 0 or shake ~= 0 then
				draw:save()
				draw:translate(shake, dy)
				draw:glyph(g, paint)
				draw:restore()
			else
				draw:glyph(g, paint)
			end
		end
	end
	glyphs(if v.error then s.danger else s.ink)

	-- The icing, through the digits' own shapes.
	if glazed and not v.error and #v.glyphs > 0 and v.size >= ICED_FROM then
		local poured, sprinkled = 1, 1
		if kind == "result" then
			poured = math.min(age / POUR, 1)
			sprinkled = math.clamp((age - POUR * 0.6) / SPRINKLE_TIME, 0, 1)
			if age < POUR + SPRINKLE_TIME then
				sooner(1 / 30)
			end
		end
		-- The digits' top, from the typeface's own measures.
		local top = v.rect.y + (JERSEY_ASCENT - JERSEY_CAPS) * v.size
		local tall = ceil(JERSEY_CAPS * v.size / PX)
		-- (In steps, so that the picture is made a dozen times, not thirty.)
		make_icing(s, key, tall, floor(poured * 12 + 0.5) / 12, floor(sprinkled * 8 + 0.5) / 8)
		local y = snap(top) - PX
		draw:push_mask("alpha")
		glyphs(white)
		draw:pop()
		local x = snap(v.rect.x) - PX * 2
		while x < v.rect.x + v.rect.w do
			draw:image(icing_picture, x, y, ICING_W * PX, ICING_H * PX, NEAREST)
			x += ICING_W * PX
		end
		draw:pop()
	end

	return again
end

---------------------------------------------------------------------------
-- 8. Options
---------------------------------------------------------------------------

-- A flavour's tile in Settings: the wall, the awning, and the doughnut in
-- that flavour's icing, drawn square by square.
local function tile(draw: Draw, frame: Preview, choice: string)
	local s = scheme_for(choice, frame.ctx)
	local w, h = frame.width, frame.height
	draw:push_clip(0, 0, w, h, 6)
	draw:rect(0, 0, w, h, s.wall)
	for x = 0, w, STRIPE do
		draw:rect(x, 0, STRIPE / 2, 8, s.stripe)
		draw:rect(x + STRIPE / 2, 0, STRIPE / 2, 8, s.stripe_other)
	end
	draw:rect(0, 8, w, PX, s.line)
	local left, top = floor((w - BUDDY * PX) / 2), 10
	for y = 0, BUDDY - 1 do
		-- Runs of one colour as one rectangle.
		local from, color: Rgba? = 0, nil
		for x = 0, BUDDY do
			local c = if x < BUDDY then buddy_pixel(s, x, y, "open", 0, 0) else nil
			if c ~= color then
				if color then
					draw:rect(left + from * PX, top + y * PX, (x - from) * PX, PX, color)
				end
				from, color = x, c
			end
		end
	end
	draw:pop()
end

local function is(flavour: string)
	return function(options: { [string]: OptionValue }, _ctx: Ctx): boolean
		return options.flavour == flavour
	end
end

local options: { Option } = {
	{
		id = "flavour",
		type = "choice",
		group = "Flavour",
		label = "Flavour",
		description = "The icing on the keys, the awning and your results",
		default = "strawberry",
		choices = {
			{ id = "strawberry", label = "Strawberry" },
			{ id = "blueberry", label = "Blueberry" },
			{ id = "matcha", label = "Matcha" },
			{ id = "lemon", label = "Lemon" },
			{ id = "caramel", label = "Caramel" },
			{ id = "system", label = "System" },
			{ id = "custom", label = "Freestyle" },
		},
		preview = tile,
	},
	{
		id = "system_note",
		type = "note",
		group = "Flavour",
		label = "Iced in your desktop's accent colour (the freedesktop portal setting your desktop or shell publishes), with a colour that goes with it, and follows it live.",
		visible = is("system"),
	},
	{
		id = "icing",
		type = "color",
		group = "Flavour",
		label = "Icing",
		description = "The equals key, what is switched on, the awning and your results",
		default = DEFAULT_ICING,
		visible = is("custom"),
	},
	{
		id = "second",
		type = "color",
		group = "Flavour",
		label = "Second colour",
		description = "The operator keys",
		default = DEFAULT_SECOND,
		visible = is("custom"),
		actions = {
			{
				label = "Go with the icing",
				tooltip = "A colour most of the way round the wheel from the icing, as mint goes with strawberry",
				run = function(o: { [string]: OptionValue }, _ctx: Ctx): { [string]: OptionValue }
					return { second = gmnb.to_hex(second_for(iced(colour(o.icing, DEFAULT_ICING)))) }
				end,
			},
		},
	},
	{
		id = "buddy",
		type = "toggle",
		group = "Shop",
		label = "Doughnut on the shelf",
		description = "It watches the number, nods along and jumps for a result",
		default = true,
	},
	{
		id = "living",
		type = "toggle",
		group = "Shop",
		label = "Falling sprinkles",
		description = "Sprinkles drift down the wall, and the doughnut blinks. Pauses whenever the window is in the background",
		default = true,
	},
	{
		id = "fountain",
		type = "toggle",
		group = "Shop",
		label = "Sprinkle fountain",
		description = "The = key throws sprinkles over the keys with every result",
		default = true,
	},
	{
		id = "motion_note",
		type = "note",
		group = "Shop",
		label = "With the system's “reduce animations” setting nothing moves, and every number is shown glazed.",
	},
}

---------------------------------------------------------------------------
-- 9. Sound
---------------------------------------------------------------------------
--
-- A shop like this one sounds like the machine in its corner: chip-tune.
-- Every sound is a square wave or two (a triangle for the low ones, a
-- crunch of held noise for what is swept away), switched on and off
-- rather than faded, and nothing rings for long. They follow the sound
-- chapter's rules (docs/theming-media.md, "Designing sounds") and were
-- balanced by measuring, not by ear: tools/theming/sound_check.py.
--
-- What makes it part of the theme is the notes. Every flavour has a
-- five-note scale. Number keys play its notes, each key its own, so that
-- any sum typed is a tune in the flavour's key and no key is a wrong
-- note; the operators play a low note that goes with them; and a result
-- after = is the flavour's jingle, three notes rising and a patter, timed
-- to what the readout does: the icing pours on the first, the doughnut
-- is in the air on the third, and the sprinkles land with the patter.

-- A square wave at `pitch`: on for `body` seconds, then off within `tail`.
-- `width` is the part of each period it spends high: a half is hollow, a
-- quarter bright, an eighth thin.
local function pulse(pitch: Pitch | PitchGlide, width: number, level: number, at: number, body: number, tail: number): SynthLayer
	return {
		wave = "square",
		width = width,
		freq = pitch,
		gain = level,
		at = at,
		env = { attack = 0.001, decay = 0.006, sustain = 0.6, hold = body, release = tail },
	}
end

-- A crunch: noise held `pitch` times a second.
local function crunch(seed: number, pitch: number | Glide, level: number, at: number, seconds: number): SynthLayer
	return {
		wave = "noise",
		seed = seed,
		freq = pitch,
		gain = level,
		at = at,
		env = { attack = 0, decay = seconds },
	}
end

-- Nothing under the small speakers' reach, twice over (a lopsided square
-- wave switched on and off is a thump as well as a note), and the top
-- taken off the edges.
local chip: { SynthEffect } = {
	{ type = "highpass", freq = 220 },
	{ type = "highpass", freq = 220 },
	{ type = "lowpass", freq = 7000 },
}

-- Number keys: one note, on C5, in two takes played in turn. The key's
-- own note is this one played faster or slower.
local blip_a = gmnb.synth({
	gain = 0.23,
	layers = { pulse("C5", 0.5, 1, 0, 0.012, 0.024), pulse("C6", 0.25, 0.22, 0, 0.003, 0.012) },
	effects = chip,
})
local blip_b = gmnb.synth({
	gain = 0.23,
	layers = { pulse("C5", 0.25, 1, 0, 0.012, 0.024), pulse("C6", 0.5, 0.18, 0, 0.003, 0.012) },
	effects = chip,
})

-- Operators: a triangle on G4 that dips as it lands, rounder and lower
-- than any number key.
local bop = gmnb.synth({
	gain = 0.16,
	layers = {
		{
			wave = "triangle",
			freq = { "A4", "G4", time = 0.018 } :: PitchGlide,
			env = { attack = 0.001, decay = 0.008, sustain = 0.7, hold = 0.016, release = 0.026 },
		},
		pulse("G5", 0.125, 0.12, 0, 0.002, 0.008),
	},
	effects = chip,
})

-- Function keys and buttons: thinner and drier, a tick more than a note.
local tap = gmnb.synth({
	gain = 0.24,
	layers = { pulse("E5", 0.125, 1, 0, 0.008, 0.02), crunch(41, 6000, 0.35, 0, 0.008) },
	effects = chip,
})

-- The equals key: two notes, a fifth apart, short and dry. The jingle
-- that follows is the result's.
local set = gmnb.synth({
	gain = 0.17,
	layers = { pulse("C5", 0.5, 1, 0, 0.016, 0.014), pulse("G5", 0.5, 1, 0.045, 0.022, 0.03) },
	effects = chip,
})

-- 2nd and hyp: a switch thrown, two quick notes.
local flip = gmnb.synth({
	gain = 0.18,
	layers = { pulse("E5", 0.25, 1, 0, 0.008, 0.012), pulse("A5", 0.25, 1, 0.034, 0.012, 0.022) },
	effects = chip,
})

-- Backspace: a note that falls away.
local back = gmnb.synth({
	gain = 0.18,
	layers = { pulse({ "A5", "D5", time = 0.035 } :: PitchGlide, 0.25, 1, 0, 0.012, 0.022) },
	effects = chip,
})

-- Clear: the counter swept, a crunch that falls.
local sweep = gmnb.synth({
	gain = 0.2,
	layers = {
		crunch(42, { 9000, 700, time = 0.12 } :: Glide, 1, 0, 0.14),
		pulse({ "C5", "C4", time = 0.09 } :: PitchGlide, 0.5, 0.3, 0, 0.03, 0.05),
	},
	effects = chip,
})

-- The memory row: something dropped in the jar. A tick, and two notes a
-- fifth apart, the second left to ring a little.
local jar = gmnb.synth({
	gain = 0.18,
	layers = {
		crunch(43, 8000, 0.4, 0, 0.006),
		pulse("G5", 0.25, 1, 0, 0.014, 0.012),
		pulse("D6", 0.25, 0.9, 0.04, 0.02, 0.07),
	},
	effects = chip,
})

-- The jingle's voice: a square on E5 with a triangle an octave under it,
-- and one echo. The jingle is this one note at other speeds.
local voice_fx: { SynthEffect } = {
	{ type = "highpass", freq = 170 },
	{ type = "delay", time = 0.12, feedback = 0.25, mix = 0.2 },
	{ type = "lowpass", freq = 6500 },
}
local note = gmnb.synth({
	gain = 0.23,
	length = 0.7,
	layers = {
		{ wave = "square", width = 0.5, freq = "E5", env = { attack = 0.002, decay = 0.2 } },
		{ wave = "square", width = 0.25, freq = "E6", env = { attack = 0.002, decay = 0.07 }, gain = 0.25 },
		{ wave = "triangle", freq = "E4", env = { attack = 0.002, decay = 0.24 }, gain = 0.6 },
	},
	effects = voice_fx,
})

-- Sprinkles landing: a patter of high ticks, each its own note.
local patter = gmnb.synth({
	gain = 0.12,
	layers = {
		pulse("E6", 0.25, 1, 0, 0.004, 0.014),
		pulse("G6", 0.25, 0.9, 0.045, 0.004, 0.014),
		pulse("C7", 0.25, 0.8, 0.085, 0.004, 0.014),
		pulse("G6", 0.25, 0.7, 0.14, 0.004, 0.014),
		pulse("E7", 0.25, 0.7, 0.18, 0.004, 0.03),
		crunch(44, 9000, 0.25, 0, 0.004),
		crunch(45, 9000, 0.2, 0.085, 0.004),
		crunch(46, 9000, 0.2, 0.18, 0.004),
	},
	effects = chip,
})

-- An error: the tray dropped. Two notes falling, the second three whole
-- tones under the first and wavering, with the grit of a cheap chip.
local womp_fx: { SynthEffect } = {
	{ type = "crush", bits = 6, rate = 11000 },
	{ type = "highpass", freq = 170 },
	{ type = "lowpass", freq = 5000 },
}
local womp = gmnb.synth({
	gain = 0.15,
	layers = {
		pulse({ "B4", "A4", time = 0.1 } :: PitchGlide, 0.5, 1, 0, 0.08, 0.03),
		{
			wave = "square",
			width = 0.5,
			freq = { "F4", "E4", time = 0.25 } :: PitchGlide,
			vibrato = { rate = 13, depth = 35 },
			at = 0.137,
			env = { attack = 0.002, decay = 0.01, sustain = 0.7, hold = 0.12, release = 0.14 },
		},
	},
	effects = womp_fx,
})

-- A change of page or mode: the bell over the shop's door, two notes
-- coming down, a triangle with a bright edge that is gone at once.
local door_fx: { SynthEffect } = {
	{ type = "highpass", freq = 200 },
	{ type = "delay", time = 0.09, feedback = 0.2, mix = 0.15 },
}
local function ding(pitch: string, above: string, at: number, level: number): (SynthLayer, SynthLayer)
	return { wave = "triangle", freq = pitch, at = at, gain = level, env = { attack = 0.002, decay = 0.32 } },
		{ wave = "square", width = 0.25, freq = above, at = at, gain = level * 0.14, env = { attack = 0.001, decay = 0.05 } }
end
local ding_1, edge_1 = ding("E6", "E7", 0, 1)
local ding_2, edge_2 = ding("C6", "C7", 0.13, 0.85)
local door = gmnb.synth({
	gain = 0.16,
	length = 0.8,
	layers = { ding_1, edge_1, ding_2, edge_2 },
	effects = door_fx,
})

-- A flavour's notes, as speeds: its five for the number keys (of the
-- blips, which are on C5), its jingle's four (of the voice, on E5), the
-- note its operators play (of the bop, on G4), and its home note for the
-- equals key and the patter (which are in C).
type Voice = { keys: { number }, jingle: { number }, low: number, home: number }

local function speeds(notes: { string }, of: string): { number }
	local out = {}
	for i, name in notes do
		out[i] = gmnb.note(name) / gmnb.note(of)
	end
	return out
end

local function voice(keys: { string }, jingle: { string }, low: string, home: string): Voice
	return {
		keys = speeds(keys, "C5"),
		jingle = speeds(jingle, "E5"),
		low = gmnb.note(low) / gmnb.note("G4"),
		home = gmnb.note(home) / gmnb.note("C5"),
	}
end

local VOICES: { [string]: Voice } = {
	-- C major's five: bright and plain, the tune a toy plays.
	strawberry = voice({ "C5", "D5", "E5", "G5", "A5" }, { "C5", "E5", "G5", "C6" }, "G4", "C5"),
	-- A minor's five: the same notes from the dark end.
	blueberry = voice({ "A4", "C5", "D5", "E5", "G5" }, { "A4", "C5", "E5", "A5" }, "E4", "A4"),
	-- The in scale, with its half step: a koto's tuning, for tea.
	matcha = voice({ "D5", "Eb5", "G5", "A5", "C6" }, { "D5", "G5", "A5", "D6" }, "A4", "D5"),
	-- D major's five, higher than the rest: sharp.
	lemon = voice({ "D5", "E5", "F#5", "A5", "B5" }, { "D5", "F#5", "A5", "D6" }, "A4", "D5"),
	-- F major's five, the lowest: slow and warm.
	caramel = voice({ "F4", "G4", "A4", "C5", "D5" }, { "F4", "A4", "C5", "F5" }, "F4", "F4"),
	-- G major's five, between the rest: the desktop's own, plain as it.
	system = voice({ "G4", "A4", "B4", "D5", "E5" }, { "G4", "B4", "D5", "G5" }, "D4", "G4"),
}

-- Freestyle sings in the key of the flavour whose icing is nearest its
-- own round the wheel (a grey icing, in System's).
local HUES: { { id: string, hue: number } } = {}
for _, id in { "strawberry", "blueberry", "matcha", "lemon", "caramel" } do
	table.insert(HUES, { id = id, hue = (gmnb.to_hsl(hex(FLAVOURS[id].icing))) })
end

local function nearest_voice(icing: Rgba): Voice
	local h, s = gmnb.to_hsl(icing)
	if s < 0.08 then
		return VOICES.system
	end
	local best, apart = VOICES.strawberry, math.huge
	for _, f in HUES do
		local d = math.abs(h - f.hue) % 360
		d = math.min(d, 360 - d)
		if d < apart then
			best, apart = VOICES[f.id], d
		end
	end
	return best
end

local function voice_for(id: OptionValue?, ctx: Ctx): Voice
	if id == "custom" then
		return nearest_voice(iced(colour(ctx.options.icing, DEFAULT_ICING)))
	end
	return (if type(id) == "string" then VOICES[id] else nil) or VOICES.strawberry
end

-- Which sound for which event. This table is all of it if `sound` below
-- is ever switched off: the keys then keep a pitch each, in no scale.
local sounds: Sounds = {
	digit = { sound = { blip_a, blip_b }, pick = "key", rate = { 0.84, 1.5 }, pan = "position" },
	operator = { sound = bop, rate = { 0.985, 1.015 }, pan = "position" },
	func = { sound = tap, pick = "key", rate = { 0.94, 1.06 }, pan = "position" },
	equals = { sound = set, pan = "position" },
	toggle = { sound = flip, pan = "position" },
	clear = sweep,
	backspace = { sound = back, rate = { 0.97, 1.03 }, pan = "position" },
	memory = { sound = jar, pan = "position" },
	button = { sound = tap, volume = 0.9, pan = "position" },
	result = { sound = note, rate = 1.5, volume = 0.7, delay = 0.09 },
	-- It starts as the number begins to shake its head.
	error = { sound = womp, delay = 0.03 },
	mode = door,
}

-- Number keys pressed so far, and results in a row from = alone (pressed
-- again, it repeats the operation).
local turn = 0
local repeats = 0

local function sound(event: SoundEvent, ctx: Ctx): SoundChoice
	local v = voice_for(ctx.options.flavour, ctx)
	if event.kind == "press" then
		local key = event.key
		if not key or event.action then
			repeats = 0
			return nil
		end
		if key.kind ~= "equals" then
			repeats = 0
		end
		local pan = event.pan * 0.6
		if key.kind == "number" then
			-- The key's note in the flavour's scale, the two takes in turn.
			turn += 1
			local id = key.id or 0
			local play: Play = {
				sound = if turn % 2 == 0 then blip_a else blip_b,
				rate = v.keys[id * 7 % #v.keys + 1],
				volume = 0.88 + 0.12 * (turn * 0.618034 % 1),
				pan = pan,
			}
			return play
		elseif key.kind == "operator" then
			local play: Play = { sound = bop, rate = v.low, pan = pan }
			return play
		elseif key.kind == "equals" then
			local play: Play = { sound = set, rate = v.home, pan = pan }
			return play
		end
		return nil
	elseif event.kind == "result" and event.page == "calculator" then
		if event.cause == "equals" then
			repeats += 1
			if repeats == 1 then
				-- The jingle: three notes up the flavour's chord, and the
				-- patter of sprinkles where the fourth would be.
				local jingle: { Play } = {
					{ sound = note, rate = v.jingle[1], volume = 0.8, pan = -0.3, delay = 0.09 },
					{ sound = note, rate = v.jingle[2], volume = 0.85, pan = 0, delay = 0.17 },
					{ sound = note, rate = v.jingle[3], volume = 0.9, pan = 0.3, delay = 0.25 },
					{ sound = patter, rate = v.home, volume = 1, pan = 0, delay = 0.36 },
				}
				return jingle
			end
			-- = again and again: the top note and the patter, each time.
			local again: { Play } = {
				{ sound = note, rate = v.jingle[4], volume = 0.6, pan = 0, delay = 0.09 },
				{ sound = patter, rate = v.home, volume = 0.7, pan = 0, delay = 0.2 },
			}
			return again
		elseif event.cause == "function" then
			-- A function's result (a square root, a sine): one note.
			local one: Play = { sound = note, rate = v.jingle[3], volume = 0.6, delay = 0.06 }
			return one
		end
		-- The running total an operator shows: its own note said it.
		return false
	elseif event.kind == "error" then
		repeats = 0
	elseif event.kind == "option" and event.id == "flavour" then
		-- A new flavour plays its jingle through, where the sprinkles
		-- shake out from under the awning.
		local fresh = voice_for(event.value, ctx)
		local run: { Play } = {}
		for i, speed in fresh.jingle do
			run[i] = { sound = note, rate = speed, volume = 0.7, pan = (i - 2.5) * 0.24, delay = 0.08 * (i - 1) }
		end
		return run
	end
	return nil
end

local theme: Theme = {
	api = 1,
	name = "Sprinkles",
	description = "An 8-bit doughnut shop. Press = and your number comes out glazed, with sprinkles.",
	author = "GMNB contributors",
	license = "MIT",
	fonts = { "fonts/Jersey10-Regular.ttf" },
	credits = { { title = "Jersey 10 typeface", file = "fonts/OFL-Jersey10.txt" } },
	options = options,
	style = style,
	sounds = sounds,
	sound = sound,
	layers = {
		background = { draw = background, fade = false },
		overlay = { draw = fountain, events = { press = FOUNTAIN_LIFE + 0.3, result = FOUNTAIN_LIFE, option = RAIN_LIFE } },
		keypad = { draw = keypad, place = place, events = { press = CRUMB_LIFE, show = 1.2, result = 0.6, error = 0.3 } },
		display = { draw = display, events = { press = 0.12, result = 1.0, error = 1.1 } },
	},
}

return theme
