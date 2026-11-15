--!strict
-- Modules: a theme that draws with a WebAssembly module, for the tests
-- and for luau-lsp (docs/theming.md, "Modules"). Not one of GMNB's
-- themes.
--
-- Its module (`paint.wasm`, written by `tests/modules.rs` from text) has
-- `fill(w, h, out)`, which paints a w × h picture at `out` in its memory,
-- `tint(r, g, b)`, which sets the colour it paints with, and `bump()`. The
-- theme names the module at the top of its file; it is compiled when the
-- theme is applied, and used only in the hooks.

local paint = gmnb.module("paint.wasm")
local picture = paint:picture()
local tile = paint:picture()

-- Where in the module's memory the picture is painted.
local OUT = 1024

local function size(width: number, height: number): (number, number)
	return math.clamp(math.floor(width / 8), 1, 512), math.clamp(math.floor(height / 8), 1, 512)
end

-- Typed apart, as the built-in themes do: an option's `type` is then one
-- of the names, not any string.
local options: { Option } = {
	{
		id = "tint",
		type = "choice",
		label = "Tint",
		choices = { { id = "warm", label = "Warm" }, { id = "cool", label = "Cool" } },
		preview = function(draw: Draw, frame: Preview, choice: string): ()
			if choice == "warm" then
				paint:call("tint", 255, 128, 0)
			else
				paint:call("tint", 0, 128, 255)
			end
			paint:call("fill", 16, 16, OUT)
			tile:take(paint, OUT, 16, 16)
			draw:image(tile, 0, 0, frame.width, frame.height, { filter = "nearest" })
		end,
	},
}

local theme: Theme = {
	api = 1,
	name = "Modules",
	description = "A picture a WebAssembly module paints",
	options = options,
	layers = {
		background = {
			draw = function(draw: Draw, frame: Frame): number?
				-- The contract's way: no work when the picture wouldn't
				-- take it (the layer is drawn again when it would).
				if picture:ready() then
					local w, h = size(frame.width, frame.height)
					local bytes = paint:call("fill", w, h, OUT)
					assert(bytes == w * h * 4)
					picture:take(paint, OUT, w, h)
				end
				draw:image(picture, 0, 0, frame.width, frame.height, { filter = "linear" })
				local seen = paint:read(0, 4)
				if seen and buffer.readu32(seen, 0) > 0 then
					paint:write(0, buffer.create(4))
				end
				return nil
			end,
		},
	},
}

return theme
