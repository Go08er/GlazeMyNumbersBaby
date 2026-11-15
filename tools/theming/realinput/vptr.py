#!/usr/bin/env python3
"""Holds a wlr virtual pointer open on the seat, and does nothing else.

A headless sway has no pointer device, so its seat has no pointer capability
and a client never gets pointer events, however `swaymsg seat seat0 cursor ...`
moves the cursor. While this runs the seat has a pointer, and sway's own IPC
(cursor set / press / release) drives it. Plain Wayland wire protocol over
the socket, standard library only. Exits when killed (sw.sh kills it by PID).
"""
import os
import socket
import struct
import sys
import time


def msg(obj, op, payload=b""):
    return struct.pack("<II", obj, ((8 + len(payload)) << 16) | op) + payload


def string(s):
    b = s.encode() + b"\0"
    return struct.pack("<I", len(b)) + b + b"\0" * (-len(b) % 4)


path = os.path.join(os.environ["XDG_RUNTIME_DIR"], os.environ["WAYLAND_DISPLAY"])
s = socket.socket(socket.AF_UNIX)
s.connect(path)
s.sendall(msg(1, 1, struct.pack("<I", 2)))  # wl_display.get_registry -> id 2
buf = b""
globs = {}
deadline = time.time() + 5
s.sendall(msg(1, 0, struct.pack("<I", 3)))  # wl_display.sync -> id 3
done = False
while not done and time.time() < deadline:
    buf += s.recv(65536)
    while len(buf) >= 8:
        obj, w = struct.unpack_from("<II", buf)
        size, op = w >> 16, w & 0xFFFF
        if len(buf) < size:
            break
        body = buf[8:size]
        buf = buf[size:]
        if obj == 2 and op == 0:  # registry.global
            name, n = struct.unpack_from("<II", body)
            iface = body[8 : 8 + n - 1].decode()
            ver = struct.unpack_from("<I", body, 8 + n + (-n % 4))[0]
            globs[iface] = (name, ver)
        elif obj == 3:  # callback.done
            done = True
for need in ("wl_seat", "zwlr_virtual_pointer_manager_v1"):
    if need not in globs:
        sys.exit(f"vptr.py: the compositor has no {need}")


def bind(iface, ver, new_id):
    name, v = globs[iface]
    return msg(2, 0, struct.pack("<I", name) + string(iface) + struct.pack("<II", min(ver, v), new_id))


# seat -> id 4, manager -> id 5, manager.create_virtual_pointer(seat, id 6)
s.sendall(bind("wl_seat", 1, 4) + bind("zwlr_virtual_pointer_manager_v1", 1, 5))
s.sendall(msg(5, 0, struct.pack("<II", 4, 6)))
print("vptr: ready", flush=True)
while True:
    if not s.recv(65536):
        break
