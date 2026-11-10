#!/usr/bin/env python3
"""Draws a theme's rendered sounds as one picture: a row for every sound,
every event and the typing run, each a waveform over a spectrogram.

    cargo run --release -p gmnb-sound --example render -- <theme> <folder>
    python3 -I tools/theming/sound_sheet.py <folder> <out.png> [--names a,b,c]

`--names` labels the sounds, in the order the theme makes them (the render
knows them only by number).

A row's left picture is the sound itself, or an event's first take, on its
own time scale (its length is written beside it). Its right picture, for an
event, is the whole file: eight takes, or the typing run. Frequency runs
from 100 Hz at the bottom to 16 kHz at the top, an octave every 11 pixels;
brightness is level, 70 dB from black to white-hot. The waveform's frame is
the events' peak budget (0.71 of full scale) and its inner line the sounds'
(0.5).

Nothing is played, and nothing but Python's own library is used. It takes
a minute or two: the transform is written out in Python.
"""

import array
import cmath
import math
import os
import struct
import sys
import wave
import zlib

RATE = 48000
FULL = 32767.0

LABEL_W = 236
FIRST_W = 400
WHOLE_W = 640
GAP = 14
SPEC_H = 80
WAVE_H = 22
ROW_H = SPEC_H + WAVE_H + 22
MARGIN = 12
F_LOW, F_HIGH = 100.0, 16000.0
DB_FLOOR, DB_TOP = -84.0, -14.0

BACK = (16, 16, 22)
INK = (232, 232, 240)
DIM = (120, 122, 140)
LINE = (58, 60, 76)

# A 5 x 7 font, ASCII 32 to 126: five columns each, the low bit at the top.
FONT = bytes.fromhex(
    "0000000000" "00005f0000" "0007000700" "147f147f14" "242a7f2a12" "2313086462"
    "3649552250" "0005030000" "001c224100" "0041221c00" "082a1c2a08" "08083e0808"
    "0050300000" "0808080808" "0060600000" "2010080402" "3e5149453e" "00427f4000"
    "4261514946" "2141454b31" "1814127f10" "2745454539" "3c4a494930" "0171090503"
    "3649494936" "064949291e" "0036360000" "0056360000" "0008142241" "1414141414"
    "4122140800" "0201510906" "324979413e" "7e1111117e" "7f49494936" "3e41414122"
    "7f4141221c" "7f49494941" "7f09090101" "3e41415132" "7f0808087f" "00417f4100"
    "2040413f01" "7f08142241" "7f40404040" "7f0204027f" "7f0408107f" "3e4141413e"
    "7f09090906" "3e4151215e" "7f09192946" "4649494931" "01017f0101" "3f4040403f"
    "1f2040201f" "7f2018207f" "6314081463" "0304780403" "6151494543" "00007f4141"
    "0204081020" "41417f0000" "0402010204" "4040404040" "0001020400" "2054545478"
    "7f48444438" "3844444420" "384444487f" "3854545418" "087e090102" "081454543c"
    "7f08040478" "00447d4000" "2040443d00" "007f102844" "00417f4000" "7c04180478"
    "7c08040478" "3844444438" "7c14141408" "081414187c" "7c08040408" "4854545420"
    "043f444020" "3c4040207c" "1c2040201c" "3c4030403c" "4428102844" "0c5050503c"
    "4464544c44" "0008364100" "00007f0000" "0041360800" "08082a1c08"
)

# Black, indigo, magenta, orange, pale yellow.
HEAT = [(0, 0, 4), (40, 11, 84), (101, 21, 110), (159, 42, 99), (212, 72, 66),
        (245, 125, 21), (250, 193, 39), (252, 255, 164)]


def heat(t):
    t = min(max(t, 0.0), 1.0) * (len(HEAT) - 1)
    i = min(int(t), len(HEAT) - 2)
    f = t - i
    a, b = HEAT[i], HEAT[i + 1]
    return (int(a[0] + (b[0] - a[0]) * f), int(a[1] + (b[1] - a[1]) * f),
            int(a[2] + (b[2] - a[2]) * f))


class Picture:
    def __init__(self, width, height):
        self.width, self.height = width, height
        self.rows = [bytearray(BACK * width) for _ in range(height)]

    def dot(self, x, y, color):
        if 0 <= x < self.width and 0 <= y < self.height:
            self.rows[y][3 * x:3 * x + 3] = bytes(color)

    def box(self, x, y, w, h, color):
        for yy in range(max(y, 0), min(y + h, self.height)):
            a, b = max(x, 0), min(x + w, self.width)
            if b > a:
                self.rows[yy][3 * a:3 * b] = bytes(color) * (b - a)

    def text(self, x, y, text, color, scale=1):
        for ch in text:
            code = ord(ch) - 32
            if not 0 <= code < len(FONT) // 5:
                code = ord("?") - 32
            for cx in range(5):
                bits = FONT[code * 5 + cx]
                for cy in range(7):
                    if bits >> cy & 1:
                        self.box(x + cx * scale, y + cy * scale, scale, scale, color)
            x += 6 * scale
        return x

    def save(self, path):
        raw = b"".join(b"\x00" + bytes(row) for row in self.rows)

        def chunk(kind, data):
            body = kind + data
            return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

        with open(path, "wb") as f:
            f.write(b"\x89PNG\r\n\x1a\n")
            f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", self.width, self.height, 8, 2, 0, 0, 0)))
            f.write(chunk(b"IDAT", zlib.compress(raw, 9)))
            f.write(chunk(b"IEND", b""))


class Transform:
    """A radix-2 FFT of one size, with a Hann window of `window` samples."""

    def __init__(self, size, window):
        self.size, self.window = size, window
        bits = size.bit_length() - 1
        self.order = [int(format(i, f"0{bits}b")[::-1], 2) for i in range(size)]
        self.twiddle = [cmath.exp(-2j * math.pi * i / size) for i in range(size // 2)]
        self.hann = [0.5 - 0.5 * math.cos(2 * math.pi * (i + 0.5) / window) for i in range(window)]
        # So that a full-scale sine reads 0 dB.
        self.scale = 2.0 / sum(self.hann)

    def levels(self, samples, centre):
        """Magnitudes (0 dB is a full-scale sine) of the bins up to half
        the rate, for the window centred on `centre`."""
        start = centre - self.window // 2
        size = self.size
        data = [0j] * size
        hann = self.hann
        n = len(samples)
        for i in range(self.window):
            j = start + i
            if 0 <= j < n:
                data[self.order[i]] = samples[j] * hann[i]
        half, step = 1, size // 2
        twiddle = self.twiddle
        while half < size:
            for k in range(half):
                w = twiddle[k * step]
                for i in range(k, size, half * 2):
                    b = data[i + half] * w
                    a = data[i]
                    data[i] = a + b
                    data[i + half] = a - b
            half *= 2
            step //= 2
        scale = self.scale
        return [abs(data[i]) * scale for i in range(size // 2)]


SHORT = Transform(2048, 384)   # 8 ms: for a key's sound
LONG = Transform(2048, 2048)   # 43 ms: for what rings


def panel(pic, x, y, width, left, right, seconds):
    """The first `seconds` of a sound at (x, y): waveform over spectrogram,
    with a time scale."""
    frames = max(1, int(seconds * RATE))
    mono = [(a + b) / (2 * FULL) for a, b in zip(left[:frames], right[:frames])]
    # The waveform: the loudest sample of either channel under each pixel.
    pic.box(x, y, width, WAVE_H, (26, 27, 36))
    mid = y + WAVE_H // 2
    half = WAVE_H // 2
    pic.box(x, mid - int(half * 0.5 / 0.71), width, 1, LINE)
    pic.box(x, mid + int(half * 0.5 / 0.71), width, 1, LINE)
    for px in range(width):
        a = px * frames // width
        b = max(a + 1, (px + 1) * frames // width)
        peak = max(max(map(abs, left[a:b]), default=0), max(map(abs, right[a:b]), default=0))
        reach = min(half, round(peak / FULL / 0.71 * half))
        if peak:
            pic.box(x + px, mid - reach, 1, 2 * reach + 1, (150, 214, 255))
    # The spectrogram, a column every two pixels.
    top = y + WAVE_H + 2
    transform = SHORT if seconds < 0.3 else LONG
    bins = transform.size // 2
    hz = RATE / transform.size
    octaves = math.log2(F_HIGH / F_LOW)
    edges = [F_LOW * 2 ** (octaves * i / SPEC_H) / hz for i in range(SPEC_H + 1)]
    for px in range(0, width, 2):
        centre = int((px + 1) * frames / width)
        levels = transform.levels(mono, centre)
        for row in range(SPEC_H):
            lo, hi = edges[row], edges[row + 1]
            a, b = int(lo), min(int(hi) + 1, bins - 1)
            if b - a <= 1:
                # Under a bin wide: between the two bins it lies in.
                f = (lo + hi) / 2
                i = min(int(f), bins - 2)
                level = levels[i] + (levels[i + 1] - levels[i]) * (f - i)
            else:
                level = max(levels[a:b])
            decibels = 20 * math.log10(level) if level > 1e-9 else -180.0
            color = heat((decibels - DB_FLOOR) / (DB_TOP - DB_FLOOR))
            pic.box(x + px, top + SPEC_H - 1 - row, 2, 1, color)
    # 1 kHz and 10 kHz.
    for f, name in ((1000.0, "1k"), (10000.0, "10k")):
        row = int(math.log2(f / F_LOW) / octaves * SPEC_H)
        for px in range(0, width, 6):
            pic.dot(x + px, top + SPEC_H - 1 - row, (90, 92, 110))
        pic.text(x + 2, top + SPEC_H - 9 - row, name, (150, 152, 170))
    # Time: a mark every 10 ms, 100 ms or second, whichever gives a few.
    step = next(s for s in (0.01, 0.05, 0.1, 0.5, 1.0, 5.0) if seconds / s <= 12)
    t = 0.0
    while t <= seconds + 1e-9:
        px = min(width - 1, int(t / seconds * width))
        pic.box(x + px, top + SPEC_H, 1, 3, DIM)
        if t > 0:
            label = f"{t * 1000:.0f}ms" if step < 1 else f"{t:.0f}s"
            pic.text(x + px - 3 * len(label), top + SPEC_H + 5, label, DIM)
        t += step


def read(path):
    with wave.open(path, "rb") as f:
        samples = array.array("h")
        samples.frombytes(f.readframes(f.getnframes()))
    if sys.byteorder == "big":
        samples.byteswap()
    return list(samples[0::2]), list(samples[1::2])


def length(left, right):
    """Frames up to the last sample within 60 dB of the peak, and the peak."""
    peak = max(max(map(abs, left), default=0), max(map(abs, right), default=0))
    if peak == 0:
        return 0, 0
    last = len(left) - 1
    while abs(left[last]) < peak / 1000 and abs(right[last]) < peak / 1000:
        last -= 1
    return last + 1, peak


def main(folder, out, names):
    with open(os.path.join(folder, "index.tsv"), encoding="utf-8") as f:
        lines = [line.rstrip("\n").split("\t") for line in f][1:]
    width = MARGIN * 2 + LABEL_W + FIRST_W + GAP + WHOLE_W
    pic = Picture(width, MARGIN * 2 + 22 + ROW_H * len(lines))
    title = open(os.path.join(folder, "index.txt"), encoding="utf-8").readline().strip()
    pic.text(MARGIN, MARGIN, title, INK, 2)
    y = MARGIN + 22
    for file, kind, name, by, takes in lines:
        left, right = read(os.path.join(folder, file))
        starts = [int(s) for s in takes.split(",")] if takes else [0]
        end = starts[1] if len(starts) > 1 else len(left)
        first_l, first_r = left[starts[0]:end], right[starts[0]:end]
        frames, peak = length(first_l, first_r)
        if kind == "sound":
            number = int(name)
            label = f"{number:02} {names[number - 1]}" if number <= len(names) else f"sound {number}"
        elif kind == "typing":
            label = "typing"
        else:
            label = name
        pic.box(MARGIN, y, width - 2 * MARGIN, 1, LINE)
        pic.text(MARGIN, y + 8, label[:19], INK, 2)
        level = 20 * math.log10(peak / FULL) if peak else -99.0
        pic.text(MARGIN, y + 30, f"{frames * 1000 / RATE:.0f} ms to -60 dB", DIM)
        pic.text(MARGIN, y + 42, f"peak {level:.1f} dBFS", DIM)
        if kind == "typing":
            pic.text(MARGIN, y + 54, name, DIM)
        elif by:
            pic.text(MARGIN, y + 54, f"by {by}", DIM)
        x = MARGIN + LABEL_W
        if kind == "typing":
            seconds = len(left) / RATE
            panel(pic, x, y + 8, FIRST_W + GAP + WHOLE_W, left, right, seconds)
        else:
            # The take and a little after it, never less than 60 ms.
            seconds = max(0.06, frames / RATE * 1.15)
            panel(pic, x, y + 8, FIRST_W, first_l, first_r, seconds)
            if len(starts) > 1:
                panel(pic, x + FIRST_W + GAP, y + 8, WHOLE_W, left, right, len(left) / RATE)
        y += ROW_H
        print(f"{label}", file=sys.stderr)
    pic.save(out)
    print(f"{out}: {pic.width} x {pic.height}")


if __name__ == "__main__":
    args = sys.argv[1:]
    names = []
    if "--names" in args:
        at = args.index("--names")
        names = args[at + 1].split(",")
        del args[at:at + 2]
    if len(args) != 2:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    main(args[0], args[1], names)
