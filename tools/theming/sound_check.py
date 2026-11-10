#!/usr/bin/env python3
"""Checks a theme's rendered sounds against the sound chapter's design rules
(docs/theming-media.md, "Designing sounds").

    cargo run --release -p gmnb-sound --example render -- <theme> <folder>
    python3 -I tools/theming/sound_check.py <folder>

It reads what the render example wrote (the WAV files and index.tsv) and
prints, for every sound and for every event as the mixer plays it: its
loudest sample, its loudness, how long it lasts, how much of it lies under
120 Hz and its DC offset. It exits 1 if a rule of the budget below is
broken, 2 if the folder can't be read, else 0. Flags are printed and do
not fail.

Nothing is played, and nothing but Python's own library is used.
"""

import array
import math
import os
import statistics
import sys
import wave

# ---- The budget -----------------------------------------------------------
#
# One budget for every built-in theme. Change a number here and the chapter's
# "Designing sounds" with it.

# FAILS

# A sound's loudest sample, of full scale: its recipe's `gain`. "0.5 at most
# in the built-in themes."
GAIN_MAX = 0.5
# An event's loudest sample as the mixer plays it (panned, and several plays
# together if its `sound` function asks for them): what one sound at 0.5
# reaches when it is panned hard to one side, 0.5 x sqrt 2. The mixer's
# limiter starts just above, at 0.75.
EVENT_PEAK_MAX = 0.5 * math.sqrt(2)

# How long an event may last: from the event until it has fallen 60 dB under
# its own loudest sample, in milliseconds, a `delay` included. The longest
# of the eight takes the render made counts.
#   typed:  what is pressed many times a second. "A key sound over 80 ms
#           smears fast typing."
#   once:   keys and buttons pressed once in a while. They may be a gesture.
#   rings:  "Results can ring for a second; nothing needs more than two."
TYPED_MS = 80
ONCE_MS = 250
RINGS_MS = 2000
LENGTH = {
    "digit": TYPED_MS,
    "operator": TYPED_MS,
    "func": TYPED_MS,
    "backspace": TYPED_MS,
    "equals": ONCE_MS,
    "toggle": ONCE_MS,
    "clear": ONCE_MS,
    "memory": ONCE_MS,
    "button": ONCE_MS,
    "result": RINGS_MS,
    "error": RINGS_MS,
    "mode": RINGS_MS,
}
# A change of one of the theme's options (only a `sound` function hears it).
OPTION_MS = RINGS_MS
# A sound by itself, whatever it is used for.
SOUND_MS = RINGS_MS

# FLAGS

# Loudness this far from the theme's own median, in LU: a sound among the
# sounds, an event among the events. (A chime is rightly louder than a key;
# this is for the one that sticks out or can't be heard.)
SPREAD_LU = 12.0
# Twenty presses at eight a second, short-term loudness in LUFS at full
# volume. -20 is about as loud as speech on the radio.
TYPING_LUFS_MAX = -20.0
# The part of the energy under 120 Hz, in dB relative to the whole. "Leave
# the low end alone": laptop speakers have nothing there and headphones have
# too much.
LOW_DB_MAX = -20.0
# The mean of the samples while the sound lasts, in dBFS.
DC_DBFS_MAX = -40.0

# ---- How it is measured ---------------------------------------------------
#
# peak   The largest sample of either channel, in dB of full scale.
# loud   Loudness as ITU-R BS.1770 measures it (K-weighting, both channels
#        summed), the loudest 400 ms ("momentary"). A sound shorter than that
#        is its energy spread over 400 ms: about how the ear takes a short
#        sound, quieter the shorter it is. Typing is measured over 3 s
#        ("short-term").
# ms     From the start (of the file, or of the take) to the last sample
#        within 60 dB of the peak.
# low    Energy through an eighth-order Butterworth low-pass at 120 Hz, over
#        the energy of all of it, in dB. (Steep, so that a tone at 160 Hz is
#        not taken for one under 120: it reads -20 dB.)
# dc     The larger of the two channels' means over those milliseconds.
#
# An event's file holds eight takes; each is measured by itself and the
# worst of each column is shown.

RATE = 48000
FULL = 32767.0
BLOCK = RATE // 10  # 100 ms: momentary is 4 of these, short-term 30.
SILENT = -120.0

# BS.1770-4's two filters at 48 kHz: the shelf, then the high-pass.
SHELF = (1.53512485958697, -2.69169618940638, 1.19839281085285,
         -1.69065929318241, 0.73248077421585)
HIGH = (1.0, -2.0, 1.0, -1.99004745483398, 0.99007225036621)


def lowpass(freq, q):
    """A low-pass biquad (b0, b1, b2, a1, a2), as the synthesiser's own."""
    w = 2 * math.pi * freq / RATE
    alpha = math.sin(w) / (2 * q)
    c = math.cos(w)
    a0 = 1 + alpha
    return ((1 - c) / 2 / a0, (1 - c) / a0, (1 - c) / 2 / a0, -2 * c / a0, (1 - alpha) / a0)


# Eighth-order Butterworth at 120 Hz: four sections.
LOW = tuple(lowpass(120.0, 0.5 / math.cos(math.pi * k / 16)) for k in (1, 3, 5, 7))


def db(ratio):
    return 10 * math.log10(ratio) if ratio > 0 else SILENT


def cascade(samples, first, second):
    """`samples` through two biquads in a row."""
    b0, b1, b2, a1, a2 = first
    c0, c1, c2, d1, d2 = second
    z1 = z2 = w1 = w2 = 0.0
    out = []
    for x in samples:
        y = b0 * x + z1
        z1 = b1 * x - a1 * y + z2
        z2 = b2 * x - a2 * y
        v = c0 * y + w1
        w1 = c1 * y - d1 * v + w2
        w2 = c2 * y - d2 * v
        out.append(v)
    return out


def energy(samples):
    return sum(x * x for x in samples)


class Measured:
    """One sound, or one take of an event."""

    def __init__(self, left, right):
        self.peak = max(max(map(abs, left), default=0), max(map(abs, right), default=0))
        self.frames = 0
        self.loud = SILENT
        self.low = SILENT
        self.dc = SILENT
        self.blocks = []
        if self.peak == 0:
            return
        floor = self.peak / 1000.0
        last = len(left) - 1
        while abs(left[last]) < floor and abs(right[last]) < floor:
            last -= 1
        self.frames = last + 1
        # Silence after the last sample, for the filters to ring out.
        after = [0.0] * (RATE // 20)
        left = [x / FULL for x in left[:self.frames]] + after
        right = [x / FULL for x in right[:self.frames]] + after
        weighted = cascade(left, SHELF, HIGH), cascade(right, SHELF, HIGH)
        self.blocks = [
            energy(weighted[0][i:i + BLOCK]) + energy(weighted[1][i:i + BLOCK])
            for i in range(0, len(left), BLOCK)
        ]
        self.loud = loudness(self.blocks, 4)
        whole = energy(left) + energy(right)
        under = sum(energy(cascade(cascade(x, LOW[0], LOW[1]), LOW[2], LOW[3]))
                    for x in (left, right))
        self.low = db(under / whole) if whole > 0 else SILENT
        mean = max(abs(sum(left)), abs(sum(right))) / self.frames
        self.dc = 2 * db(mean)

    @property
    def peak_db(self):
        return 2 * db(self.peak / FULL)

    @property
    def ms(self):
        return self.frames * 1000.0 / RATE


def loudness(blocks, span):
    """The loudest `span` blocks in a row, in LUFS."""
    if not blocks:
        return SILENT
    most = max(sum(blocks[i:i + span]) for i in range(max(1, len(blocks) - span + 1)))
    return -0.691 + db(most / (span * BLOCK))


def read(path):
    """A WAV file as the render example writes them: two lists of samples."""
    with wave.open(path, "rb") as f:
        if (f.getnchannels(), f.getsampwidth(), f.getframerate()) != (2, 2, RATE):
            raise ValueError("not 16-bit stereo at 48 kHz")
        samples = array.array("h")
        samples.frombytes(f.readframes(f.getnframes()))
    if sys.byteorder == "big":
        samples.byteswap()
    return list(samples[0::2]), list(samples[1::2])


class Row:
    """One line of a table: a file, and the worst of its takes."""

    def __init__(self, name, takes, limit_ms):
        self.name = name
        self.limit_ms = limit_ms
        self.peak = max(t.peak for t in takes)
        self.peak_db = max(t.peak_db for t in takes)
        self.loud = max(t.loud for t in takes)
        self.ms = max(t.ms for t in takes)
        self.low = max(t.low for t in takes)
        self.dc = max(t.dc for t in takes)
        self.fails = []
        self.flags = []

    def judge(self, peak_max, median):
        if self.peak > round(peak_max * FULL):
            self.fails.append(f"peak {self.peak / FULL:.3f} is over {peak_max:.3f}")
        if self.ms > self.limit_ms:
            self.fails.append(f"{self.ms:.0f} ms is over {self.limit_ms} ms")
        if self.peak == 0:
            self.flags.append("silent")
            return
        if abs(self.loud - median) > SPREAD_LU:
            self.flags.append(f"{self.loud - median:+.1f} LU from the median")
        if self.low > LOW_DB_MAX:
            self.flags.append(f"{self.low:.0f} dB of it is under 120 Hz")
        if self.dc > DC_DBFS_MAX:
            self.flags.append(f"DC offset {self.dc:.0f} dBFS")

    def line(self, width):
        verdict = "ok"
        if self.fails or self.flags:
            verdict = "; ".join([f"FAIL: {f}" for f in self.fails] + [f"flag: {f}" for f in self.flags])
        return (f"  {self.name:{width}}  {self.peak_db:6.1f}  {self.loud:6.1f}  {self.ms:7.0f}"
                f"  {self.limit_ms:5}  {self.low:6.1f}  {self.dc:6.1f}  {verdict}")


def table(title, rows, peak_max):
    if not rows:
        return
    heard = [r.loud for r in rows if r.peak > 0]
    median = statistics.median(heard) if heard else SILENT
    width = max(len(r.name) for r in rows)
    print(f"\n{title} (median loudness {median:.1f} LUFS)")
    print(f"  {'':{width}}    peak    loud       ms    max     low      dc")
    print(f"  {'':{width}}    dBFS    LUFS                     dB    dBFS")
    for r in rows:
        r.judge(peak_max, median)
        print(r.line(width))


def main(folder):
    index = os.path.join(folder, "index.tsv")
    try:
        with open(index, encoding="utf-8") as f:
            lines = [line.rstrip("\n").split("\t") for line in f][1:]
    except OSError as e:
        print(f"sound_check: {e}\n(render the theme first: "
              "cargo run --release -p gmnb-sound --example render -- <theme> <folder>)",
              file=sys.stderr)
        return 2

    sounds, events, typing = [], [], None
    problems = []
    for file, kind, name, _by, takes in lines:
        try:
            left, right = read(os.path.join(folder, file))
        except (OSError, ValueError, EOFError, wave.Error) as e:
            print(f"sound_check: {file}: {e}", file=sys.stderr)
            return 2
        if kind == "sound":
            sounds.append(Row(file, [Measured(left, right)], SOUND_MS))
        elif kind in ("event", "option"):
            limit = OPTION_MS if kind == "option" else LENGTH.get(name)
            if limit is None:
                # Never passed over: an event this script has no rule for.
                problems.append(f"{file}: no length is budgeted for the event {name!r}")
                continue
            starts = [int(s) for s in takes.split(",")] + [len(left)]
            measured = [Measured(left[a:b], right[a:b]) for a, b in zip(starts, starts[1:])]
            events.append(Row(name, measured, limit))
        elif kind == "typing":
            typing = (name, Measured(left, right))
        else:
            problems.append(f"{file}: index.tsv calls it {kind!r}, which is unknown here")

    print(f"{folder}")
    table("Sounds", sounds, GAIN_MAX)
    table("Events, as mixed: the worst of each one's takes", events, EVENT_PEAK_MAX)

    flags = sum(len(r.flags) for r in sounds + events)
    if typing:
        name, m = typing
        short = loudness(m.blocks, 30)
        verdict = "ok"
        if short > TYPING_LUFS_MAX:
            verdict = f"flag: over {TYPING_LUFS_MAX:.0f} LUFS"
            flags += 1
        print(f"\nTyping ({name}): {short:.1f} LUFS short-term, peak {m.peak_db:.1f} dBFS  {verdict}")

    fails = sum(len(r.fails) for r in sounds + events) + len(problems)
    for problem in problems:
        print(f"FAIL: {problem}")
    print(f"\n{fails} failure(s), {flags} flag(s)")
    return 1 if fails else 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
