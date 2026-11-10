-- The design's ten example sounds, as a theme: something to render and
-- listen to (see ../render.rs), and recipes to start from.
--
--   cargo run -p gmnb-sound --example render -- crates/gmnb-sound/examples/example-sounds OUT
--
-- gmnb-theme's tests hold each of these to the samples the design's
-- reference renderer made (tests/sound.rs), so don't tune them here: copy
-- one into your own theme and tune it there.

-- A soft click: a falling sine for the body, a higher one for the tick,
-- a pinch of noise for the edge.
local click_soft = gmnb.synth({
	gain = 0.5,
	layers = {
		{ freq = { 950, 420, time = 0.03 }, env = { attack = 0.0005, decay = 0.045 } },
		{ freq = { 2400, 1800, time = 0.012 }, env = { attack = 0, decay = 0.015 }, gain = 0.15 },
		{ wave = "noise", seed = 1, filter = { type = "bandpass", freq = 3800, q = 1.2 },
		  env = { attack = 0, decay = 0.012 }, gain = 0.35 },
	},
})

-- The same click, a touch lower and duller: for function keys.
local click_low = gmnb.synth({
	gain = 0.5,
	layers = {
		{ freq = { 700, 300, time = 0.035 }, env = { attack = 0.0005, decay = 0.055 } },
		{ wave = "noise", seed = 2, filter = { type = "bandpass", freq = 2600, q = 1.0 },
		  env = { attack = 0, decay = 0.014 }, gain = 0.3 },
	},
})

-- A glassy chime: two FM bells a major third apart, in a little room, and
-- a faint octave above, a few cents sharp, so the tail shimmers.
local function bell(note, at, pan, gain)
	return { freq = note, fm = { ratio = 3.5, index = { 2.6, 0.05, time = 0.35 } },
	         env = { attack = 0.002, decay = 0.9 }, at = at, pan = pan, gain = gain }
end
local chime_glass = gmnb.synth({
	gain = 0.4, length = 1.6,
	layers = {
		bell("E6", 0, -0.3, 1), bell("G#6", 0.075, 0.3, 0.85),
		{ freq = "E7", detune = 7, env = { attack = 0.004, decay = 0.6 }, gain = 0.2 },
	},
	effects = { { type = "reverb", size = 0.55, damp = 0.3, mix = 0.3 } },
})

-- An error buzz: two short low growls, the second lower. Not harsh.
local function growl(freq, at)
	local env = { attack = 0.004, decay = 0.02, sustain = 0.8, hold = 0.07, release = 0.04 }
	local fall = { freq, freq * 0.94, time = 0.12, curve = "linear" }
	return { wave = "square", width = 0.35, freq = fall, env = env, at = at,
	         filter = { type = "lowpass", freq = 900, q = 0.8 } },
	       { wave = "saw", freq = fall, detune = 14, env = env, at = at, gain = 0.5,
	         filter = { type = "lowpass", freq = 700, q = 0.7 } }
end
local a1, a2 = growl(116.5, 0)
local b1, b2 = growl(98, 0.15)
local error_buzz = gmnb.synth({
	gain = 0.42,
	layers = { a1, a2, b1, b2 },
	effects = { { type = "drive", amount = 0.25 } },
})

-- Equals: a soft thump with a bright tick on top.
local equals_thump = gmnb.synth({
	gain = 0.55,
	layers = {
		{ freq = { 240, 95, time = 0.07 }, env = { attack = 0.001, decay = 0.14 } },
		{ wave = "triangle", freq = "E5", env = { attack = 0.001, decay = 0.09 }, gain = 0.25 },
		{ wave = "noise", seed = 3, filter = { type = "bandpass", freq = 5200, q = 2.0 },
		  env = { attack = 0, decay = 0.01 }, gain = 0.25 },
	},
})

-- A mode change: air moving past.
local mode_whoosh = gmnb.synth({
	gain = 0.35,
	layers = {
		{ wave = "noise", seed = 4,
		  filter = { type = "bandpass", freq = { 500, 3200, time = 0.22 }, q = 1.4 },
		  env = { attack = 0.09, decay = 0.2 } },
	},
})

-- Toggles: two notes up for on, the same two down for off.
local function pip(note, at)
	return { wave = "triangle", freq = note, env = { attack = 0.002, decay = 0.07 }, at = at }
end
local toggle_on = gmnb.synth({ gain = 0.4, layers = { pip("A5", 0), pip("D6", 0.055) } })
local toggle_off = gmnb.synth({ gain = 0.4, layers = { pip("D6", 0), pip("A5", 0.055) } })

-- An arcade result: a rising square arpeggio, crushed, with an echo.
local function step(note, i)
	return { wave = "square", width = 0.25, freq = note, at = i * 0.06,
	         env = { attack = 0.001, decay = 0.01, sustain = 0.6, hold = 0.035, release = 0.03 } }
end
local arcade_result = gmnb.synth({
	gain = 0.3, length = 1.2,
	layers = { step("C5", 0), step("E5", 1), step("G5", 2), step("C6", 3) },
	effects = {
		{ type = "crush", bits = 5, rate = 11025 },
		{ type = "delay", time = 0.12, feedback = 0.35, mix = 0.3, ping_pong = true },
	},
})

-- Clear: a quick fall with a wobble.
local clear_fall = gmnb.synth({
	gain = 0.4,
	layers = {
		{ wave = "triangle", freq = { 1400, 260, time = 0.16 },
		  vibrato = { rate = 28, depth = 60 }, env = { attack = 0.002, decay = 0.18 } },
	},
})

return {
	api = 1,
	name = "Example sounds",
	description = "The ten example sounds of the sound chapter, one per kind of event.",
	sounds = {
		-- Every key: the soft click, each key at its own pitch, placed where
		-- the key is.
		key = { sound = click_soft, pick = "key", rate = { 0.94, 1.06 }, pan = "position" },
		func = { sound = click_low, rate = { 0.97, 1.03 }, pan = "position" },
		operator = { sound = click_low, rate = 1.15, pan = "position" },
		equals = equals_thump,
		toggle = { sound = { toggle_on, toggle_off }, pick = "cycle" },
		clear = clear_fall,
		backspace = { sound = click_soft, rate = 1.5, volume = 0.6 },
		button = { sound = click_low, volume = 0.8 },
		result = { sound = { chime_glass, arcade_result }, delay = 0.06 },
		error = error_buzz,
		mode = mode_whoosh,
	},
}
