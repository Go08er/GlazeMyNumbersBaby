#!/usr/bin/env python3
"""Writes a control's rules in GMNB's structure stylesheet.

A Control of the theme schema (crates/gmnb-theme/src/schema.rs) is always
styled the same way: its resting properties, then one rule per state
combination (hover, active, checked, checked-hover, checked-active,
disabled, checked-disabled), each reading the finished values the app
writes as custom properties (docs/theming.md, "Names of the custom
properties"), and the two rules that keep what a dialog or the sidebar
covers looking as it did (`.wc-inert`, "Covered is not disabled").

The stylesheet marks each such block with a line saying what it is for:

    /* control keys.number button.wc-key.wc-num active=.wc-flash icon=pathicon */
    ...
    /* end control */

and this tool rewrites every marked block in place:

    tools/theming/control_css.py apps/gmnb/src/style.css

To style a new control, add its marker line and the end line, and run it.
The marker's words: the style path, the selector, then any of

    scope=SELECTOR     an ancestor the rules sit under (with a space after
                       it); `+` stands for a space inside it
    active=SEL,...     more ways of being pressed, besides `:active`
    checked=SEL,...    the ways of being on (default `:checked`)
    icon=SELECTOR      a descendant that takes the icon colour

A Text (a label's look: colour, font, halo, margin) is marked the same
way, with no options but `scope=`:

    /* text header.title .wc-title */
    ...
    /* end text */

`--check` changes nothing and fails if a block is out of date.
"""

import re
import sys

REST = [
    ("background-color", "background-color"),
    ("background-image", "background-image"),
    ("color", "color"),
    ("box-shadow", "shadow"),
    ("opacity", "opacity"),
    ("filter", "filter"),
    ("transform", "transform"),
    ("transition", "transition"),
    ("border-radius", "radius"),
    ("padding", "padding"),
    ("margin", "margin"),
    ("min-width", "min-width"),
    ("min-height", "min-height"),
    ("font-family", "font-family"),
    ("font-size", "font-size"),
    ("line-height", "line-height"),
    ("font-weight", "font-weight"),
    ("font-style", "font-style"),
    ("letter-spacing", "letter-spacing"),
    ("font-feature-settings", "font-features"),
    ("font-variation-settings", "font-variations"),
]

# What a state combination is written as (docs/theming.md).
STATE = [
    ("background-color", "background-color"),
    ("background-image", "background-image"),
    ("color", "color"),
    ("box-shadow", "shadow"),
    ("opacity", "opacity"),
    ("filter", "filter"),
    ("transform", "transform"),
    ("transition", "transition"),
    ("font-weight", "font-weight"),
]

# A Text's properties.
TEXT = [
    ("color", "color"),
    ("font-family", "font-family"),
    ("font-size", "font-size"),
    ("line-height", "line-height"),
    ("font-weight", "font-weight"),
    ("font-style", "font-style"),
    ("letter-spacing", "letter-spacing"),
    ("font-feature-settings", "font-features"),
    ("font-variation-settings", "font-variations"),
    ("text-shadow", "text-shadow"),
    ("margin", "margin"),
    ("padding-top", "text-nudge"),
]

MARK = re.compile(r"^/\* (control|text) (\S+) (\S+)((?: \S+=\S+)*) \*/$")
END = {"control": "/* end control */", "text": "/* end text */"}


def var(path, lead, name):
    prop = "--gmnb-" + path.replace(".", "-").replace("_", "-")
    return f"var({prop}-{lead}{name})"


def rule(selectors, declarations):
    out = ",\n".join(selectors) + " {\n"
    for prop, value in declarations:
        out += f"  {prop}: {value};\n"
    return out + "}\n"


def control(path, selector, options):
    scope = options.get("scope", "").replace("+", " ")
    scope = scope + " " if scope else ""
    active = [":active"] + [s for s in options.get("active", "").split(",") if s]
    checked = [s for s in options.get("checked", ":checked").split(",") if s]
    icon = options.get("icon")
    base = scope + selector

    def combos(*parts):
        """Every selector for the base with one of each part's ways appended."""
        sels = [base]
        for ways in parts:
            sels = [s + w for s in sels for w in ways]
        return sels

    def block(selectors, lead, props):
        out = rule(selectors, [(p, var(path, lead, n)) for p, n in props])
        if icon:
            out += rule(
                [f"{s} {icon}" for s in selectors],
                [("color", var(path, lead, "icon-color"))],
            )
        return out

    out = block([base], "", REST)
    # Its label's line sits where DGMNB puts it (css.rs `text_line`).
    out += rule([f"{base} label"], [("padding-top", var(path, "", "text-nudge"))])
    out += block(combos([":hover"]), "hover-", STATE)
    out += block(combos(active), "active-", STATE)
    out += block(combos(checked), "checked-", STATE)
    out += block(combos(checked, [":hover"]), "checked-hover-", STATE)
    out += block(combos(checked, active), "checked-active-", STATE)
    out += block(combos([":disabled"]), "disabled-", STATE)
    out += block(combos(checked, [":disabled"]), "checked-disabled-", STATE)
    # Covered is not disabled: what the app itself didn't turn off keeps
    # its look.
    # (`.wc-inert` is inside the window: it goes after the scope.)
    covered = scope + ".wc-inert " + selector
    inert = [f"{covered}:disabled:not(.wc-off)"]
    out += block(inert, "", STATE)
    inert = [f"{covered}{way}:disabled:not(.wc-off)" for way in checked]
    out += block(inert, "checked-", STATE)
    return out


def text_rule(path, selector, options):
    scope = options.get("scope", "").replace("+", " ")
    scope = scope + " " if scope else ""
    return rule([scope + selector], [(p, var(path, "", n)) for p, n in TEXT])


def regenerate(text):
    lines = text.split("\n")
    out, i = [], 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        i += 1
        m = MARK.match(line)
        if not m:
            continue
        kind, path, selector = m.group(1), m.group(2), m.group(3)
        options = dict(w.split("=", 1) for w in m.group(4).split())
        while i < len(lines) and lines[i] != END[kind]:
            i += 1
        if i == len(lines):
            sys.exit(f"{kind} {path}: no '{END[kind]}' line")
        write = control if kind == "control" else text_rule
        out.append(write(path, selector, options).rstrip("\n"))
    return "\n".join(out)


def main():
    args = [a for a in sys.argv[1:] if a != "--check"]
    if len(args) != 1:
        sys.exit(__doc__)
    text = open(args[0]).read()
    new = regenerate(text)
    if "--check" in sys.argv:
        sys.exit(0 if new == text else f"{args[0]}: control blocks are out of date")
    if new != text:
        open(args[0], "w").write(new)


if __name__ == "__main__":
    main()
