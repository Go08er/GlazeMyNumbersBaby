#!/usr/bin/env python3
"""Draws Glazed's pictures: every file in images/ is this script's output.

    python3 -I make_assets.py            writes images/ beside this file
    python3 -I make_assets.py --check    fails if a file differs from what it would write

Nothing here is copied from anywhere: the pictures are drawn below, pixel
by pixel, and the same input always gives the same pictures (the sprinkles
come from a fixed seed). It needs only Python's standard library. The
check compares a PNG's pixels, not its bytes: how small zlib packs them
is that zlib's own business, and differs from one build of it to another.

Glazed is drawn on a grid of *art pixels*, each 2 x 2 real ones, like the
theme's own drawing (theme.luau, `PX`). Stills are written at their real
size, because the app tiles a picture at its own size. The loop is written
at art size, and the theme draws it twice as large with hard pixels.
"""

import os
import struct
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "images")
PX = 2

# ---- Writing PNG and GIF files ----------------------------------------------


def png(width, height, rgba):
    """An 8-bit RGBA PNG of `rgba`, `width * height * 4` bytes, rows from the top."""
    assert len(rgba) == width * height * 4

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    rows = b"".join(b"\0" + rgba[y * width * 4 : (y + 1) * width * 4] for y in range(height))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )


def unpng(data):
    """(width, height, pixels) of a PNG as `png` writes them: 8-bit RGBA, no filter."""
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    at, packed, size = 8, b"", None
    while at < len(data):
        (length,) = struct.unpack(">I", data[at : at + 4])
        kind, body = data[at + 4 : at + 8], data[at + 8 : at + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert (depth, colour, interlace) == (8, 6, 0)
            size = (width, height)
        elif kind == b"IDAT":
            packed += body
        at += 12 + length
    width, height = size
    rows = zlib.decompress(packed)
    stride = 1 + width * 4
    assert len(rows) == stride * height and all(rows[y * stride] == 0 for y in range(height))
    return width, height, b"".join(rows[y * stride + 1 : (y + 1) * stride] for y in range(height))


def lzw(indices, bits):
    """GIF's LZW of a frame's palette indices, `bits` being the code size it starts from."""
    clear, end = 1 << bits, (1 << bits) + 1
    out = bytearray()
    held, count = 0, 0
    size = bits + 1
    table = {}
    following = end + 1

    def emit(code):
        nonlocal held, count
        held |= code << count
        count += size
        while count >= 8:
            out.append(held & 255)
            held >>= 8
            count -= 8

    emit(clear)
    prefix = indices[0]
    for index in indices[1:]:
        key = (prefix << 8) | index
        code = table.get(key)
        if code is not None:
            prefix = code
            continue
        emit(prefix)
        if following < 4096:
            table[key] = following
            if following == 1 << size and size < 12:
                size += 1
            following += 1
        else:
            emit(clear)
            table = {}
            size = bits + 1
            following = end + 1
        prefix = index
    emit(prefix)
    emit(end)
    if count:
        out.append(held & 255)
    return bytes(out)


def gif(width, height, palette, frames, centiseconds):
    """A looping GIF. `palette` is a list of (r, g, b); index 0 is see-through.
    Each frame is `width * height` palette indices and is cleared before the next."""
    bits = 2
    while (1 << bits) < len(palette):
        bits += 1
    table = b"".join(bytes(c) for c in palette) + b"\0\0\0" * ((1 << bits) - len(palette))
    out = bytearray(b"GIF89a")
    out += struct.pack("<HHBBB", width, height, 0x80 | 0x70 | (bits - 1), 0, 0)
    out += table
    out += b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00"
    for frame in frames:
        assert len(frame) == width * height
        # Disposal 2 (clear to see-through), and index 0 is the see-through one.
        out += b"\x21\xf9\x04" + struct.pack("<BHB", (2 << 2) | 1, centiseconds, 0) + b"\x00"
        out += b"\x2c" + struct.pack("<HHHHB", 0, 0, width, height, 0)
        out.append(bits)
        data = lzw(frame, bits)
        for at in range(0, len(data), 255):
            piece = data[at : at + 255]
            out.append(len(piece))
            out += piece
        out.append(0)
    out.append(0x3B)
    return bytes(out)


# ---- A sheet of art pixels ----------------------------------------------------


def colour(text):
    """'#rrggbb' or '#rrggbbaa' as (r, g, b, a)."""
    text = text.lstrip("#")
    parts = [int(text[i : i + 2], 16) for i in range(0, len(text), 2)]
    return tuple(parts + [255])[:4] if len(parts) == 3 else tuple(parts)


class Sheet:
    """A picture in art pixels, see-through to begin with."""

    def __init__(self, width, height):
        self.width, self.height = width, height
        self.cells = [(0, 0, 0, 0)] * (width * height)

    def put(self, x, y, c):
        if 0 <= x < self.width and 0 <= y < self.height:
            self.cells[y * self.width + x] = c

    def rect(self, x, y, w, h, c):
        for j in range(y, y + h):
            for i in range(x, x + w):
                self.put(i, j, c)

    def png(self, scale=PX):
        """The sheet as a PNG, each art pixel `scale` real ones a side."""
        rows = []
        for y in range(self.height):
            row = b"".join(bytes(c) * scale for c in self.cells[y * self.width : (y + 1) * self.width])
            rows.extend([row] * scale)
        return png(self.width * scale, self.height * scale, b"".join(rows))


# ---- The pictures -------------------------------------------------------------

# The shop's wall: square tiles, two shades in a chequer, with a line of
# grout to the right of and under each and a glint along its top and left.
# One picture is four tiles; the theme repeats it over the window.
TILE = 16
WALLS = {
    # name: (tile, the other tile, grout, glint)
    "wall-light": ("#fff4df", "#ffedcd", "#f0d9b5", "#fffaf0"),
    "wall-dark": ("#2b1a22", "#301e27", "#1d1017", "#38242e"),
}


def wall(tile, other, grout, glint):
    sheet = Sheet(TILE * 2, TILE * 2)
    for ty in range(2):
        for tx in range(2):
            x, y = tx * TILE, ty * TILE
            sheet.rect(x, y, TILE, TILE, colour(tile if (tx + ty) % 2 == 0 else other))
            sheet.rect(x, y, TILE - 1, 1, colour(glint))
            sheet.rect(x, y, 1, TILE - 1, colour(glint))
            sheet.rect(x + TILE - 1, y, 1, TILE, colour(grout))
            sheet.rect(x, y + TILE - 1, TILE, 1, colour(grout))
    return sheet.png()


# The awning's hem: scallops, one under each stripe. Three masks (white,
# with only their opacity meaning anything), which the theme fills with
# the flavour's colours: the scallops under the odd stripes, those under
# the even ones, and the line around them all.
SCALLOP_W, SCALLOP_H = 16, 7
# How many art pixels in from each side a scallop's row starts, top row first.
SCALLOP_INSET = [0, 0, 0, 1, 1, 2, 4]


def scallops():
    white = (255, 255, 255, 255)
    odd, even, line = (Sheet(SCALLOP_W * 2, SCALLOP_H + 1) for _ in range(3))
    for n, body in enumerate((odd, even)):
        x0 = n * SCALLOP_W
        for row, inset in enumerate(SCALLOP_INSET):
            body.rect(x0 + inset, row, SCALLOP_W - 2 * inset, 1, white)
        # The line: one art pixel under the scallop wherever nothing of it
        # is below, and up the step at each end of a row.
        insets = SCALLOP_INSET + [SCALLOP_W // 2]
        for row in range(SCALLOP_H):
            here, below = insets[row], insets[row + 1]
            line.rect(x0 + here, row + 1, below - here, 1, white)
            line.rect(x0 + SCALLOP_W - below, row + 1, below - here, 1, white)
    return odd.png(), even.png(), line.png()


# Sprinkles falling: a loop that repeats in time and, drawn side by side
# and one above the other, in space. Each sprinkle is a rod three art
# pixels long that turns as it falls; some fall one art pixel a frame and
# some two, so after `LOOP` frames each is back where it began.
LOOP = 128  # frames, and the picture's side in art pixels
SPRINKLE_COLOURS = ["#ff6fae", "#ffd23f", "#5fd6b4", "#6aa5ff", "#ff9b54", "#c58cff"]
RODS = [
    [(0, 1), (1, 1), (2, 1)],  # lying
    [(0, 2), (1, 1), (2, 0)],  # leaning one way
    [(1, 0), (1, 1), (1, 2)],  # standing
    [(0, 0), (1, 1), (2, 2)],  # leaning the other
]


def rng(seed):
    """A 32-bit xorshift, as the theme's own (theme.luau, `random`)."""
    state = seed

    def step():
        nonlocal state
        state ^= (state << 13) & 0xFFFFFFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFFFFFF
        return state

    return step


def sprinkles():
    step = rng(0x51EDBEEF)
    rods = []
    # A sprinkle to every 32 x 32 art pixels or so, none close to another.
    for cy in range(4):
        for cx in range(4):
            x = cx * 32 + 4 + step() % 22
            y = cy * 32 + 4 + step() % 22
            rods.append((x, y, 1 + step() % 2, step() % len(SPRINKLE_COLOURS), step() % 4, 6 + step() % 3 * 2))
    frames = []
    for frame in range(LOOP):
        cells = bytearray(LOOP * LOOP)
        for x, y, speed, which, turn, patience in rods:
            top = (y + frame * speed) % LOOP
            # It turns every `patience` frames; 128 frames hold a whole number of turns of 4.
            shape = RODS[(turn + frame // 8) % 4] if patience else RODS[turn]
            for dx, dy in shape:
                cells[((top + dy) % LOOP) * LOOP + (x + dx) % LOOP] = which + 1
        frames.append(bytes(cells))
    palette = [(0, 0, 0)] + [colour(c)[:3] for c in SPRINKLE_COLOURS]
    return gif(LOOP, LOOP, palette, frames, 12)


def pictures():
    files = {}
    for name, colours in WALLS.items():
        files[name + ".png"] = wall(*colours)
    odd, even, line = scallops()
    files["scallop-odd.png"] = odd
    files["scallop-even.png"] = even
    files["scallop-line.png"] = line
    files["sprinkles.gif"] = sprinkles()
    return files


def main():
    check = "--check" in sys.argv[1:]
    bad = 0
    os.makedirs(OUT, exist_ok=True)
    for name, data in sorted(pictures().items()):
        path = os.path.join(OUT, name)
        if check:
            have = open(path, "rb").read() if os.path.exists(path) else None
            if have is not None and name.endswith(".png"):
                same = unpng(have) == unpng(data)
            else:
                same = have == data
            print(("ok       " if same else "DIFFERS  ") + name)
            bad += not same
        else:
            with open(path, "wb") as f:
                f.write(data)
            print(f"{len(data):7d}  {name}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
