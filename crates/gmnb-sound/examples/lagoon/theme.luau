-- The sounds of the chapter's opening example (docs/theming-media.md, "A
-- living background and key sounds"), without its picture: a click under
-- every key and a chime for a result.
--
--   cargo run -p gmnb-sound --example render -- crates/gmnb-sound/examples/lagoon OUT

-- A soft click: a falling sine for the body, a pinch of noise for the edge.
local click = gmnb.synth({
	gain = 0.5,
	layers = {
		{ freq = { 950, 420, time = 0.03 }, env = { attack = 0.0005, decay = 0.045 } },
		{ wave = "noise", seed = 1, filter = { type = "bandpass", freq = 3800, q = 1.2 },
		  env = { attack = 0, decay = 0.012 }, gain = 0.35 },
	},
})

-- A glassy chime: two bells a major third apart, in a little room.
local function bell(note, at, pan)
	return { freq = note, fm = { ratio = 3.5, index = { 2.6, 0.05, time = 0.35 } },
	         env = { attack = 0.002, decay = 0.9 }, at = at, pan = pan }
end
local chime = gmnb.synth({
	gain = 0.4, length = 1.6,
	layers = { bell("E6", 0, -0.3), bell("G#6", 0.075, 0.3) },
	effects = { { type = "reverb", size = 0.55, damp = 0.3, mix = 0.3 } },
})

return {
	api = 1,
	name = "Lagoon",
	sounds = {
		-- Every key: the click, never twice the same pitch, placed where the key is.
		key = { sound = click, rate = { 0.96, 1.04 }, pan = "position" },
		equals = false,          -- the result's chime speaks for it
		result = chime,
	},
}
