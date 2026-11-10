-- Mono: the base look with tight corners, a typewriter face and an accent
-- you pick. The whole theme is this file: what it leaves out stays as it is.
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
			-- The base colours, worked out afresh from this accent: text on it
			-- and beside it stays readable whatever was picked.
			colors = gmnb.base(ctx.dark, ctx.options.accent),
			fonts = { ui = "monospace", display = "monospace" },
			shape = { radius = corner, row_radius = corner, card_radius = corner * 2 },
			keys = { gap = 2 },
		}
	end,
}
