-- Swirls: a demonstration of draw:field, for the tests and for looking at.
-- Not one of GMNB's themes.
--
-- The picture is the background of s0y b0y: the ColorMix field
-- (fairyglade/ly, WTFPL, src/animations/ColorMix.zig) pushed through
-- three colour bands. The page computes it in a fragment shader. Here the
-- theme computes the smooth part, the distorted coordinate, on a coarse
-- grid ten times a second; the app interpolates it for every pixel thirty
-- times a second and only then looks the colour up, so the bands' edges
-- stay sharp (docs/theming-media.md, "Fields").

local cos, sin, sqrt = math.cos, math.sin, math.sqrt
local floor, ceil = math.floor, math.ceil
local writef32 = buffer.writef32
local TAU = 2 * math.pi

-- The page's three colours: gold, cyan, near-black. A band blends into
-- the next between 8% and 92% of its width, and after the last comes the
-- first again.
local ramp = gmnb.ramp({
	colors = { "#ffcf29", "#0ed6f0", "#010609" },
	edge = { 0.08, 0.92 },
	cycle = true,
})

-- How often the field is computed, and how often the picture is made of
-- it: the page's own rates.
local FIELD_RATE = 10
local SHOWN_RATE = 30

-- One snapshot of the field at `time`: the distorted coordinate (u, v) at
-- every point of a cols × rows grid whose corners are the corners of a
-- width × height picture, as two 32-bit floats a point.
local function swirl(buf: buffer, cols: number, rows: number, width: number, height: number, time: number)
	local p = (time * 0.06) % TAU
	local px, py = 1.31 + p, 4.27 - p
	local sx, sy = 0.825 * width / height, 1.65
	local o = 0
	for j = 0, rows - 1 do
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

-- What a layer keeps between frames: two snapshots a tenth of a second
-- apart, and which tenth they are of.
type Kept = { cols: number, rows: number, a: buffer, b: buffer, tick: number }
local kept: { [string]: Kept } = {}

local function draw_swirl(where: string, draw: Draw, frame: Frame): number?
	if (frame.ctx.options.where or "background") ~= where then
		return nil
	end
	local width, height = frame.width, frame.height
	if width < 2 or height < 2 then
		return nil
	end
	-- A point every 12 pixels, or every 16.
	local step = tonumber(frame.ctx.options.grid) or 12
	local cols = math.min(ceil(width / step) + 1, 256)
	local rows = math.min(ceil(height / step) + 1, 256)
	local k = kept[where]
	if not k or k.cols ~= cols or k.rows ~= rows then
		k = { cols = cols, rows = rows, a = buffer.create(cols * rows * 8), b = buffer.create(cols * rows * 8), tick = -1 }
		kept[where] = k
	end
	-- The field ten times a second: when a new tenth begins, the later
	-- snapshot becomes the earlier one and one new snapshot is computed.
	local t = frame.time * FIELD_RATE
	local now = floor(t)
	if now ~= k.tick then
		if k.tick >= 0 and now == k.tick + 1 then
			k.a, k.b = k.b, k.a
		else
			swirl(k.a, cols, rows, width, height, now / FIELD_RATE)
		end
		swirl(k.b, cols, rows, width, height, (now + 1) / FIELD_RATE)
		k.tick = now
	end
	-- The picture thirty times a second: the app glides between the two
	-- snapshots (on the numbers, before the colours) and colours every
	-- pixel through the ramp.
	draw:field(0, 0, width, height, {
		data = k.a,
		next = k.b,
		mix = t - now,
		cols = cols,
		rows = rows,
		channels = 2,
		ramp = ramp,
		scale = 1.25,
	})
	return if frame.motion then 1 / SHOWN_RATE else nil
end

return {
	api = 1,
	name = "Swirls (demo)",
	description = "The ColorMix swirl drawn with draw:field. A demonstration, not a theme.",
	license = "MIT",
	options = {
		{
			id = "grid",
			type = "choice",
			group = "Swirl",
			label = "A point of the field every",
			default = "12",
			choices = { { id = "12", label = "12 pixels" }, { id = "16", label = "16 pixels" } },
		},
		{
			id = "where",
			type = "choice",
			group = "Swirl",
			label = "Drawn",
			description = "Over everything, the window is the swirl alone: for comparing pictures",
			default = "background",
			choices = { { id = "background", label = "Behind the calculator" }, { id = "overlay", label = "Over everything" } },
		},
	},
	layers = {
		background = {
			draw = function(draw, frame)
				return draw_swirl("background", draw, frame)
			end,
		},
		overlay = {
			draw = function(draw, frame)
				return draw_swirl("overlay", draw, frame)
			end,
		},
	},
}
