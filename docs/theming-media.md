# Theming GMNB: pictures that move, and sound

This chapter adds **media** and **sound** to theme API **1**. It belongs to
[the theme contract](theming.md) and follows its rules: the first half is
for people writing a theme, and from [For implementers](#for-implementers)
on it is the specification the app is built to.

- The type definitions are in [`types/gmnb.d.luau`](../types/gmnb.d.luau).
- Everything here is optional. A theme that uses none of it costs nothing
  more, and GMNB without a theme loads none of it.

What a theme gains:

| | With |
|---|---|
| Still pictures: PNG, JPEG, WebP, GIF | `gmnb.image` (as before; now decoded by the app itself) |
| Animated pictures: GIF, APNG, animated WebP | `gmnb.animation`, drawn with `draw:image` |
| Silent video loops | An animated WebP, made from the video with one command |
| Pictures the theme computes | `gmnb.pixels`: a Luau `buffer` of pixels, drawn as a picture |
| Swirls, plasma, contour bands, gradient maps | `draw:field`: a coarse grid of numbers the theme computes, coloured per pixel by the app through a `gmnb.ramp` |
| Sounds for keys, results, errors | `sounds = { … }`, `sound(event)`, `gmnb.synth`, `gmnb.sound` |

Not in API 1, and why: [What is in API 1 and what is deferred](#what-is-in-api-1-and-what-is-deferred).

Contents

1. [A living background and key sounds](#a-living-background-and-key-sounds)
2. [Pictures](#pictures)
3. [Sound](#sound)
4. [Events: what is new](#events-what-is-new)
5. [The `gmnb` library: additions](#the-gmnb-library-additions)
6. [Limits](#limits)
7. [What is in API 1 and what is deferred](#what-is-in-api-1-and-what-is-deferred)
8. [For implementers](#for-implementers)

---------------------------------------------------------------------------

## A living background and key sounds

```lua
-- ~/.local/share/gmnb/themes/lagoon/theme.luau
local water = gmnb.animation("images/water.webp")   -- a 2 s loop, 480×270

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
	layers = {
		background = {
			draw = function(draw, frame)
				-- Plays on this layer's clock; the app redraws when the next frame is due.
				draw:image(water, 0, 0, frame.width, frame.height)
			end,
		},
	},
	sounds = {
		-- Every key: the click, never twice the same pitch, placed where the key is.
		key = { sound = click, rate = { 0.96, 1.04 }, pan = "position" },
		equals = false,          -- the result's chime speaks for it
		result = chime,
	},
}
```

What this shows:

- A picture, moving or not, is loaded once at the top of the file and drawn
  with `draw:image`.
- An animation needs no clock of your own. It moves while the window is in
  use and stands still otherwise, like everything else a layer draws.
- A sound is **text**: a recipe the app renders once into samples. There is
  no sound file to record, license or ship (though `gmnb.sound` loads one
  if you have it).
- `sounds` says which sound goes with which event. Nothing plays unless
  the person using GMNB has theme sounds switched on.

## Pictures

```
mytheme/
  theme.luau
  images/           stills and animations
  sounds/           sound files, if any
```

### Still pictures

`gmnb.image("images/stars.png")` loads a PNG, JPEG, WebP or GIF (its first
frame) and returns a picture: `{ width, height }` plus what the app needs.

- What a file is, is read from its first bytes, never from its name. A file
  that is none of the four is refused.
- 16-bit PNGs are reduced to 8 bits. Colour profiles and rotation tags are
  ignored: save pictures in sRGB, the right way up.
- Load pictures at the top of the file. The call reads the file and looks
  at its header, so the picture has its size at once. The pixels are
  decoded when the picture is first drawn (some tens of milliseconds for
  the largest allowed), and never on your hook's time. Later calls with
  the same name return the same picture at no cost.
- A file that is cut short or spoiled may still have a sound header. It
  then has a size and draws nothing, and Settings says so under the theme
  once it has been drawn.

### Animated pictures

```lua
local fire = gmnb.animation("images/fire.gif")
-- fire.width, fire.height, fire.frames, fire.duration (seconds, one pass)
```

GIF, APNG and animated WebP. The app does what each format asks for:
frame delays, transparency, frames that cover part of the picture, and
what is left behind when a frame ends.

- A frame's delay is the file's. One shorter than 20 ms (in GIFs this is
  usually a mistake for "as fast as possible") is taken as 100 ms, as
  browsers do.
- An animation always loops. How many times the file says to repeat is
  ignored.
- Frames are decoded as they are shown, not all at the start, so a long
  loop costs no more memory than a short one. Against the
  [image budget](#limits) an animation counts as four to six of its
  frames, by its format: a GIF the least, an animated PNG the most.

**Drawing one.** `draw:image(fire, x, y, w, h)` draws the frame that is due
on the layer's own clock, `frame.time`:

| The layer | The animation |
|---|---|
| is being animated | plays |
| waits: the window is hidden or not the active one, or the app has settled | stands on the frame it reached |
| `frame.motion == false` (reduced motion), and in a `preview` tile | shows its first frame, whatever `time` says |

**The next frame.** A layer that drew an animation this way is drawn again
when the animation's next frame is due, as if `draw` had returned that
delay. If `draw` returns a smaller number, that one wins. You don't work
out frame delays yourself:

```lua
draw = function(draw, frame)
	draw:image(fire, 0, 0, frame.width, frame.height)
	-- returning nothing is fine: the animation keeps the layer going
end
```

At most 30 frames a second are shown. A faster animation keeps its speed
and skips frames.

**Your own clock.** Pass `time` to say where in the animation you are;
the app then schedules nothing, and what `draw` returns decides as usual.

```lua
-- Play the burst once whenever a result arrives.
local e = frame.events[#frame.events]          -- this layer listens for "result"
if e and e.age < burst.duration then
	draw:image(burst, e.x - 60, e.y - 60, 120, 120, { time = e.age, loop = false })
	return 0
end
```

`loop = false` holds the last frame once `time` passes the end. An
animation has one playhead: draw it from one layer, or with the same
`time` everywhere. (Its first frame is the exception: a preview's tile or
`draw:tile` can show it while the animation plays elsewhere.)

**What it costs.** Decoding happens off the interface's thread and uses at
most a quarter of one processor core. If an animation is too heavy for
that it plays slower rather than using more, and Settings says so under
the theme. Measured on the development machine, for 2 s loops at 24 frames
a second:

| Loop | GIF | APNG | WebP (lossy) |
|---|---|---|---|
| 480×270, soft gradients | 177 KiB, 0.4% of a core | 799 KiB, 0.6% | 157 KiB, 2.3% |
| 480×270, busy detail | 2.7 MiB, 2.3% | 6.0 MiB, 2.8% | 427 KiB, 3.1% |
| 760×640, soft gradients | 646 KiB, 3.2% | 1.3 MiB, 1.0% | 176 KiB, 3.9% |
| 760×640, busy detail | 9.9 MiB (too big), 8.0% | 21.2 MiB (too big), 10.2% | 1.5 MiB, 13.3% |

Those are the prototype's figures. The app as built takes more, measured
while the machine's processors were capped and shared with other work:
1.3% (GIF), 2.2% (APNG) and 4.5% (WebP) for the soft 480×270 loop, 3.4%
and 4.5% for the busy GIF and WebP of that size, 5.3%, 5.4% and 8.7% for
the soft 760×640 loop, and 17.6% for the busy 760×640 WebP, which is
close to all the quarter allows. A loop that fills the window is best
kept soft, or small and scaled up.

For photographs and film, WebP is six times smaller than GIF. For flat
shapes and few colours, GIF and APNG are cheapest to play.

### Video loops

A silent video loop is an animated WebP. Make one from any video:

```sh
ffmpeg -i clip.mp4 -an -vf "fps=24,scale=480:-2" -c:v libwebp_anim -q:v 70 -loop 0 images/clip.webp
```

`-an` drops the sound; `fps` and `scale` set how much there is to decode;
`-q:v` trades size for quality. A window-sized background is seldom worth
more than 480 pixels across: `draw:image` scales it smoothly.

WebM and MP4 files are not played. [Why](#why-video-files-are-deferred).

### Pictures the theme computes

```lua
local W, H = 96, 80
local canvas = gmnb.pixels(W, H)
local buf = buffer.create(W * H * 4)          -- Luau's own buffer library

draw = function(draw, frame)
	for i = 0, W * H - 1 do
		local v = math.floor(127 + 127 * math.sin(i * 0.01 + frame.time))
		buffer.writeu32(buf, i * 4, 0xff000000 + v * 0x010101)   -- 0xAABBGGRR
	end
	canvas:set(buf)
	draw:image(canvas, 0, 0, frame.width, frame.height, { filter = "nearest" })
	return 1 / 15
end
```

- `gmnb.pixels(width, height)` makes a picture you fill yourself. It
  starts transparent. Asked for again with the same size, it is the same
  picture.
- `canvas:set(buffer)` copies `width × height × 4` bytes into it: one
  pixel after another, row by row from the top, each as red, green, blue,
  opacity (not premultiplied). Written as one number with
  `buffer.writeu32` that is `0xAABBGGRR`. It returns `true`, or `false` if
  the picture was changed too recently ([limits](#limits)); the old
  picture then stays, and the app draws your layer once more when the
  picture can be changed again, so a picture that stands is never left
  out of date.
- Setting it to the bytes it already holds changes nothing and costs
  nothing.
- The buffer is yours and counts against the theme's 8 MiB of memory; the
  picture is the app's and counts against the image budget.

A pixel costs about 17 ns to write and far more to compute, and a `draw`
call has 8 ms. Measured: one plain write per pixel fills 380×320 in
2.1 ms; a real effect worked out per pixel in Luau managed 95×80 in
3.9 ms. So this is for small pictures, pictures that change rarely, and
pixel art. For a whole window of smooth colour, use a field.

### Fields: swirls, plasma, contours

Most procedural backgrounds are a smooth quantity (a distance, a height, a
warped coordinate) pushed through a colour scheme with sharp edges. The
smooth part can be computed coarsely. The sharp part cannot: colours
worked out on a coarse grid and scaled up come out blurred and broken.

So the two are split. The theme computes the **field**, a coarse grid of
numbers. The app interpolates the field for every pixel it draws and only
then looks up the colour, in a **ramp** the theme declared.

```lua
local ramp = gmnb.ramp({
	colors = { "#ffcf29", "#0ed6f0", "#010609" },
	edge = { 0.08, 0.92 },     -- where in a band the blend to the next colour starts and ends
	cycle = true,              -- after the last colour comes the first again
})

draw:field(0, 0, frame.width, frame.height, {
	data = buf, cols = 65, rows = 55, channels = 2,
	ramp = ramp, scale = 1.25,
	quality = 0.5,
})
```

**The field.** `data` is a buffer of 32-bit floats (`buffer.writef32`),
`cols × rows × channels` of them, row by row from the top. The grid's
corners sit on the corners of the rectangle being drawn. With one channel
a point's **value** is the number itself. With two, it is the length of
the pair, `√(u² + v²)`: the pair is interpolated first, so a field that
winds around a centre keeps its rings round.

**The ramp.** With `n` colours, a pixel's **position** is
`value × scale + offset`. Its whole part picks a band and so a colour; the
fraction says how far through the band it is, and between `edge[1]` and
`edge[2]` the colour blends smoothly into the next one.

| Ramp | Gives |
|---|---|
| `cycle = true`, a few colours, `edge = { 0.08, 0.92 }` | Soft repeating bands: swirls, oil on water |
| `cycle = true`, `edge = { 0.5, 0.5 }` | Hard-edged bands: contour maps, posterised plasma |
| `cycle = false`, `edge = { 0, 1 }` (the default) | A gradient map: position 0 is the first colour, `n − 1` the last, nothing repeats |

**Moving it.** Changing the field every frame is wasteful: it changes
slowly. Compute it a few times a second and let the app glide between two
snapshots with `next` and `mix`. The glide is done on the numbers, before
the colours, so the bands stay sharp while they move.

```lua
-- The ColorMix swirl (fairyglade/ly, WTFPL): the field ten times a second,
-- the picture thirty times a second.
local COLS, ROWS = 65, 55                       -- a point every 12 px of a 760×640 window
local a = buffer.create(COLS * ROWS * 8)
local b = buffer.create(COLS * ROWS * 8)
local tick = -1

local function swirl(buf, width, height, time)
	local p = (time * 0.06) % (2 * math.pi)
	local px, py = 1.31 + p, 4.27 - p
	local sx, sy = 0.825 * width / height, 1.65
	local o = 0
	for j = 0, ROWS - 1 do
		for i = 0, COLS - 1 do
			local u, v = (2 * i / (COLS - 1) - 1) * sx, (1 - 2 * j / (ROWS - 1)) * sy
			local u2 = u + v
			local v2 = u2
			for _ = 1, 3 do
				local len = math.sqrt(u * u + v * v)
				u2 += u + len
				v2 += v + len
				u += 0.5 * math.cos(px + v2 * 0.2)
				v += 0.5 * math.sin(py + u2)
				local d = math.cos(u + v) - math.sin(u * 0.7 - v)
				u -= d
				v -= d
			end
			buffer.writef32(buf, o, u)
			buffer.writef32(buf, o + 4, v)
			o += 8
		end
	end
end

draw = function(draw, frame)
	local t = frame.time * 10
	local now = math.floor(t)
	if now ~= tick then
		if tick >= 0 and now == tick + 1 then a, b = b, a else swirl(a, frame.width, frame.height, now / 10) end
		swirl(b, frame.width, frame.height, (now + 1) / 10)
		tick = now
	end
	draw:field(0, 0, frame.width, frame.height, {
		data = a, next = b, mix = t - now,
		cols = COLS, rows = ROWS, channels = 2, ramp = ramp, scale = 1.25,
	})
	return if frame.motion then 1 / 30 else nil
end
```

What that costs, measured for a 760×640 window in the app's own engine:
the field takes 1.6 ms in Luau (0.9 ms with a point every 16 px) ten
times a second, and a whole `draw` call that only glides, the app's
colouring pass at `quality = 0.5` included, 0.5 ms thirty times a second.
Together about 3% of one core while the window is in use, and nothing at
rest. (Handing each picture to the graphics card and redrawing the window
thirty times a second come on top: the whole app, with the software
renderer, used 9 to 12% of a core, and 1 to 2 MiB of memory more than
with the same theme standing still.) The same picture worked out per pixel on
the processor takes 62 ms a frame.

The built-in theme Swirls
([`apps/gmnb/themes/swirls`](../apps/gmnb/themes/swirls/theme.luau)) is
this example grown into a theme: the same field behind glass keys, with a
grid that keeps its number of points whatever the window's size, a clock
quantised to thirtieths of a second, the paint stirred where keys are
pressed (by moving where the field is looked up), and the readout's
figures cut out of a slice of it.

A field that hasn't changed isn't coloured again. Drawn with the same
numbers, the same ramp and the same size as last time, it is the picture
already made, at no cost: a layer that is redrawn for another reason (an
event, the pointer) pays nothing for a background that stands still.

`quality` is how many pixels the app computes per pixel of the rectangle
along each side (0.1 to 1; 0.5 by default). The app may use less to stay
under its [limit](#limits). The result is scaled up smoothly.

### `draw:image`, in full

```lua
draw:image(image, x, y, w, h, { filter = "linear", time = 1.5, loop = false })
```

| Option | Meaning | Default |
|---|---|---|
| `filter` | `"trilinear"`: smooth, and clean when drawn much smaller. `"linear"`: smooth. `"nearest"`: hard pixels. | `"trilinear"` for `gmnb.image` and `gmnb.noise`; `"linear"` for animations and `gmnb.pixels` |
| `time` | Animations: where in the animation, in seconds. | The layer's clock |
| `loop` | Animations, with `time`: wrap around past the end, or hold the last frame. | `true` |

`draw:tile` takes stills and `gmnb.pixels` pictures; of an animation it
tiles the first frame. Of the options it reads only `filter`, which
matters only where the screen has more pixels than the window (scale 2):
there `"nearest"` keeps a tile's pixels hard.

## Sound

A theme may give GMNB a voice: a click under each key, a chime for a
result. It is the theme's choice to have sounds and the user's choice to
hear them.

### Which sound for which event

```lua
sounds = {
	press = click,                                    -- anything pressed
	digit = { sound = { tick1, tick2, tick3 }, pick = "cycle" },
	operator = { sound = click, rate = 0.8 },
	equals = false,                                   -- nothing, though `press` is set
	backspace = { sound = click, rate = 1.5, volume = 0.6 },
	result = { sound = chime, delay = 0.04 },
	error = buzz,
}
```

Each event has a name. One with no entry uses the next name in its row
that has one; `false` stops the search and plays nothing.

| Name | When | Then tries |
|---|---|---|
| `digit` | A number key (0 to 9, the point, A to F) | `key`, `press` |
| `operator` | + − × ÷ and their like | `key`, `press` |
| `func` | Any other function key | `key`, `press` |
| `equals` | The equals key | `key`, `press` |
| `toggle` | A key that switches something on or off (2nd, hyp) | `key`, `press` |
| `clear` | C, CE, and the graphing keypad's clear | `key`, `press` |
| `backspace` | Backspace | `key`, `press` |
| `memory` | MC, MR, M+, M−, MS, and the buttons on a stored value | `button`, `press` |
| `button` | F-E, the converter's swap, a date picked, other buttons | `press` |
| `result` | A calculation ended in a value, on the Calculator page: after `=` (or Enter) or a function key, not the running total an operator shows (its `cause` is `"equals"` or `"function"`) | |
| `error` | It ended in an error, on the Calculator page, whatever brought it about (`1 ÷ 0 +` too) | |
| `mode` | The page or the calculator's mode was changed | |

A key typed on the keyboard sounds the same as the key clicked.

An entry is a sound, or a table:

| Field | Meaning | Default |
|---|---|---|
| `sound` | A sound, or a list of up to 8 to choose from | required |
| `pick` | How to choose and vary: `"cycle"` (the list in turn), `"random"`, or `"key"` (by the key: the same key always sounds the same) | `"cycle"` |
| `volume` | 0 to 1, or a range `{ low, high }` | 1 |
| `rate` | Speed and with it pitch: 0.5 (an octave down) to 2, or a range | 1 |
| `pan` | −1 (left) to 1 (right), a range, or `"position"`: where the control is across the window, at 60% width | 0 |
| `delay` | Seconds to wait before it starts, up to 1 | 0 |

A range picks a new value for each play: at random, or with
`pick = "key"` by the key, so that each key keeps its own pitch and place.
A narrow `rate` range (`{ 0.97, 1.03 }`) is what stops a click from
sounding like a machine.

### Deciding in code

```lua
local streak = 0
sound = function(event, ctx)
	if event.kind == "result" and event.cause == "equals" then
		streak += 1
		return { sound = chime, rate = math.min(1 + streak * 0.06, 1.5) }   -- each result a little higher
	elseif event.kind == "result" and event.cause == "operator" then
		return { sound = chime, volume = 0.3, rate = 2 }   -- the sum so far, faintly
	elseif event.kind == "error" then
		streak = 0
		return nil            -- the `sounds` table's error
	end
	return nil
end
```

`sound(event, ctx)` is called for every `press`, `result`, `error`, `mode`
and `option` event, before the table is consulted. `event` is what a layer
would see (see [Events](theming.md#events)), in window coordinates, with
`age` 0, and one more field: `pan`, where it happened across the window,
−1 at the left edge to 1 at the right. `ctx` is the whole of what
`style(ctx)` is given: light or dark, the accent, the theme's options
(`ctx.options.palette`) and the rest.

| It returns | Meaning |
|---|---|
| `nil` | Use the `sounds` table as if there were no function. |
| `false` | Play nothing. |
| a sound | Play it. |
| `{ sound = …, volume = …, rate = …, pan = …, delay = … }` | Play it that way (single numbers, one sound). |
| a list of up to 4 of those | Play them together: a click now, a chime in 80 ms. |

It has 2 ms, and like a draw hook it is switched off after three
failures; the table then decides alone. It sees `result` on every page
(the converter gives one with each digit) and with every `cause` (the
running total `+` shows is `"operator"`), and `option` when the user
changes one of the theme's options, none of which the table plays.

### Making a sound

```lua
gmnb.synth({
	gain = 0.5,            -- how loud the finished sound's loudest moment is, 0 to 1
	length = 1.2,          -- optional: the whole length, echoes included
	layers = { … },        -- 1 to 8 tones or noises, mixed
	effects = { … },       -- up to 4, applied to the mix in order
})
```

Every sound is rendered once, at 48 000 samples a second in stereo, and
then scaled so that its loudest sample is `gain`. You cannot clip, and you
balance sounds against each other with one number each.

**A layer** is one oscillator with its own loudness curve:

| Field | Meaning | Default |
|---|---|---|
| `wave` | `"sine"`, `"triangle"`, `"square"`, `"saw"`, `"noise"` | `"sine"` |
| `freq` | Pitch: hertz, a note name (`"A4"` is 440, `"C#5"`, `"Eb3"`), or a glide `{ from, to, time = seconds, curve = "exp" }`. For noise: left out, every sample is new (white noise); given, each value is held for one period (a crunchy, retro noise). | 440 |
| `detune` | Cents (hundredths of a semitone) | 0 |
| `width` | Square: the fraction of each period spent high, 0.05 to 0.95 | 0.5 |
| `phase` | Where in its period the wave starts, 0 to 1 | 0 |
| `seed` | Noise: which noise. Give layers different seeds. | `0x9e3779b9` |
| `fm` | `{ ratio, index }`: a second sine at `ratio` × the pitch bends this one's phase by up to `index` radians. `index` may glide. Whole ratios sound like reeds and brass, others like bells and glass. | none |
| `vibrato` | `{ rate, depth }`: pitch wobble, `rate` in hertz, `depth` in cents | none |
| `tremolo` | `{ rate, depth }`: loudness wobble, `depth` 0 to 1 | none |
| `filter` | `{ type, freq, q }`: `"lowpass"`, `"highpass"` or `"bandpass"`; `freq` may glide; `q` 0.1 to 30 (0.707 is flat) | none |
| `env` | The loudness curve, below | |
| `gain` | Its share of the mix | 1 |
| `pan` | −1 to 1 | 0 |
| `at` | Seconds into the sound at which it starts | 0 |

**The envelope** `env = { attack, decay, sustain, hold, release, curve }`:

```
level 1 ─────╮
            ╱ ╲
           ╱   ╲___________  sustain
          ╱                ╲
     0 ──╯                  ╲──
        attack decay  hold  release
```

- `attack` seconds from silence to full, `decay` seconds down to the
  `sustain` level (0 to 1), `hold` seconds there, `release` seconds to
  silence. Defaults: 0.002, 0.1, 0, 0, 0.
- With `sustain = 0` (the default) the layer is a pluck: it ends after
  `attack + decay`, and `hold` and `release` are not used.
- `curve = "exp"` (the default) falls the way a struck thing does, 60 dB
  over the segment; `"linear"` falls in a straight line. The attack is
  always a straight line.
- A layer's length is the sum of the parts it uses.

**A glide** `{ from, to, time = seconds, curve = "exp" | "linear" }` goes
from the first value to the second over `time` (left out: the layer's
whole length) and stays there. `"exp"`, the default, moves by equal
musical steps and needs both values above zero.

**Effects**, applied to the whole mix:

| Effect | Fields | Does |
|---|---|---|
| `{ type = "delay", time, feedback, mix, ping_pong }` | seconds (0.001 to 1); 0 to 0.9; 0 to 1; boolean | Echoes, each quieter by `feedback`; `ping_pong` bounces them left and right |
| `{ type = "reverb", size, damp, mix }` | each 0 to 1 | A room: `size` its length, `damp` how dull its walls |
| `{ type = "drive", amount }` | 0 to 1 | Warm overdrive |
| `{ type = "crush", bits, rate }` | 1 to 16; 100 to 48 000 | Fewer bits and fewer samples a second: arcade grit |
| `{ type = "lowpass" \| "highpass" \| "bandpass", freq, q }` | as a layer's filter | Filters the mix; `freq` may glide over the whole sound |

A number left out of an effect is: delay `time` 0.25, `feedback` 0.35,
`mix` 0.3; reverb `size` 0.5, `damp` 0.5, `mix` 0.3; drive `amount` 0.5;
crush `bits` 8, `rate` 12 000. A filter, as an effect or on a layer,
needs its `freq` (without one it is left out, with a note); its `q` is
0.707. Left out of a layer: `fm.ratio` is 1 and `fm.index` 0, and the
`rate` and `depth` of a vibrato or tremolo are 0.

A sound with a delay or reverb rings on after its layers end. The app
keeps the tail until it has faded (or until `length`, or 4 s) and trims
the silence after it.

**The same recipe always gives the same samples**, on every machine and in
every version of the app that speaks API 1. Themes can be shared as text
and reviewed by reading.

**From a file.** `gmnb.sound("sounds/tap.ogg")` loads a WAV or Ogg Vorbis
file, mono or stereo, up to 4 s. `gmnb.sound(file, { gain = 0.5 })` scales
it like a synthesised sound; without `gain` it plays as recorded.

### When sounds play, and when they don't

- **Only if the user lets them.** Settings → Appearance → Sound has a
  switch, *Theme sounds*, and a volume. If the desktop is set to make no
  event sounds, GMNB makes none either.
- **Only in answer to the user.** Nothing sounds while the app starts,
  while it restores what it was showing, or when the window is not the one
  in use.
- **Keyboard and pointer alike.** But not for a control operated through
  the accessibility interface (a screen reader's "activate"), nor for the
  result or error such a press brings about: speech is that person's
  feedback, and a click or a chime on top of it helps nobody.
- **Reduced motion does not silence sound.** It is a setting about
  movement. The sound switch is the setting about sound.
- **Overlapping sounds mix.** Up to 12 at once; a thirteenth fades out the
  oldest. The same event again within 30 ms is dropped, so a held key
  rattles rather than roars.
- **No sound system, no fuss.** Without a sound server the app is silent
  and says so once, in Settings.
- **When the theme is unloaded** or sound is switched off, what is
  sounding fades out within 30 ms and everything sound-related is let go.

**Sound never carries information by itself.** Every sound accompanies
something GMNB already shows and already tells a screen reader. An error
is on the display and announced whether or not it buzzes. A theme has no
way to make a sound mean something the screen does not, and built-in
themes are reviewed for it.

### Designing sounds: a few rules that work

- **Short.** A key sound over 80 ms smears fast typing. Results can ring
  for a second; nothing needs more than two.
- **Quiet, and quieter for what happens most.** `gain` 0.5 for a click
  that lasts 40 ms is modest; a sustained tone at 0.5 is loud. Start
  digits around 0.4 to 0.5 and tones around 0.3.
- **A click is two things.** A body (a sine falling an octave in 30 ms)
  and an edge (5 to 15 ms of band-passed noise). Change the body's pitch
  for weight and the noise's band for material.
- **Vary a little.** `rate = { 0.97, 1.03 }` or three takes with
  `pick = "cycle"`.
- **Leave the low end alone.** Laptop speakers have nothing below 200 Hz
  and headphones have too much. A `highpass` at 120 costs nothing.
- **One family.** Derive a theme's sounds from two or three recipes by
  pitch, length and filter, rather than ten unrelated ones.
- **Equals and result arrive together.** Give `equals` a short dry sound
  and `result` a `delay` of 40 to 80 ms, or silence one of them.
- **A result is not only the equals key's.** The app reports a result
  whenever a calculation gives a new value: after `=`, and also after `+`
  once a number is in (the sum so far) and after `sin` or `√`. Each says
  what brought it about, its `cause`: `"equals"`, `"operator"`,
  `"function"`, and so on. The table plays `sounds.result` for
  `"equals"` and `"function"` only, so a chime there rings once in
  `1 + 2 + 3 =`; `sounds.error` it plays whatever the cause. `sound` hears every one, and can answer
  each in its own way: by `event.cause`, or by the key pressed last (the
  press comes first, then what it came to), as Aurora does.
- **Panned is louder.** A sound in the middle plays at its `gain` in both
  ears. Moved to one side it keeps its power, so it is louder there: 2.6 dB
  at the 60% of `pan = "position"`, 3 dB all the way over.

`GMNB_SOUND_DUMP=<folder>` writes every sound a theme makes to a WAV file
there, to listen to and to look at in an editor.

**Checking the levels.** Listen first, then measure. Three commands, none
of which plays anything:

```
cargo run --release -p gmnb-sound --example render -- mytheme out
python3 -I tools/theming/sound_check.py out
python3 -I tools/theming/sound_sheet.py out sheet.png
```

The first writes a theme's sounds and events to WAV files in `out`: every
sound by itself; every event eight times on keys across the window,
decided as the app decides it (`sound` first, then the table), a result or
an error after the equals key; each value of the theme's choices and
toggles that `sound` answers; and `typing.wav`, twenty presses at eight a
second with the results the app reports along the way. The second
measures them and prints a table. The third draws them, a spectrogram
each, on one page.

`sound_check.py` holds every built-in theme to one budget. The numbers
are at the top of the script, with how each is measured:

| | Budget | Over it |
|---|---|---|
| A sound's loudest sample (its `gain`) | 0.5, which is −6 dBFS | fails |
| An event's loudest sample as mixed: panned, and several plays together | 0.71, −3 dBFS: one sound at 0.5 panned hard over. The limiter starts at 0.75. | fails |
| `digit`, `operator`, `func`, `backspace`: from the event to 60 dB under its peak, the longest of its eight takes | 80 ms | fails |
| `equals`, `toggle`, `clear`, `memory`, `button` | 250 ms | fails |
| `result`, `error`, `mode`, and what `sound` plays for an option | 2 s | fails |
| Loudness, against the median of the theme's sounds, or of its events | within 12 LU | flagged |
| Typing: twenty presses at eight a second | −20 LUFS at full volume, about speech on the radio | flagged |
| Energy under 120 Hz | 20 dB under the whole | flagged |
| DC offset | −40 dBFS | flagged |

Loudness is measured as ITU-R BS.1770 does (K-weighted, the loudest
400 ms; typing over 3 s), so a short sound counts for less than a long one
at the same peak, as it does to the ear. A failure is a broken rule. A
flag is a question to answer by listening: an error may well be meant to
be the loudest thing a theme has.

## Events: what is new

Four additions to [the contract's events](theming.md#events), for layers
and for sound alike:

| Addition | Meaning |
|---|---|
| `press` events have `action` | `"clear"`, `"backspace"` or `"memory"` when the control does that; absent otherwise. What a control *is* (`key.kind`) and what it *does* differ: both C and sin are function keys. |
| `via` may be `"assistive"` | The control was operated through the accessibility interface. Layers get the event; sound ignores it. |
| `result` and `error` have `cause`, and the press's `via` | What brought it about: the press just before it. `cause` is `"equals"` (= or Enter), `"operator"` (the running total), `"function"`, `"digit"`, `"clear"`, `"backspace"`, `"memory"` or `"button"`; absent when no press did (a paste). `via` is that press's, so what follows an assistive press is silent too. |
| A new `kind`: `"mode"` | The user changed page or calculator mode. `page` is the new page; `rect` is the page's area. It is not sent for the page the app starts on. A layer listens with `events = { mode = 1 }`. |

## The `gmnb` library: additions

| Function | Returns |
|---|---|
| `gmnb.image(file)` | A still picture (PNG, JPEG, WebP, GIF). Unchanged for themes. |
| `gmnb.animation(file)` | An animated picture: `{ width, height, frames, duration }`. |
| `gmnb.pixels(width, height)` | A picture the theme fills: `{ width, height }` and `:set(buffer)` → boolean. |
| `gmnb.ramp(spec)` | A colour ramp for `draw:field`: `{ colors, edge?, cycle? }`. |
| `gmnb.synth(recipe)` | A sound rendered from a recipe. |
| `gmnb.sound(file, { gain }?)` | A sound from a WAV or Ogg Vorbis file. |
| `gmnb.note(name)` | A note name's frequency in hertz: `gmnb.note("A4")` is 440. |

All of them remember their arguments: called again with the same ones they
return the same thing at no cost. Make pictures and sounds at the top of
the file. One made inside a hook works, but a sound made there may miss
the event that made it: it plays only once it is rendered.

New draw calls:

| Call | GtkSnapshot / GSK |
|---|---|
| `draw:image(image, x, y, w, h, options?)` | `append_scaled_texture` with the chosen filter |
| `draw:field(x, y, w, h, spec)` | The app fills an RGBA picture on the processor, then `append_scaled_texture` (linear) |

New fields of the theme table:

| Field | Type | Meaning |
|---|---|---|
| `sounds` | table | Which sound for which event. |
| `sound` | function | `sound(event, ctx)` → what to play. |

## Limits

These rows join [the contract's table](theming.md#limits-and-what-happens-when-a-theme-fails).

| Limit | Value |
|---|---|
| Images: formats | PNG, JPEG, WebP, GIF (the existing rows for size and count apply) |
| Animations: file; frame size; frames; count | 8 MiB; 1024×1024; 2048; 4 |
| Animations: counted against "Images: total decoded" | By what playing one holds: a GIF 4¼ of its frames, a WebP 5, an animated PNG 6 (a frame is width × height × 4 bytes); a file of one frame, that frame |
| Animations: frames shown; decoding | 30 a second; a quarter of one core, all animations together |
| Pixel pictures: size; count; changes | 512×512; 4; 30 a second each |
| Fields: points; per `draw`; computed pixels per call; passes | 16 384, each side 2 to 256; 2; 262 144; 30 a second per layer |
| Ramps: colours; count | 2 to 16; 8 |
| Sounds: count; length; all together | 32; 4 s; 8 MiB (43 s) |
| Sound files: size; kinds | 1 MiB; WAV (8, 16, 24 or 32-bit, or float), Ogg Vorbis; 1 or 2 channels; 8 to 96 kHz |
| Sound recipes: layers; effects; layer-seconds over the whole theme | 8; 4; 64 |
| Voices at once; plays per event; the same event again | 12; 4; not within 30 ms |
| A `sounds` entry's list | 8 sounds |
| A play: `volume`; `rate`; `pan`; `delay` | 0 to 1; 0.5 to 2; −1 to 1; 0 to 1 s |
| Time: one `sound` call | 2 ms |

**A picture is never decoded inside one of the theme's calls.** A still
is decoded when it is first drawn, after the hook that drew it has
returned; an animation's frames on a thread of their own. What does
happen inside the call counts against its time limit like any other work
done on the theme's behalf: reading a file (a tenth of a millisecond for
a picture of 2 MiB), counting an animation's frames, copying a buffer in
`canvas:set`, and the colouring pass of `draw:field`, which is part of
drawing. All of it is bounded by the sizes above.

| What goes wrong | What the user sees |
|---|---|
| A picture or sound file can't be read, is of another kind than its first bytes promise, or is over a limit | The theme loads without it (drawing it draws nothing; playing it plays nothing); a note. |
| A picture's file has a sound header and its contents can't be decoded | It has its size and draws nothing; a note, when it is first drawn. An animation that fails after its first frame stands on the frame it reached. |
| An animation is too heavy to decode in time | It plays slower. A note, once. |
| `canvas:set` or `draw:field` is called more often than allowed | `set` returns `false`; the field's last picture is drawn again. The layer is drawn once more when there is room, so what stands is never out of date. No note. |
| A buffer is the wrong size for its picture or field | That call does nothing; a note, once per call site. |
| A recipe has an unknown key or a value of the wrong type | That value's default is used; a note. A number out of range is clamped; a note. |
| A recipe has no layers, or more than the limits allow | The sound is silent; a note. |
| `sound` fails or runs out of time | The `sounds` table decides for that event. After 3 failures the function is switched off; a toast, once. |
| There is no sound server, or it goes away | Silence. Settings → Sound says "No sound system found". No toast. |

## What is in API 1 and what is deferred

| | In API 1 | Deferred |
|---|---|---|
| Stills | PNG, JPEG, WebP, GIF, through the app's own decoders | SVG, AVIF, JPEG XL; colour profiles |
| Animations | GIF, APNG, animated WebP, streamed | Playing backwards; several playheads on one animation |
| Video | As animated WebP | WebM, MP4 and other video files |
| Procedural | `gmnb.pixels`, `draw:field` with one or two channels and a ramp | Custom shaders; ramps with uneven bands; fields that warp another picture |
| Sound sources | `gmnb.synth`; WAV and Ogg Vorbis files | FLAC, Opus, MP3 |
| Sound events | The twelve names above; `sound(event)` | Sounds for history, copy and paste, sheets and menus; looping or ambient sound; stopping a sound once started |
| Sound output | PulseAudio's protocol (PipeWire or PulseAudio) | ALSA or JACK directly |

Everything deferred can be added to API 1 later without breaking a theme
([Versions](theming.md#versions)).

### Why video files are deferred

Playing a WebM or MP4 means GStreamer, through GTK's `GtkMediaFile`. It
was tried (GTK 4.22.4, GStreamer 1.28.7, a 2 s 854×480 VP9 loop in WebM,
muted, on a private headless compositor; AV1 and H.264 were not tried):

| Finding | Detail |
|---|---|
| It can take the app down | With GTK's media backend present but GStreamer's base plug-ins missing, creating the `GtkMediaFile` ended the process: `GstPlay: 'playbin3' element not found` is a fatal error inside the library. In the builds examined (nixpkgs), GTK 4.22.4 has the backend linked into the library itself and 4.18.6 loads it as a module, so on 4.22 this is one missing package away. With no backend at all GTK is polite: the stream's `error` says "GTK could not find a media module". |
| Looping was unreliable | With `loop` set, the clip restarted in one run of four. In the other three it reported `playing` with no error and showed no new frame after the first pass. When it did loop it was gapless: 300 frames in 10 s, longest gap 36.9 ms (at 44% of one core, much of it the software renderer scaling each frame: the test compositor has no graphics card). |
| Memory and threads | With the GL renderer already up, playing added 102 MiB of resident memory and 31 threads. Pausing brought CPU to 0.2% of a core but freed neither; closing the file freed 17 threads and no memory. |
| Start | 0.3 to 4.8 s from creating the file to its first frame (the longest with an empty plug-in cache). |
| No way to say "only this" | `GtkMediaFile` takes a file and picks demuxers and decoders itself, from every GStreamer plug-in installed, hardware decoders included. A theme file from a stranger would be parsed by whichever C code claims it. Restricting it needs the GStreamer API itself (lowering the rank of everything not on an allow-list), a second dependency. |
| Packaging | The plug-ins are separate packages on every distribution, and which codecs they hold differs (H.264 most of all). |

An animated WebP gives the same thing on screen with a memory-safe decoder
that is part of the app, the same on every system, at 1 to 6 ms a frame.
Video files can join API 1 later, as an optional capability, if that
proves not to be enough.

---------------------------------------------------------------------------

## For implementers

### Where the code lives

| Crate | Holds | Depends on |
|---|---|---|
| `crates/gmnb-media` (new) | What a file is and how big (`sniff.rs`); decoding stills (`still.rs`); an animation's timeline (`anim.rs`), its frames composited (`stream.rs`), the player and its thread (`player.rs`); ramps and the field pass (`field.rs`); the allowance (`bucket.rs`). No toolkit and no Luau: bytes in, RGBA bytes out. | `png`, `zune-jpeg`, `gif`, `image-webp`; `libc` for a thread's processor time |
| `crates/gmnb-sound` (new) | The synthesiser (`synth.rs`); reading sound files (`file.rs`, `resample.rs`); the mixer (`mixer.rs`); the table of sounds by event and how an entry becomes a play (`table.rs`); the PulseAudio client (`pulse.rs`); the engine and its thread (`engine.rs`); for tests only, a stand-in sound server (`standin.rs`, the `stand-in` feature) | `crmath`, `hound`, `lewton`, `pulseaudio` |
| `crates/gmnb-theme` | `gmnb.synth`, `gmnb.sound`, `gmnb.note`; checking the `sounds` table and what `sound` returns; an event's name, and what an event plays (`decide`: steps 1 to 4 below, which the app and the render example share) (`sound.rs`). It declares and decides; it makes no sound. And the pictures' functions: `gmnb.image`, `gmnb.animation`, `gmnb.pixels` (`picture.rs`), `gmnb.ramp` and `draw:field` (`field.rs`), `draw:image`'s options (`draw.rs`): the caps, which frame of an animation is due, the allowances. It decodes nothing while a theme's code runs. | `gmnb-sound`, for the types; `gmnb-media` |
| `apps/gmnb` | Turning bytes into `gdk::MemoryTexture`, owning the animation player, and the one place its thread reaches the main loop (`layers/replay.rs`, `layers/mod.rs`); the one sound engine, handed each theme's sounds, and every event's plays (`src/sound.rs`, held by the hub in `src/theme.rs`); Settings | both |

Rules these keep:

- **DGMNB links neither crate.** The check in `.github/workflows/ci.yml`
  ("DGMNB links no Luau") is widened to fail if `cargo tree -p dgmnb`
  names any of `gmnb-theme`, `gmnb-media`, `gmnb-sound`, `mlua`, `luau`,
  `pulseaudio`, `lewton`, `ogg`, `hound`, `gif`, `weezl`, `image-webp`,
  `zune-jpeg`, `zune-core`. (`png`, `futures-*`, `bitflags` and `log` are
  already DGMNB's through its own dependencies and are not in the list.)
- **No new native library.** Every dependency above is Rust. GMNB's list
  of linked libraries (`readelf -d`) does not change, so nothing more is
  mapped when the base look starts.
- **Nothing starts until used.** No thread, socket, decoder state or
  texture exists for media or sound until a theme asks for one:

  | Thing | Created | Gone |
  |---|---|---|
  | The media thread | The first time an animation needs a frame after its first | The theme is unloaded |
  | The sound threads and the connection to the sound server | A theme with `sounds` or `sound` is loaded while *Theme sounds* is on and the desktop allows event sounds | The theme is unloaded, or the switch goes off |
  | The playback stream | With the connection, corked | With it |

- Themes never reach any of this directly: a script has no file, socket or
  device access, only the functions here.

### Decoding stills

`gdk::Texture::from_bytes` is not used for theme files. What it does was
checked in the libraries themselves (nixpkgs' builds of GTK 4.18.6 and
4.22.4, by the symbols they import): both hand PNG, JPEG and TIFF to
libpng, libjpeg and libtiff, chosen by the file's first bytes, and
**everything else to gdk-pixbuf**, that is to whichever loader modules the
system has. Neither uses glycin; the Flatpak runtime (freedesktop 26.08)
ships glycin, so there the answer may differ again. A theme's file would
be parsed by C code the app did not choose, in formats it did not mean to
accept.

Instead:

1. Read the file (inside the theme's folder; the path rules of the
   contract). Refuse over 4 MiB.
2. Sniff: `89 50 4E 47 0D 0A 1A 0A` PNG; `FF D8 FF` JPEG; `RIFF….WEBP`
   WebP; `GIF87a` / `GIF89a` GIF. Anything else is refused.
3. Read the header for the size. Refuse over 2048×2048, or if it would
   pass the count or the total.
4. Decode to RGBA8 with the decoder's own limits set to match (`png`
   `Limits`, `zune` `set_max_width/height`, `gif` `MemoryLimit`; for
   WebP see below). PNG: `EXPAND | ALPHA | STRIP_16`. JPEG:
   `DecoderOptions::new_safe()`, and zune-jpeg built without its `x86`
   and `neon` features (its portable code only: none of its `unsafe` SIMD
   paths is even compiled; at most 100 scans, its default), output RGBA.
   A decoder that panics is caught (`catch_unwind`) and the file refused.
5. Premultiply, and wrap as a `gdk::MemoryTexture`
   (`R8G8B8A8_PREMULTIPLIED`) that holds the bytes without copying them.

**When.** Steps 1 to 3 happen at the `gmnb.image` call: the theme is told
a size, and the caps are checked, from the header alone. Steps 4 and 5
happen when the app first draws the picture (`layers/replay.rs`), on the
thread that draws, after the hook that drew it has returned. A picture
loaded at the top of the file is there for the first frame all the same.
A theme that is only listed in Settings, or loads pictures it doesn't
draw, decodes nothing. A file whose header passes and whose contents
don't decode keeps the size the theme was told, draws nothing, and is
noted when that is found; it is not tried again.

**A WebP says its size twice**: in the file's header (for an animation,
in each frame's) and in the picture's own bitstream. `image-webp` 0.2.4
decodes a lossy picture at the size its bitstream says, allocating for
it, and compares with the header only afterwards; its `set_memory_limit`
covers metadata (ICC, EXIF, XMP) and no picture. Measured: a file of 126
bytes with a 16×12 header around a bitstream claiming 16 383×16 383 made
the decoder ask for 384 MiB before it refused. So `gmnb-media` reads the
bitstream's own size first (`VP8 `: 14 bits each after the start code;
`VP8L`: 14 bits each after the signature) and hands it to the decoder
only if it is the size already approved; a lossy bitstream must also be a
key frame whose first partition the chunk holds. Every picture reaches
the decoder as a still file made of just its own chunks (`ALPH`, `VP8 `,
`VP8L`), so what was checked is what is decoded. A WebP shorter than its
RIFF header says is refused as cut short.

**A lossless picture's codes are counted first.** A lossless bitstream
(`VP8L`, or the picture of opacity in an `ALPH` chunk) codes its pixels
with groups of five Huffman codes, as many groups as the largest number
in a small picture at its start says: up to 65 536, whatever the
picture's size. `image-webp` builds every group, a table of up to 1024
entries a code, before it decodes a pixel. Measured: a file of 3.8 MB,
one pixel, made it hold 1301 MiB for two seconds. So `gmnb-media` walks
the bitstream first, as the decoder will read it (`vp8l.rs`: the
transforms and their small pictures, the entropy image, then every
group's code lengths), adds up what the decoder will build, and hands it
on only if that is within 16 MiB. libwebp's files hold a few hundred
groups at most, well under 1 MiB of codes (the most seen, of 694
bitstreams by cwebp, gif2webp and img2webp: 31 groups, 576 KiB).

One thing is left open: a lossy bitstream also says how long each of its
later partitions is, in 24 bits, and the decoder allocates a partition
before finding that the file doesn't hold it. How many partitions there
are is inside the compressed header, out of reach without decoding. One
such allocation is alive at a time, so a spoiled lossy WebP can make the
decoder ask for up to 16 MiB that it never touches, for a moment
(7.1 MiB seen, from a file with one changed byte).

**A file that fails is remembered.** Asked for again, by the same name,
it is the same empty picture at no cost and with no second attempt, until
the theme is reloaded. It has also used one of the count (16 images, 4
animations, 32 sounds), so a theme can make the app try at most that many
files, good or bad, however its hooks are written.

Measured, 2048×2048, in the configuration above: PNG 31 ms (a detailed
picture) to 45 ms (film grain, 9.7 MiB, over the size limit); JPEG 34 ms.
Reading a header takes a microsecond. As built, with the machine's
processors capped and shared with other work: a 2.1 MiB PNG 45 ms, a
2.8 MiB JPEG 61 ms (`tests/memory.rs` prints both).

What a decoder holds while it works, counted with a counting allocator
(`crates/gmnb-media/tests/memory.rs`), as a multiple of the decoded
picture: PNG 1.1 (1.03 for a real 2048×2048 one), JPEG 1.03, GIF 1.26,
lossless WebP up to 1.64 (the picture, and its file once more). Files
with a few bytes changed, under a cap of 256×256, never held more than
288 KiB (but for the lossy WebP case above).

**No time is given back.** The first design had `Engine::credit`, to take
decoding off a call's deadline, because deadlines were of time on the
wall and a picture was decoded at the `gmnb.image` call. Neither holds
now: a time limit is of the processor time the calling thread has run,
and no picture is decoded inside a call. What the pictures' functions
still do inside a call is small and bounded by the limits: reading the
file, reading a header, counting an animation's frames (at most 0.9 ms).
It counts against the call like any other work. Measured in the engine,
the processor time of the whole call: `gmnb.image` of a 2.1 MiB PNG or a
2.8 MiB JPEG 0.12 ms; `gmnb.animation` of a 2.7 MiB GIF 0.35 ms, of a
1.3 MiB animated PNG or a 1.5 MiB WebP 0.09 ms (`gmnb-theme`'s
`tests/media.rs`, `what_making_a_picture_costs_its_call`). By those
figures a theme that loads sixteen images and four animations at the top
of the file spends a few milliseconds of its 250 on them.

The four decoders and the code around them add 836 KiB to GMNB's stripped
release binary (the decoders alone were measured at 664: png 176,
zune-jpeg 224, gif 96, image-webp 292). DGMNB links none of it. Its
binary grows by 8 KiB all the same: the new crates' licences, in the
list of them it shows.

### The animation player

**Opening** (`gmnb.animation`, on the calling thread, on the call's own
time):

1. Read (8 MiB at most), sniff: GIF, PNG with an `acTL` chunk before
   `IDAT`, or WebP with the animation flag. A still PNG, WebP or GIF is
   accepted as an animation of one frame. A JPEG is refused.
2. Walk the file without decoding pixels and build the timeline: GIF with
   `DecodeOptions::skip_frame_decoding(true)`; APNG by its `fcTL` chunks
   (which must be as many as `acTL` announces); WebP by its `ANMF` chunks
   (which `WebPDecoder::new` must count the same). Measured: at most
   0.9 ms, for a 9.9 MiB GIF.
3. Delays: GIF hundredths of a second; APNG `delay_num / delay_den` (a
   denominator of 0 is 100); WebP milliseconds. A delay under 20 ms
   becomes 100 ms. A delay over 10 s becomes 10 s.
4. Check the limits, and charge the image total with what playing it
   will hold (below).

No frame is decoded at the call. **Frame 1** is decoded when the
animation is first drawn, on the thread that draws, and kept for as long
as the theme is loaded: it is the still frame. Reduced motion, a
preview's tile and `draw:tile` show it without asking anything of the
media thread, which they therefore never start, and without disturbing
an animation that plays in another layer.

**Compositing** (`stream.rs`). One RGBA8 canvas of the file's canvas
size, transparent at the start of each pass.

- GIF: decoded to palette indices; the frame's rectangle is drawn,
  skipping transparent pixels; then disposed of as the frame says (keep;
  restore to transparent; restore the canvas as it was before the frame,
  for which a copy of that rectangle is kept).
- APNG: `blend_op` source or over; `dispose_op` none, background
  (transparent), previous (on the first frame: background). A first image
  that is not a frame (no `fcTL` before `IDAT`) is skipped.
- WebP: each frame's own picture is decoded by `image-webp` as a still
  (see "A WebP says its size twice") and composited here: blended or not
  as its flag says, its rectangle made transparent afterwards if it asks
  to be disposed of. `image-webp` 0.2.4's own compositing is not used:
  it ignores "dispose to background" unless it is given a background
  colour, and with one it clears the wrong bytes under a frame that has
  no alpha.

Blending is source-over on straight alpha, rounded to the nearest. The
compositing was compared once, frame by frame, with other programs' on
the six test animations this chapter's figures come from (ImageMagick
for GIF and, through libwebp, WebP; ffmpeg for APNG): 288 frames, all
identical. Which blends and disposals those six files use was not
catalogued (their frames cover parts of the picture, and three of them
are partly clear throughout); every blend and disposal is exercised by
the tests' hand-made files, against pictures written out by hand.

Each finished canvas is copied, premultiplied, and handed over; the
texture made of it holds those bytes without another copy.

**Playing** (`player.rs`). Per animation the main thread holds `first`,
`shown` and at most one frame `ahead`. A draw wants frame `i`: the last
frame whose start is at or before the position (`time`, or the layer's
clock; modulo `duration` when looping, else clamped).

| `i` is | Then |
|---|---|
| `shown` | Draw it. |
| `ahead` | It becomes `shown`; ask the thread for the frame after it (wrapping); draw it. |
| neither | Ask the thread for `i`; draw `shown` meanwhile. When `i` arrives it becomes `shown`, every layer that drew this animation in its last frame is painted again (its hook is not run again), and the frame after `i` is asked for. |

The first frame is never asked of the thread: after the last frame,
`ahead` is `first`.

The thread decodes forward only. Asked for a frame behind its place, it
starts the file again. Asked for one further ahead, it decodes and drops
the frames between. Of several requests for one animation it does the
latest.

**A frame that fails.** A frame that can't be decoded (a file spoiled
past its header; a decoder that panics, which is caught) leaves the
animation on the frame it reached, with a note; the thread is not asked
again, and goes on serving the others.

**What it holds**, counted with a counting allocator while each format
played (256×256 frames, one of them a part that asks for the previous
picture back): the first frame, the one shown, the one ahead (or being
made) and the thread's canvas are four frames. On top comes what the
format's decoder keeps between frames: a GIF's frame as palette indices,
a quarter of a frame (4.47 frames held in all); for a PNG the `png`
crate's own buffers, about a frame and a half (5.72); for a WebP nothing
between frames (4.01) and the frame being decoded while it is (4.68). And
the rectangle a frame asks to have put back. The image total is charged
4¼, 6 and 5 frames, plus that rectangle (`Timeline::charge`). (The first
design said four frames for every format.) A file of one frame never
plays, so no thread or canvas is made for it: it is charged its one
frame, as a still is.

**The quarter core.** After a frame that took `d` of processor time to
decode and composite (the thread's own, `CLOCK_THREAD_CPUTIME_ID`: a
machine busy with other things doesn't make an animation rest longer),
the thread does not start another for `3d`. One thread serves all the
theme's animations in turn, so the cap covers them together.

**Too heavy.** The player keeps the mean processor time a frame of each
animation takes: of the frames decoded so far and, once there are as
many as the file has, of about the last pass (frames of one file differ
in cost, its first being the dearest, and what counts is what a pass
takes). It is believed once two dozen frames are in it. The animations
asked for within the last
second need, at full speed, the sum of `time a frame / mean delay` of a
core. If that is more than a quarter, the app tells the theme's clock how
much slower to go (`Api::set_slow`): from then on an animation falls
behind its layer's clock by that factor, frame by frame, none skipped,
and its next frame is due that much later. At a factor of 1 its position
is the layer's clock exactly. A note says so, once.

**Scheduling.** A draw of an animation on the layer clock records "next
frame due in `max(start[i+1] − position, 1/30)`" (`Drawn::due`). It is
kept apart from what the hook returned (a readout's change ends when
`draw` returns `nil`, whatever it drew); the layer's request is the
smaller of the two. Everything else is the contract's frame clock: the
request is honoured only while the window is mapped and active, motion is
allowed and the app has not settled.

**Once a frame.** The toolkit rounds a frame's time to the display's
refresh, so the time a layer is drawn at can be some milliseconds before
or after the moment (up to 7 seen, at 60 a second). Three things keep a
layer's clock and its timer in step (`layers/mod.rs`, `draw.rs`):

- A layer its timer woke is drawn at a time no earlier than the timer's.
- A timer waits from the frame's time: one armed in a frame dated ahead
  of the moment waits that much longer.
- Timers are of whole milliseconds, so the frame due on a layer's clock
  is the last that starts within 2 ms (no frame is shorter than 20 ms).

Without them a layer was often woken a few milliseconds before its
animation's next frame was due on its clock. It drew the old frame again
and showed the frames after it late, a thirtieth of a second apart, until
it had caught up: a 24-a-second loop was drawn 28 times a second.
Measured now, for each of the nine loops below: 358 to 361 draws for the
361 frames decoded in 15 s.

**At rest:** no request, so no frame; the thread blocks on its queue;
0 wake-ups. Measured in the running app: with an animated background and
the window not in use, or under reduced motion, no media thread exists;
after the app has settled the thread's processor time and its count of
wake-ups (the kernel's) stand still. Over 10 s, every thread of the
settled app (an animated background, and the swirl) spent 0.000 ms on a
processor and was not woken once, like the base look's (each thread's
`schedstat` and voluntary context switches).

**Playing, measured** in the running app for the 2 s loops at 24 frames a
second of the authors' table, the media thread's own processor time
(decoding, compositing, the premultiplied copy), of one core:

| Loop | GIF | APNG | WebP (lossy) |
|---|---|---|---|
| 480×270, soft gradients | 1.3% | 2.2% | 4.5% |
| 480×270, busy detail | 3.4% | (file not kept) | 4.5% |
| 760×640, soft gradients | 5.3% | 5.4% | 8.7% |
| 760×640, busy detail | (too big) | (too big) | 17.6% |

It is woken once a frame. Nothing was said to play slower: the busy
760×640 WebP, the heaviest, keeps under the quarter. The whole app,
painting the window 24 times a second with the software renderer, used
8 to 12% of a core at 480×270 and 23 to 35% at 760×640: scaling and
painting a picture costs more than decoding it.

### Pixel pictures

`gmnb.pixels(w, h)` charges `w × h × 4` to the image total and starts
transparent. Like the others it remembers its arguments: the same size is
the same picture. `set(buffer)` checks the length and copies the bytes
(`mlua::Buffer::to_vec`); the app makes a new `gdk::MemoryTexture`
(`R8G8B8A8`, straight alpha) of them the next time the picture is drawn.
The old texture is dropped; the toolkit frees its copy on the graphics
card with it. Bytes equal to the ones it holds change nothing and spend
nothing. A picture over a limit has no size, and a `set` that returns
`false`.

The allowance: each picture has up to 2 tokens, gains 30 a second, and a
`set` spends one. (A plain "not within 33 ms" would refuse every other
frame of a steady 30 a second, since frames arrive a millisecond early as
often as late.) Fields use the same rule per layer.

**A refusal doesn't leave a stale picture standing.** When a `set` or a
field's pass is refused inside a hook, the layer is owed a frame
(`Drawn::retry`): the app draws it once more when the allowance has room,
under reduced motion and after a hook that returned `nil` too. Four such
frames in a row at most, so a hook that is refused every time it is drawn
(it sets one picture twice a frame) is not drawn for ever on that
account.

Measured in the real engine (sandbox, heap cap, deadline interrupt on):
17.2 ns per `buffer.writeu32`; 96×80 in 0.13 ms, 190×160 in 0.52 ms,
380×320 in 2.1 ms. The whole ColorMix effect per pixel, colours included:
64×54 in 1.8 ms, 95×80 in 3.9 ms, 190×160 in 15.4 ms.

### The field pass, exactly

Input: `data` (and `next`), `cols`, `rows`, `channels`, a ramp of `n`
colours with edges `e0 ≤ e1`, `scale`, `offset`, `mix`, `quality`, and
the rectangle `w × h`. All arithmetic in 32-bit floats.

1. **Output size.** `W = max(1, round(w × quality))`, `H` likewise. If
   `W × H > 262 144`, both are multiplied by `√(262 144 / (W × H))` and
   rounded down.
2. **The field.** If `next` is given, every value is
   `data + (next − data) × mix`. A value that is not finite counts as 0.
3. **Per output pixel** `(X, Y)`: `fx = (X + 0.5) / W × (cols − 1)`,
   `fy = (Y + 0.5) / H × (rows − 1)`; `i = min(floor(fx), cols − 2)`,
   `j` likewise; bilinear interpolation of each channel between points
   `(i, j)`, `(i+1, j)`, `(i, j+1)`, `(i+1, j+1)`, rows first
   (`row = top + (bottom − top) × ty`, then
   `value = left + (right − left) × tx`).
4. **Value.** One channel: the number. Two: `√(u² + v²)`.
5. **Position.** `p = value × scale + offset`. A position that is not
   finite counts as 0.
   - `cycle`: `p ← p − n × floor(p / n)`. Band `b = floor(p)`, fraction
     `f = p − b`; the colours are `b` and `(b + 1) mod n`.
   - not `cycle`: `p` is clamped to `0 … n − 1`. Band
     `b = min(floor(p), n − 2)`, `f = p − b`; the colours are `b` and
     `b + 1`.
6. **Blend.** `t = clamp((f − e0) / (e1 − e0), 0, 1)` (if `e0 = e1`:
   0 below it, 1 from it on); `s = t² (3 − 2t)`; the colour is
   `a + (b − a) × s` on the colours' sRGB-encoded channels and opacity
   (not on linear light: that is what the shaders this imitates do).
7. **The table.** Steps 5 and 6 are not done per pixel. When a ramp is
   made the app fills a table of 256 entries per band, entry `k` of band
   `b` being the colour at `f = (k + 0.5) / 256`, stored as premultiplied
   RGBA8 (each channel `floor(c × 255 + 0.5)`). A pixel reads entry
   `min(floor(f × 256), 255)` of its band.
8. The `W × H` picture becomes a `gdk::MemoryTexture`
   (`R8G8B8A8_PREMULTIPLIED`), drawn with `append_scaled_texture`
   (linear) over the rectangle.

Step 1's "rounded down" is done in whole numbers (the largest `W'` with
`W'² × H ≤ W × 262 144`), so that no rounding of a square root decides a
pixel. The rest is the steps as written, in 32-bit floats, with nothing
left to the compiler's taste: `floor` is computed in float arithmetic
alone (adding and taking away 2²³, which is exact), with no branch and
one conversion to a whole number a pixel, so a row is worked several
pixels at a time. `crates/gmnb-media/tests/field.rs` holds the pass equal,
byte for byte, to these steps written out plainly with the library's own
`floor` (120 fields, ramps and sizes, over a million pixels), and to a
stored hash.

The texture is of the picture's own bytes, without a copy, and the app
keeps it for as long as the picture is alive (`layers/replay.rs`): in a
layer's last frame, or as the last picture its field made. A field that
moves leaves no heap of old frames behind it (the first build kept the
last sixteen textures: 5.6 MiB for the swirl), and one that stands is
made a texture of once, in however many layers and tiles it is drawn.

**Where and how often.** The pass runs inside the `draw:field` call that
asks for it, on that call's time. Per layer (per keypad, per readout) it
runs 30 times a second at most, two in hand. Past that the field's last
picture is drawn again and the layer is owed a frame (see "Pixel
pictures"). Two fields in one `draw` take turns: the one that went
without last time goes first. A field asked for with the numbers (after
the glide), the ramp, `scale`, `offset` and size of its last picture *is*
that picture: nothing is coloured and nothing spent. A preview's tile is
drawn once and stands, so its fields are always coloured.

Measured for a 760×640 rectangle, one thread, no hand-written SIMD
(release build, the development machine under its processor cap):

| Computed pixels | As built | The first prototype's table | Blend computed per pixel |
|---|---|---|---|
| 760×640 (`quality` 1; over the cap, so not reachable) | 2.2 ms | 2.9 ms | 7.2 ms |
| 557×469 (the most allowed) | 1.2 ms | | |
| 380×320 (`quality` 0.5) | 0.55 ms | 0.74 ms | 1.8 ms |
| 253×213 (`quality` 1/3) | 0.25 ms | 0.33 ms | 0.8 ms |

In the real engine, a whole `draw` call of the swirl demonstration at
760×640 (`crates/gmnb-theme/tests/themes/swirls`): 0.53 ms of processor
time when it only glides, 2.2 ms when it also computes a field every
12 px (1.5 ms every 16 px); the Luau field itself 1.6 ms and 0.93 ms.

**Does the primitive earn its place?** Against the same shader worked out
per pixel (the picture the user's site draws), for the ColorMix swirl:

| How | Luau per update | Mean error, of 255 | Pixels off by more than 32 |
|---|---|---|---|
| Field every 16 px, coloured by the app at half size | 0.78 ms | 1.91 | 0.87% |
| Field every 12 px, coloured by the app at half size | 1.39 ms | 1.13 | 0.18% |
| Field every 8 px, coloured by the app at half size | 2.99 ms | 0.64 | 0.01% |
| Colours every 12 px from the theme (`gmnb.pixels`), scaled up | 1.76 ms | 4.41 | 4.94% |
| Colours every 8 px from the theme, scaled up | 3.87 ms | 2.38 | 2.34% |
| Colours every 4 px from the theme, scaled up | 15.4 ms (over the limit) | 0.74 | 0.20% |

A field every 12 px matches what colours every 4 px achieve, at a tenth
of the Luau time, and its errors are soft where the other's are broken
band edges. Stills of each are in `theming-work/media/out/swirls/`.

As built the same comparison is a test (`tests/field.rs`, the field
computed in Rust; `gmnb-theme`'s `tests/media.rs`, by the demonstration
theme's Luau in the real engine): 1.92 / 0.87% at 16 px, 1.14 / 0.18% at
12 px, 0.66 / 0.01% at 8 px; colours 4.42 / 4.94% at 12 px. With a
hard-edged ramp every pixel the pass makes is one of the ramp's colours;
scaled up from half size 4.3% of the window's pixels sit on a softened
edge, whatever the grid, against 19.8% for colours every 12 px.

### The synthesiser, exactly

A recipe is rendered to interleaved stereo 16-bit samples at **48 000 Hz**.
The result is a pure function of the recipe:

- All arithmetic is IEEE 754 binary64, each operation rounded once, in the
  order written below (no fused multiply-add, no reassociation).
- `sin`, `cos`, `exp`, `pow` and `tanh` are **correctly rounded**
  (`crmath`, CORE-MATH). Nothing else transcendental is used.
- `round` is to nearest, halves away from zero. `SR = 48000`,
  `TAU = 6.283185307179586`, `K = 6.907755278982137` (ln 1000).
- `samples(t) = max(0, round(t × SR))`.

This was checked: ten recipes rendered identically (SHA-256 of the
samples) in a debug build, a release build, a build for x86-64-v3 with
and without `+fma`, and with CORE-MATH's baseline and v3 code paths.

**Values.** A pitch is hertz, or a note name: a letter `A` to `G`, an
optional `#` or `b`, an octave −1 to 9; `C4` is MIDI note 60 and
`hertz = 440 × pow(2, (midi − 69) / 12)`. A glide `{ from, to, time,
curve }` at `t` seconds, with `whole` the length its `time` defaults to:
if `from = to`, that; else `u = 1` if `time ≤ 0` or `t ≥ time`, else
`t / time`; linear: `from + (to − from) × u`; exp:
`from × pow(to / from, u)`.

**A layer** starts at sample `samples(at)` and is `N` samples long, with
`na = samples(attack)`, `nd = samples(decay)`, and, only if `sustain > 0`,
`nh = samples(hold)` and `nr = samples(release)` (else both 0);
`N = na + nd + nh + nr`; `whole = N / SR`. Before the loop:
`E1 = exp(−K)`; `detune = pow(2, cents / 1200)`;
`(gr, gl) = (sin θ, cos θ)` with `θ = (pan + 1) × (TAU / 8)`;
`ph = phase − floor(phase)`; `mph = 0`; the noise state is `seed` (0
means `0x9e3779b9`), and a noise layer draws its first value now. For
sample `i` of the layer, `t = i / SR`:

1. **Envelope** `e`: for `i < na`, `i / na`. Then for the next `nd`,
   `sustain + (1 − sustain) × shape(u)` with `u = (i − na) / nd`. Then
   for `nh`, `sustain`. Then `sustain × shape(u)` with
   `u = (i − na − nd − nh) / nr`. `shape(u)` is `1 − u` (linear) or
   `(exp(−K × u) − E1) / (1 − E1)` (exp).
2. **Frequency**: `f = glide(t) × detune` (noise without `freq`: 0).
   With vibrato, `f ← f × pow(2, depth / 1200 × sin(TAU × rate × t))`.
   Clamp `f` to 0 … 20 000. `dt = f / SR`.
3. **Phase modulation**: without `fm`, `pm = 0`. With it,
   `m = sin(TAU × mph)`; then `mph ← mph + dt × ratio`,
   `mph ← mph − floor(mph)`; `pm = index(t) × m`.
4. **Oscillator** `o`:
   - sine: `sin(TAU × ph + pm)`.
   - the others use `p = ph + pm / TAU`, `p ← p − floor(p)`:
     - triangle: `q = p + 0.25`, `q ← q − floor(q)`, `1 − 4 × |q − 0.5|`;
     - saw: `2p − 1 − blep(p)`;
     - square, `w = width` clamped to 0.05 … 0.95: `(1 if p < w else −1)
       + blep(p) − blep(p2)` with `p2 = p + 1 − w`, `p2 ← p2 − floor(p2)`.
   - `blep(p)`: 0 if `dt ≤ 0`; if `p < dt`, with `x = p / dt`,
     `x + x − x × x − 1`; if `p > 1 − dt`, with `x = (p − 1) / dt`,
     `x × x + x + x + 1`; else 0.
   - noise: the held value. Without `freq` a new value is drawn first, on
     every sample.
   - A new noise value: `s ^= s << 13; s ^= s >> 17; s ^= s << 5` on 32
     bits (the generator of `gmnb.noise`), value `s / 2147483648 − 1`.
5. **Advance**: `ph ← ph + dt`; if `ph ≥ 1`, `ph ← ph − floor(ph)`, and a
   noise layer with `freq` draws a new value.
6. **Filter**, if any: when its `freq` glides, set its coefficients for
   `t` (they are also set once for `t = 0` before the loop);
   `o ← filter(o)`.
7. `v = o × e × gain`; with tremolo,
   `v ← v × (1 − depth × 0.5 × (1 − cos(TAU × rate × t)))`.
8. `left += v × gl`, `right += v × gr`.

**The filter** is the RBJ cookbook biquad. `fc` clamped to 20 … 20 000,
`Q` to 0.1 … 30; `(s, c) = (sin w, cos w)` with `w = TAU × fc / SR`;
`alpha = s / (2 × Q)`; `a0 = 1 + alpha`.

| Type | `b0`, `b1`, `b2` |
|---|---|
| lowpass | `(1 − c) / 2`, `1 − c`, `(1 − c) / 2` |
| highpass | `(1 + c) / 2`, `−(1 + c)`, `(1 + c) / 2` |
| bandpass | `alpha`, `0`, `−alpha` |

`a1 = −2c`, `a2 = 1 − alpha`; all five divided by `a0`. Transposed direct
form II: `y = b0 × x + z1`; `z1 = b1 × x − a1 × y + z2`;
`z2 = b2 × x − a2 × y`.

**The mix.** `dry` is the latest end of any layer, at most 192 000
samples (4 s). The buffer's length `n` is `samples(length)` if `length`
is given, else 192 000 if there is a delay or reverb, else `dry`; never
more than 192 000. Layers are added in order; samples past `n` are not
rendered.

**Effects**, in order, on both channels:

- **Filter**: the biquad above, one state per channel; a glide's `whole`
  is `n / SR`.
- **Delay**: `D = max(1, samples(time))` with `time` clamped to
  0.001 … 1; `fb` clamped to 0 … 0.9; two rings of `D` zeros. Per sample,
  with `dl`, `dr` the values read at the ring position: write
  `xl + fb × dl` and `xr + fb × dr` (ping-pong:
  `0.5 × (xl + xr) + fb × dr` to the left ring, `fb × dl` to the right);
  output `xl + mix × dl`, `xr + mix × dr`.
- **Reverb** (Freeverb's structure): `fb = 0.7 + 0.28 × size`,
  `dm = 0.4 × damp`, input `(xl + xr) × 0.015`. Per channel, eight combs
  in parallel of lengths 1215, 1293, 1390, 1476, 1548, 1623, 1695, 1760
  samples (the right channel's are 25 longer): `y` = the ring's value;
  `store ← y × (1 − dm) + store × dm`; write `input + store × fb`; the
  combs' `y` are summed. Then four all-passes in series of lengths 605,
  480, 371, 245 (right: 25 longer): `b` = the ring's value;
  `out = b − in`; write `in + b × 0.5`. Output `xl + mix × wet_l`,
  `xr + mix × wet_r`.
- **Drive**: `g = 1 + 9 × amount`; `x ← tanh(g × x) / tanh(g)`.
- **Crush**: `q = pow(2, bits − 1)`, `step = rate / SR` with `rate`
  clamped to 100 … 48 000; `acc = 1`. Per sample: if `acc ≥ 1`, then
  `acc ← acc − 1` and the held pair becomes `round(x × q) / q`; then
  `acc ← acc + step`; output the held pair.

**Finishing.**

1. Only if there is a delay or reverb, or `length` was given: with
   `peak` the largest magnitude in the buffer, drop frames from the end
   while both channels are below `peak / 1024`; then multiply the last
   `F = min(240, frames)` frames by a ramp, frame `last − k` by `k / F`.
2. `peak` again, over what is left. `scale = gain / peak` (0 if silent).
3. Each sample is `round(x × scale × 32767)`, as a 16-bit integer.

**Test vectors.** SHA-256 over the samples as little-endian 16-bit,
left before right:

| Recipe | Frames | SHA-256 |
|---|---|---|
| The click below | 2184 | `21c869ae7b1c53cb7049330a055fc4e7be80f991af3e04719b7ceaa69b5ce816` |
| The chime below | 65 624 | `767b821cea4900be32d7dac2bc441c0f6d000eabe1753ed477e1064f6a118dd1` |
| The buzz below | 13 632 | `02c74414c9ac4e7a4180a032de47c27634d14f41983af170d484e2d9bbcd1579` |

```lua
local click = { gain = 0.5, layers = {
	{ freq = { 950, 420, time = 0.03 }, env = { attack = 0.0005, decay = 0.045 } },
	{ freq = { 2400, 1800, time = 0.012 }, env = { attack = 0, decay = 0.015 }, gain = 0.15 },
	{ wave = "noise", seed = 1, filter = { type = "bandpass", freq = 3800, q = 1.2 },
	  env = { attack = 0, decay = 0.012 }, gain = 0.35 },
} }

local function bell(note, at, pan, gain)
	return { freq = note, fm = { ratio = 3.5, index = { 2.6, 0.05, time = 0.35 } },
	         env = { attack = 0.002, decay = 0.9 }, at = at, pan = pan, gain = gain }
end
local chime = { gain = 0.4, length = 1.6,
	layers = { bell("E6", 0, -0.3, 1), bell("G#6", 0.075, 0.3, 0.85),
	           { freq = "E7", detune = 7, env = { attack = 0.004, decay = 0.6 }, gain = 0.2 } },
	effects = { { type = "reverb", size = 0.55, damp = 0.3, mix = 0.3 } } }

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
local buzz = { gain = 0.42, layers = { a1, a2, b1, b2 },
	effects = { { type = "drive", amount = 0.25 } } }
```

The renderer these were made with is `theming-work/media/synth/src/lib.rs`
(not in the repository; 480 lines, no dependency but `crmath`). It is the
reference: port it rather than re-derive it. The hashes come from recipes
written as Rust values there (`src/main.rs`); the Luau above is the same
three recipes written out again by hand, and has not itself been run. If
the first implementation disagrees with a hash, compare the recipe it
read with the Rust one before suspecting the arithmetic.

**Cost.** The ten example sounds render in 0.2 to 10 ms each. The most a
recipe may ask for (eight 4 s layers with FM, vibrato, tremolo and a
gliding filter, and four effects) took 298 ms: 9.3 ms per layer-second.
With 64 layer-seconds and 43 s of sound per theme, rendering everything a
theme may declare stays near one second, on the sound thread.

### Sound files

`hound` for WAV, `lewton` for Ogg Vorbis; sniffed by `RIFF….WAVE` and
`OggS`. Refuse more than 2 channels, a rate outside 8 to 96 kHz, or more
than 4 s. Mono is copied to both channels. Other rates are resampled once
to 48 kHz with a band-limited (windowed-sinc, at least 16 taps) resampler.
With `gain` the result is scaled to that peak. Decoding runs on the sound
thread under `catch_unwind`. Measured for a 1.37 s stereo file: WAV
1.7 ms, Vorbis 3.5 ms.

`hound`, `lewton` and `claxon` (FLAC, deferred) together add 248 KiB;
symphonia with the same formats adds 741 KiB.

### The mixer and the stream

**The back-end** is PulseAudio's native protocol over its unix socket,
spoken by the pure-Rust `pulseaudio` crate (0.3). PipeWire's
`pipewire-pulse` and PulseAudio itself serve it; it is what a Flatpak's
`--socket=pulseaudio` grants.

- The socket: `$PULSE_SERVER` if it starts with `unix:`; else
  `$PULSE_RUNTIME_PATH/native`; else `$XDG_RUNTIME_DIR/pulse/native`. A
  `PULSE_SERVER` of any other kind is not followed (no TCP).
- **The cookie is always 256 bytes**: `$PULSE_COOKIE`'s file, else
  `~/.config/pulse/cookie`, else 256 zeros. `pipewire-pulse` answers a
  shorter one with `Invalid` (found the hard way: `Client::from_env`
  sends none when there is no file).
- One stream: signed 16-bit little-endian, stereo, 48 000 Hz;
  `media.role = "event"`, `media.name = "Theme sounds"`, the app's name,
  id and icon; `target_length` 20 ms, `minimum_request_length` 5 ms,
  `pre_buffering` 0, `adjust_latency`; created corked.

**Corked when silent.** An uncorked stream is asked for data every
quantum whether or not there is any: measured, 187 wake-ups a second and
4.4% of a core fed with silence, against 0 wake-ups and 0.002% corked.

| Moment | The sound thread |
|---|---|
| A play arrives while corked | Uncorks (a round trip: 0.4 ms median, 3.7 ms worst), then mixes. |
| While any voice sounds, or a delayed one is waiting | Answers each request with mixed audio. |
| The last voice has ended | Feeds silence for 100 ms (so what was written is heard), then corks and **flushes**. |
| Corked | Nothing. The thread sleeps. |

The flush matters: a stream corked with silence still queued plays that
silence first on the next uncork, in front of the click.

The start-up case was measured by itself: a stream created corked, left
alone for 400 ms, then its first play. The uncork took 0.24 ms (median)
and the sound was on the null sink's monitor within 19.8 ms (median) and
20.6 ms (the worst of 40).

Do not leave an uncorked stream unfed "to save wake-ups". With
`pre_buffering = 0` the server's read position moves on through the
underrun, and the next write lands behind it: in a trial, 263 plays gave
37 audible bursts, most of them cut to a few milliseconds.

**A play** from the interface's thread is one push onto a bounded queue
(64; full: the play is dropped) and one wake. It never waits.

**The mixer**, in 32-bit floats:

- A voice is a sound, a position, a step (`rate`), two gains
  (`volume × √2 cos θ`, `volume × √2 sin θ`, `θ = (pan + 1) × π / 4`), and a
  start delay. Reading between samples is linear interpolation.
- 12 voices. A thirteenth takes the place of the oldest, which fades out
  over 240 samples first.
- The sum is multiplied by the master gain, the Settings volume squared,
  then limited: `x` where `|x| ≤ 0.75`, else
  `sign(x) × (0.75 + 0.25 × tanh((|x| − 0.75) / 0.25))`.
- Stopping (unload, the switch going off): every voice fades over 1440
  samples (30 ms); then cork, flush, close.

**When there is nothing to talk to.** No socket, a refused connection or
a stream that fails: the sound side is "unavailable". Plays are dropped
before they are queued. One line goes to stderr per change of state, and
Settings shows "No sound system found". It tries again when a theme is
loaded, when the switch is turned on, and at most every 5 s on a play.
Nothing panics, nothing retries in a loop, nothing reaches the theme.

**Threads**: at most two (the client's reactor, and the thread that
renders, decodes and corks). Measured: one extra thread and 1.5 MiB of
resident memory with the stream open.

**As built** (`crates/gmnb-sound`). What differs from the prototype the
numbers above came from, and what the engine measured in the same kind
of sandbox (a private PipeWire 1.6.11 with one null sink, quantum 256, at
nice 15 under a CPU cap; `examples/latency.rs`, which refuses to run
anywhere else):

- **A corked stream is given nothing.** The prototype's source answered
  every request, so a stream created corked was handed silence that the
  first sound then queued behind. The engine's source answers only while
  the stream is in use: corked, it leaves the server's request open
  (`Poll::Pending`), and the sound thread wakes the reactor when a play
  arrives. The request to start goes out behind the first frames, not
  ahead of them. A corked stream is asked 0 times a second, and the
  request after each flush stays the same size (3072 bytes here).
- **Every wait on the server is bounded** (2 s; the greeting's blocking
  reads by a socket timeout). A server that stops answering leaves sound
  "unavailable"; dropping the engine joins its thread, and if that takes
  more than 5 s leaves it behind with a line on stderr.
- **A play that waited** more than 50 ms past its own delay in the queue
  (the sound thread was rendering) is dropped, not played late.
- `PULSE_SERVER` of another kind than `unix:` is passed over, and the
  next place is tried.
- **A wake-up is never used up by a wait on the server.** The sound
  thread sleeps by parking, in its own loop and while it waits for the
  server's answer. A play (or being told to stop) that arrived during a
  cork had its wake-up spent there and then sat in the queue; each such
  wait now leaves a wake-up behind. `tests/server.rs` holds this with a
  server that takes 20 ms to answer.
- **What is tested where.** `cargo test` never meets a sound server: the
  null sink, a socket nobody listens on, and a stand-in server in the
  test itself (`tests/server.rs`: created corked, nothing while corked,
  sound in the stream's first audio, cork then flush 100 ms after the last
  voice, a server that goes away and comes back). Against the sandbox's
  real `pipewire-pulse` 1.6.11: `examples/latency.rs` (the table below)
  and `examples/lifecycle.rs` (the session's server found from the
  environment; the server stopped while the stream rests and in the
  middle of a sound, each time "unavailable" within 12 ms, one line, and
  0 wake-ups afterwards; started again, found by the next play after the
  retry interval; and no server at all, the chapter's test 3). PulseAudio's
  own daemon was not tried.

| | Measured |
|---|---|
| Theme loaded → stream ready (connect, create corked) | p50 9.7, p95 10.8, max 11.8 ms |
| The `play` call | p50 0.01, max 0.05 ms (80 ns in the null sink's test) |
| First play after 400 ms corked → handed to the server | p50 0.15, p95 0.22, max 0.61 ms |
| First play after 400 ms corked → on the sink | p50 4.2, p95 5.1, max 9.3 ms |
| Play from corked → handed to the server | p50 0.15, p95 0.25, max 0.66 ms |
| Play from corked → on the sink | p50 3.9, p95 6.4, max 6.8 ms |
| Play, stream running → handed to the server | p50 2.8, p95 5.1, max 5.7 ms |
| Play, stream running → on the sink | p50 18.8, p95 21.1, max 21.8 ms |
| At rest, corked | 0 requests and 0 wake-ups of the sound thread a second; 0.002 to 0.003% of one core |
| With a theme loaded | +2 threads, +2.6 MiB resident (768 KiB of it the test's 4 s sound) |
| Unload while sounding | the call 0.008 ms; both threads gone after 56 ms |

A running stream holds its 15 ms of silence in front of a new sound; a
corked one holds nothing, which is why the first is slower than the
second. These are from one run; a second gave the same figures for plays
(medians within 0.3 ms). Its "stream ready" was 54 ms, measured while
another job had the machine's processor cap saturated; a third, with the
machine quieter, 10.6 ms.

**Wired into the app** (a debug build on the private headless compositor,
at nice 15 under a CPU cap, the window taken to be in use). Aurora with its
sounds, settled (45 s after the last thing that happened): 0 clock ticks
of processor time in 10 s, before the wiring and after it, with a key
played on the null sink; its sound thread 0 wake-ups. On a stand-in
server, once the stream has corked 100 ms after a key's sound: the two
sound threads (ours and the client's) 0 ns and 0 wake-ups in 5 s, and the
server asked for nothing (`tests/sound.rs`). A press to its plays on the
interface's thread: 0.01 to 0.05 ms in the running app, Aurora's `sound`
function included (`GMNB_SOUND_TRACE`); p50 0.009 ms, max 0.02 ms in
`cargo test`; and from the press to its sound at the stand-in server,
p50 4.2 ms, max 5.0 ms (the stand-in asks every 5 ms). The libraries GMNB
links (`readelf -d`) are the same 16 as before.

### From event to sound

For each `press`, `result`, `error`, `mode` and `option` event, after the
layers have been given it (`Layers::emit` tells its listener, the hub's
`hear_event`, in the window's coordinates; only while a theme with sounds
is loaded and sound is on, so that the base look does nothing more for
it):

1. Stop if `via` is `"assistive"` (a press, or the result or error such a
   press brought about), the window is not active (`GMNB_ACTIVE` takes it
   to be), the window has been on screen for less than 500 ms, or sound is
   off or unavailable.
2. If the theme has `sound` and it is not switched off: call it (2 ms)
   with the event and the whole `ctx` (`Theme::sound_choice`). `false`:
   stop. A sound, a play or a list: check it and go to 5. `nil` or a
   failure: go on. A sound it made in the call is handed to the engine
   (`SoundEngine::extend`); a failure is a note, and the one that switches
   it off a toast, once.
3. Find the event's name: `mode` is itself; `error` and `result` are
   themselves on the Calculator page, `result` only when its `cause` is
   `"equals"` or `"function"`, and they have none otherwise; `option` has
   none. A press
   with `action` `"clear"`, `"backspace"` or `"memory"` has that name; a
   press on a key is `digit`, `operator`, `func`, `equals` or `toggle` by
   `key.kind` (`"other"` is `func`); any other press is `button`.
4. Walk the name's row in the table of names until an entry is found:
   `false` stops; an entry is resolved (`pick`, ranges) to plays.
5. Drop the event if a play for the same name started less than 30 ms
   ago. Else queue the plays whose sounds are rendered; one that is not
   yet is skipped, never played late.

`pick`: `"cycle"` keeps a counter per entry; `"random"` uses a 32-bit
xorshift seeded once per theme load; `"key"` hashes `key.id` (events
without a key cycle). A range under `"key"` is `low + (high − low) ×
(hash / 2³²)`, with a different hash per field.
`pan = "position"` is `(2x / width − 1) × 0.6`.

Steps 1 to 4 but the window's are `gmnb_theme::sound::decide`; the app's
`Voice::hear` (`apps/gmnb/src/sound.rs`) calls it, then applies step 5,
and so does the render example's stand-in for the app
(`crates/gmnb-sound/examples/support/stand_in.rs`), so that what is
rendered and tested of a theme is what the app plays.

Sound events do not depend on layers. In particular they are produced
under reduced motion, where layers receive no events.

**A result's cause.** Every way of pressing a key tells the layers of the
press before the calculator is pressed (the pointer's `on_key`, the
keyboard's `flash` then `vm.press`), and the readout tells them of what
came of it in the same handler. So a `result` or an `error` takes its
`cause` and its `via` from the press told less than 250 ms before it
(`Layers::outcome`); with none so recent, it has neither (a paste, a unit
chosen, the rates arriving).

**Telling an assistive press.** Every way of operating a key ends in the
button's `clicked`. The keypad records, in capture-phase controllers that
claim nothing, each pointer press on it (`last_press`) and each Space or
Enter on a focused key (`last_activation`): a `clicked` within a second
of a pointer press inside the key is `"pointer"`, else within a second of
Space or Enter `"keyboard"`, else `"assistive"` (AT-SPI's "click" is the
widget's `activate`, which presses nothing first). A key typed on the
keyboard that presses its key on screen is always `"keyboard"`. Other
controls are told apart the same way by what the window's backdrop
records for the whole window, and a popover, whose grab keeps the pointer
and the keys from the window's controllers, records its own
(`Layers::watch`: the date picker's). Where the app cannot tell (a
sidebar row the toolkit activates by itself, and the `mode` that follows)
there is no `via`, and it sounds: the rule fails towards sound, not
towards silence.

### Settings

Settings → Appearance gains a group, **Sound**, between the theme's
options and Window:

| Row | Kind | Default |
|---|---|---|
| Theme sounds | Switch. Subtitle: "‘Name’ has sounds", "This theme has no sounds", "Turned off by the desktop's sound settings", "No sound system found", or for a run with `GMNB_SOUND=0` "Turned off for this run (GMNB_SOUND=0)". It follows the sound thread's finding at once (`Settings::on_status`, handed to the main loop). | on |
| Volume | Slider, 0 to 100%, in steps of 5. Let go (the pointer or a touch lifted, or a key that moves it released) it plays the theme's `digit` sound, at the new volume. Insensitive while the switch is off. | 50% |

Saved as `"sound": true` and `"sound_volume": 0.5` in `settings.json`,
each loading by itself like the other keys. They belong to the app, not
to a theme.

**Why on.** A sound exists only after the user has chosen a theme that
has one; the base look is silent. The switch sits directly under the list
they chose it from. The desktop's own switch is honoured: the
`gtk-enable-event-sounds` property of `GtkSettings`, followed as it
changes. GTK takes it from `settings.ini` (tested: with
`gtk-enable-event-sounds=false` in `gtk-4.0/settings.ini` a theme with
sounds loads and nothing is created, `tests/sound.rs`); that it also
follows the desktop's own setting (the settings portal, GSettings'
`event-sounds`) on each desktop and inside a Flatpak could not be checked
in the isolated test session and is still to be confirmed. Off by default
would hide the feature from the people who picked a theme for it. **Why 50%.** The gain is the slider
squared, so 50% is −12 dB. A sound's loudest sample is its `gain`, 0.5 at
most in the built-in themes, so at the default a sound in the middle peaks
at −18 dBFS at most, and one panned hard to a side 3 dB above that (the
mixer's pan keeps a sound's power, not its peak): present on speakers,
well under any music playing.

### Environment hooks

| Variable | Effect |
|---|---|
| `GMNB_SOUND=0` | No sound for this run, whatever Settings says. Nothing sound-related is created. |
| `GMNB_SOUND=null` | The mixer runs into a sink that discards its output, with no server. For tests: voices and plays can be counted. |
| `GMNB_SOUND_DUMP=<folder>` | Every sound the theme makes is written there as a 16-bit WAV, named by its order and a hash of its recipe or file name. |
| `GMNB_SCREENSHOT` (existing) | Also: no sound (Settings still says what it would be); and the shot waits for no animation frame (animations show frame 1). |
| `GMNB_SOUND_TRACE=1` | A line on stderr for each event sound hears: the event, its name, and what it played or why nothing (`sound: press (keyboard) key 137: digit: #1 volume 1.00 rate 0.969 pan -0.44 delay 0.00 Queued [0.012 ms]`). |
| `GMNB_THEME_STATS` (existing) | Also a line for sound: the engine's state, whether its thread exists, the connections to a sound server attempted, plays queued, started and skipped. |
| `GMNB_STEPS` | Dev/test hook: `keys:<script>` (typed, as `GMNB_KEYS`), `click:<id>` (a key clicked by the pointer at its middle), `activate:<id>` (activated as assistive technology does), `wait:<ms>`, `quit` (the sound line of `GMNB_THEME_STATS` written, and the window closed), separated by `;`, one every 250 ms from 1 s after the window is on screen. The app's sound test drives the window with it. |

### Where the new events come from

| In the app | Event |
|---|---|
| C, CE | `press` with `action = "clear"` (the graphing keypad has no clear) |
| Backspace (the key, the keyboard's, and the graphing keypad's ⌫) | `press` with `action = "backspace"` |
| The memory row; MC, M+, M− on a stored value | `press`, `source = "button"`, `action = "memory"` |
| A control activated through AT-SPI's action interface | `press` with `via = "assistive"` |
| A result or an error | `cause` and `via` of the press just before it (`Layers::outcome`) |
| A sidebar row chosen; the calculator's mode changed, by pointer or shortcut | `mode`, `page` = the new page, `rect` = the page |

### Tests

1. **Synth determinism**: the three test vectors above, hashed. They run
   in `cargo test` on every platform CI has.
2. **Synth limits**: recipes at and past each cap (layers, effects,
   length, layer-seconds) give the listed result; no recipe panics or
   allocates past the caps (fuzz the recipe table: random keys, types and
   magnitudes, NaN and infinity included).
3. **No device**: with `XDG_RUNTIME_DIR` pointing at an empty folder,
   loading a theme with sounds and playing 100 events gives silence, one
   stderr line, and no thread left spinning (CPU time of the process over
   2 s under 5 ms).
4. **Caps under load** (`GMNB_SOUND=null`): 100 plays at once leave 12
   voices; the same event twice within 30 ms plays once; a full queue
   drops.
5. **At rest**: with a theme that has sounds and an animation loaded, and
   the window settled, the process makes no wake-up from the sound or
   media threads (their CPU time does not grow over 5 s).
6. **Nothing in the base look**: without a theme, no media or sound
   thread exists and no connection to a sound server has been attempted
   (a counter in `gmnb-sound`); `readelf -d` of the release binary lists
   the same libraries as before this chapter.
7. **Decoders against bad files**: truncated, oversized and mislabelled
   files of each kind are refused with a note; a corpus of malformed
   files never panics past `catch_unwind` and never allocates past the
   image total.
8. **Animation**: for one GIF, one APNG and one animated WebP with known
   frames, frame `i` composited by the player equals the reference
   picture (disposal and blending included); a delay of 0 plays as
   100 ms; the loop wraps to frame 1.
9. **Field**: the pass on a fixed field and ramp equals a stored picture
   byte for byte; `quality` and the pixel cap give the sizes of step 1.
10. **Sound and screen readers**: a press delivered with
    `via = "assistive"` reaches layers and makes no play, and neither does
    the result it brings about.
11. **DGMNB**: the widened `cargo tree` check.
12. **A built-in theme's sounds** (`crates/gmnb-theme/tests/aurora_sound.rs`
    for Aurora, `glazed_sound.rs` for Glazed, `swirls_sound.rs` for Swirls):
    they load with no note, each within the gain a built-in theme may have;
    every event plays what the theme means it to, in every palette (Glazed:
    in every flavour, whose notes are the same sounds at other speeds, and a
    slower sound is a longer one), and nothing lasts longer than the budget of
    [Designing sounds](#designing-sounds-a-few-rules-that-work) allows;
    its `sound` function answers some 9000 events with no failure in the
    2 ms it has (processor time: 14 µs a call measured, the stand-in's own
    work included); and what bounds the making of its sounds (layer-seconds,
    no reverb or delay without a `length`, the bytes) stays under a fifth
    of what a theme may ask for. The events are made by the stand-in for
    the app that the render example uses
    (`crates/gmnb-sound/examples/support/stand_in.rs`), and decided by the
    app's own code (`gmnb_theme::sound::decide`). The levels are measured
    outside `cargo test`, by `tools/theming/sound_check.py` on the theme's
    render.
13. **From event to sound in the app** (`apps/gmnb/src/sound.rs`, plain
    `cargo test`, no display): a theme's sounds by event (a typed key as
    the clicked one, the running total silent, the 30 ms rule, the volume's
    preview); its `sound` first, a sound made in it played once rendered,
    three failures and it is asked no more; with a stand-in server, the
    base look, a theme without sounds, the switch off and the desktop's
    event sounds off making no thread and no connection, and a theme with
    sounds one of each; no sound system, and Settings saying so; a press
    to its play (p50 0.009 ms, max 0.02) and to the stand-in server
    (p50 4.2 ms, max 5.0: the stand-in asks every 5 ms).
14. **The app itself** (`apps/gmnb/tests/sound.rs`, on the test display:
    skipped without `GMNB_TEST_WAYLAND_DISPLAY` or
    `DGMNB_TEST_WAYLAND_DISPLAY`): GMNB run with `GMNB_STEPS` and
    `GMNB_SOUND_TRACE`, in a runtime folder of its own with no `PULSE_*`
    and no session bus. With the null sink: a digit typed and clicked, =
    and its result, an error, a change of mode, each the designed sound; an
    assistive press and the result after it, nothing. With a stand-in
    server in that runtime folder: the base look, the switch off
    (`settings.json`) and the desktop's event sounds off
    (`gtk-4.0/settings.ini`) connect to nothing; a theme with sounds
    connects once, to it (so to no other), and its sound arrives; and at
    rest (Aurora, a key played and the stream corked) its two threads do
    not run once in 5 s and the server is asked for nothing.

**The pictures' tests, as built** (5 to 9). Ten small files by other
programs' encoders are in `crates/gmnb-media/tests/fixtures`, 2 KiB in
all (a JPEG and a lossy WebP can't be written by the tests at all);
everything else is written by the tests, chunk by chunk, so that every
field of every frame is theirs to choose:

| Where | What |
|---|---|
| `gmnb-media` `tests/stills.rs` | Each decoder with good files of every kind; truncated at every length, the wrong first byte, a header claiming 60 000×60 000 (each format), a GIF frame claiming it inside a small canvas, a WebP whose bitstream claims another size than its header, 4 400 files with a few bytes changed; a decoder that panics |
| `gmnb-media` `tests/animations.rs` | Frame `i` against hand-written pictures for a GIF, an animated PNG (with and without a first image that is no frame) and an animated WebP, every disposal and blend; delays of 0; the loop; a GIF of 3 000 one-pixel frames; files that announce other than they hold; 1 500 animations with bytes changed. The player: order, the loop, 0 wake-ups at rest (counted), no thread under reduced motion or for a still, the thread gone with its player, a quarter of a core (processor time against the clock), too heavy, a source that fails or panics |
| `gmnb-media` `tests/memory.rs` | Under a counting allocator: what playing each format holds against what is charged; what decoding a still holds; nothing allocated for a refused header; malformed files under a cap |
| `gmnb-media` `tests/field.rs` | The pass against the steps written plainly, byte for byte; a stored hash; the swirl against the shader, and against colours computed coarsely; the cost (ignored: a measurement) |
| `gmnb-theme` `tests/media.rs` | The same through a theme: the four kinds of still; hostile files through the draw path; every cap of the limits table hit on purpose; an animation's frames, loop and next frame under a clock the test holds; reduced motion, previews, tiles; "plays slower"; `gmnb.pixels` and its allowance; ramps; a field written by Luau hashing to the stored value; the allowance, the owed frame, the unchanged field; the swirl demonstration under the real limits |
| `apps/gmnb` `layers::tests::a_themes_pictures_render` | A still, a tile, a filled picture, an animation's frame and a field through the toolkit's software renderer, colours checked at points, the toolkit's criticals fatal, no media thread started |
| `apps/gmnb` `theme::tests::theming_installs_no_sources` | The one place the media thread reaches the main loop (`wake`) |
| The running app (`GMNB_THEME_STATS`) | Media threads ever started (0 for the base look and Mono), the thread's wake-ups, frames decoded |

Test 7 as first written said a malformed file "never allocates past the
image total". What holds, counted: a refused header allocates nothing;
a decoder holds at most the picture it was allowed and its working copy;
and a spoiled lossy WebP can make its decoder ask for up to 16 MiB more
that it never touches (see "Decoding stills").

### Packaging

No package gains a build or run-time dependency: everything added is a
Rust crate. The package names below were confirmed in the distributions'
package indexes when the packaging was edited (Debian trixie:
`pipewire-pulse` 1.4.2, `pulseaudio` 17.0; Fedora 43 to Rawhide:
`pipewire-pulseaudio`; Arch extra: `pipewire-pulse`, `pulseaudio`).

| Package | Change |
|---|---|
| All | `Cargo.lock`; `packaging/flatpak/cargo-sources.json` (`nix run .#update-cargo-sources`); `THIRD-PARTY-LICENSES.txt` (`packaging/licences/generate.sh`). The new crates are MIT, Apache-2.0, Zlib or BSD-3-Clause (`ogg`), all already accepted in `about.toml`. |
| Flatpak, GMNB only | `finish-args` gains `--socket=pulseaudio`. DGMNB's manifest is untouched. |
| Debian 13 | `gmnb`: `Suggests: pipewire-pulse \| pulseaudio`. No `Build-Depends` change. |
| Fedora | `gmnb`: `Suggests: pipewire-pulseaudio`. No `BuildRequires` change. |
| Arch | `gmnb`: `optdepends=('pipewire-pulse: theme sounds' 'pulseaudio: theme sounds')`. |
| Nix | Nothing. (`nix/package.nix` has no sound input to add; the socket is found at run time.) |

**What `--socket=pulseaudio` exposes.** The sandbox gets the host's
PulseAudio socket (and, as far as is known, `/dev/snd` with it since
Flatpak 1.8: to be confirmed). That is the whole protocol, not only
playback: it also has recording, monitor sources that carry what other
applications play, and commands that list and adjust streams. (The
latency test here recorded the sink's monitor through the same kind of
socket.) Flatpak has no playback-only permission, and whether a sound
server holds a sandboxed client to less is that server's policy, which
was not tested. GMNB's client opens one playback stream and nothing else,
and a theme cannot reach the socket: scripts have no I/O. The permission
is still a real widening of what a compromised GMNB could do, and it is
the price of sound in the Flatpak. It is why the request is GMNB's alone.

**If video files were added** it would need, besides the code: Debian
`libgtk-4-media-gstreamer`, `gstreamer1.0-plugins-base` and `-good`;
Fedora `gstreamer1-plugins-base` and `-good`; Arch `gst-plugins-base` and
`-good`; Nix a wrapper setting `GST_PLUGIN_SYSTEM_PATH_1_0`; nothing in
the Flatpak (the runtime's base, checked in freedesktop 26.08, has 227
GStreamer plug-ins, matroska, vpx and dav1d among them).

### Why this sound stack

All latencies were measured on the development machine against a private
PipeWire 1.6.11 with one null sink (quantum 256 at 48 kHz), in a sandbox
with no sound hardware and no path to the user's sound server, at nice 15
under a CPU cap while the machine was busy. "To the sink" is the moment
the sound appeared on the null sink's monitor, read back through a 5 ms
record stream: an upper bound, since it includes that capture.

| | Pure-Rust PulseAudio client (**chosen**) | cpal 0.18 (ALSA host, through PipeWire's ALSA plug-in) |
|---|---|---|
| Play → handed to the server, stream running | p50 2.9, p95 5.1, max 11.9 ms | p50 5.3, p95 10.3, max 30.0 ms |
| Play → handed over, from corked / paused | p50 4.3, p95 7.0, max 9.5 ms | p50 3.3, p95 5.5, max 18.0 ms |
| Play → on the sink, stream running | p50 19.3, p95 21.5, max 52.3 ms | p50 21.8, p95 41.4, max 57.1 ms |
| Play → on the sink, from corked / paused | p50 20.0, p95 22.5, max 25.3 ms | p50 19.4, p95 21.7, max 27.7 ms |
| Play → on the sink, nothing open | p50 31.6, p95 37.8, max 41.0 ms | p50 19.4, p95 30.2, max 31.9 ms |
| Idle, corked / paused | 0 wake-ups, 0.002% of a core | 0 wake-ups, 0.002% |
| Resident memory with the stream open | +1.5 MiB | +6.3 MiB |
| Threads | +1 | +3 |
| Stripped binary | +440 KiB, 27 more crates | +80 KiB, 6 more crates |
| Native library | none | `libasound.so.2`, loaded at every start, sound or no sound |
| Build needs | nothing new | ALSA headers and pkg-config in all five package builds |
| Mixing, pitch, pan | ours | ours |
| No sound server and no sound hardware (run) | No socket: gives up in 3 ms, nothing on stderr, clean exit | `snd_pcm_open` fails ("Host is down") after 22 ms, nothing on stderr, clean exit |
| Flatpak | `--socket=pulseaudio`, directly | The same permission, through the runtime's ALSA-to-pulse plug-in |
| Without the PulseAudio protocol (bare ALSA, JACK) | Silent | Works |

These are single runs on a busy machine, and the tails move between runs:
a second run of the chosen client gave 21.0 ms (p95) with the stream
running and 51.9 ms (p95) from corked. Medians held within 2 ms.

Both meet "under about 30 ms from event to buffer submission" with room.
The pure-Rust client wins on the constraints that are not about speed:
nothing new is loaded in the base look, no package build changes, a
quarter of the memory and a third of the threads. It costs 360 KiB more
binary and rests on a younger crate; cpal is the fallback if that crate
disappoints, and cpal 0.18 itself offers a `pulseaudio` host built on it.

The others, not prototyped:

| Candidate | Why not |
|---|---|
| rodio | cpal plus a mixer and decoders: cpal's costs, and a mixer the contract would have to re-specify anyway. |
| GStreamer (`GtkMediaFile` per sound, or `appsrc`) | The video trial's costs (start-up of 0.3 s and more, +102 MiB, +31 threads) for a click; system decoders on theme files; a pipeline per sound has no shared mixer. |
| GSound / libcanberra | A new native library on every distribution; plays files and named desktop sounds, so synthesised sounds would go through temporary files; volume only, no pitch or pan; one server-side stream per play. |
| pipewire-rs | Needs bindgen (libclang) and PipeWire's headers in every build (as its build is known; it was not built here); PipeWire only; a Flatpak would need the PipeWire socket itself. |
| libpulse-simple | `libpulse.so.0` and its dozen dependencies loaded at every start; otherwise the chosen design with C underneath. |

### Open points

Decided here, and worth a second look by the owner:

1. **Sound is on by default**, at 50% (−12 dB).
2. **A press through the accessibility interface makes no sound.**
3. **No ALSA fallback.** A system with neither PipeWire's nor
   PulseAudio's server gets no theme sounds.
4. **The Flatpak asks for `--socket=pulseaudio`**, which is more than
   playback.
5. **Video files wait.** Animated WebP stands in for them.
6. **`gmnb.image` no longer goes through GTK's loaders.** Themes see no
   difference; the code behind it does.
7. **The effects' defaults** (delay 0.25 s, feedback 0.35, mix 0.3; reverb
   size 0.5, damp 0.5, mix 0.3; drive 0.5; crush 8 bits at 12 000) were
   chosen while building the synthesiser.
8. **The engine connects to the sound server when a theme with sounds is
   loaded**, not at its first sound, so that Settings can say early that no
   sound system was found.
9. **The `sounds` table plays `result` only after `=` or a function
   key** (`cause` `"equals"` or `"function"`): not for the running total
   an operator shows, a value a digit or a memory button puts there, nor a
   paste. `error` it plays whatever brought it about: an error is always
   news. `sound` hears them all.
10. **A result or an error is the doing of the press told less than
    250 ms before it** (its `cause` and `via`). Both are told in one
    handler; the window is generous, and a result long after a press (a
    paste a second later) has neither.
11. **What follows an assistive press is silent too**: its `via` is
    carried to the result or error it brings, which layers also see.
12. **`gtk-enable-event-sounds` from the desktop** is honoured as GTK
    reports it; that GTK fills it from each desktop's own setting, and
    inside a Flatpak, is not confirmed (only `settings.ini` was tried).
13. **`GMNB_SOUND=0`** says so under *Theme sounds* ("Turned off for this
    run"), a subtitle the table above did not have.
14. **No time is given back to a theme** (`Engine::credit` was not built):
    pictures are decoded outside the theme's calls, and what is left
    inside them (reading a file) counts.
15. **An animation counts as 4¼ to 6 of its frames, by format**, not 4:
    that is what playing one was counted to hold. The animated PNG's 6 is
    the `png` crate keeping a frame and a half of buffers between frames.
16. **`image-webp` 0.2.4 is kept at arm's length.** It composites no
    animation here, and sees no bitstream whose own size wasn't checked
    first. What it can still be made to do is ask for 16 MiB it never
    touches, for a moment. A later version may make all of this
    unnecessary; the checks cost nothing to keep.
17. **A picture that changes too often is made good one frame later**
    (the layer is owed a frame), under reduced motion too. Without it a
    still picture could stand out of date.
