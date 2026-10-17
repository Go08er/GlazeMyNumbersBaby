#!/usr/bin/env python3
"""Reads an app's accessibility tree over AT-SPI (development checks only).

    atspi.py enable                  turn accessibility on for the session bus
    atspi.py apps                    the applications on the accessibility bus
    atspi.py dump APP                every node: role | name | text | states
    atspi.py display APP             the calculator display's value
    atspi.py focused APP             the name of the focused control

APP is matched (case-insensitively) against the application names on the
accessibility bus. Needs PyGObject and the Atspi typelib (see run.sh).
"""
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib  # noqa: E402


def enable():
    """Ask at-spi-bus-launcher to switch accessibility on, so toolkits export.
    Retries while the launcher is still starting."""
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    end = time.time() + 10
    while True:
        try:
            for prop in ("IsEnabled", "ScreenReaderEnabled"):
                bus.call_sync(
                    "org.a11y.Bus", "/org/a11y/bus", "org.freedesktop.DBus.Properties",
                    "Set", GLib.Variant("(ssv)", ("org.a11y.Status", prop, GLib.Variant("b", True))),
                    None, Gio.DBusCallFlags.NONE, 3000, None)
            return
        except GLib.Error as e:
            if time.time() > end:
                sys.exit(f"atspi: the accessibility bus did not come up: {e.message}")
            time.sleep(0.2)


def find_app(name, wait=15.0):
    Atspi.init()
    end = time.time() + wait
    while time.time() < end:
        desktop = Atspi.get_desktop(0)
        for i in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(i)
            if app is not None and name.lower() in (app.get_name() or "").lower():
                return app
        time.sleep(0.3)
    sys.exit(f"atspi: no application matching {name!r} on the accessibility bus")


def walk(node, depth=0):
    if depth > 40:
        return
    yield node
    try:
        n = node.get_child_count()
    except GLib.Error:
        return
    for i in range(n):
        try:
            child = node.get_child_at_index(i)
        except GLib.Error:
            continue
        if child is not None:
            yield from walk(child, depth + 1)


def text_of(node):
    try:
        t = node.get_text_iface()
        if t is not None:
            return Atspi.Text.get_text(t, 0, min(Atspi.Text.get_character_count(t), 200))
    except GLib.Error:
        pass
    return ""


def states_of(node):
    names = set()
    try:
        ss = node.get_state_set()
    except GLib.Error:
        return names
    for i in range(0, 64):
        try:
            st = Atspi.StateType(i)
        except (ValueError, TypeError):
            continue
        if ss.contains(st):
            names.add(st.value_nick)
    return names


def display_value(app):
    """The display's value: DGMNB names its display "Display is N"; GMNB's
    display is a label whose accessible name or text starts the same way."""
    for node in walk(app):
        for s in (node.get_name() or "", text_of(node)):
            if s.startswith("Display is "):
                return s[len("Display is "):]
    return None


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "enable":
        enable()
        return
    if cmd == "apps":
        Atspi.init()
        desktop = Atspi.get_desktop(0)
        for i in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(i)
            print(app.get_name() if app is not None else None)
        return
    if cmd not in ("dump", "display", "focused") or len(sys.argv) < 3:
        sys.exit(__doc__)
    app = find_app(sys.argv[2])
    if cmd == "dump":
        for node in walk(app):
            print(f"{node.get_role_name()} | {node.get_name()} | {text_of(node)} | "
                  f"{','.join(sorted(states_of(node)))}")
    elif cmd == "display":
        value = display_value(app)
        if value is None:
            sys.exit("atspi: no display node found")
        print(value)
    else:
        for node in walk(app):
            if "focused" in states_of(node) and node.get_role_name() not in ("window", "frame"):
                print(node.get_name())
                return
        sys.exit("atspi: nothing is focused")


if __name__ == "__main__":
    main()
