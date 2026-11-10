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

A text field (a Field of the schema) is marked the same way with
`field` and ended with `/* end field */`: its resting properties, its
placeholder, selection and caret colours, and one rule each for `focus`,
`error` and `error-focus`:

    /* field field entry scope=window.gmnb.own-field */
    ...
    /* end field */

A Text (`text`) and a Surface (`surface`) are marked the same way, each
ended with its own `/* end text */` or `/* end surface */`, and written
as one rule of their properties.

and this tool rewrites every marked block in place:

    tools/theming/control_css.py apps/gmnb/src/style.css

To style a new control, add its marker line and the end line, and run it.
The marker's words: the style path, the selector, then any of

    scope=SELECTOR     an ancestor the rules sit under (with a space after
                       it); `+` stands for a space inside it, as in
                       the selector
    active=SEL,...     more ways of being pressed, besides `:active`
    checked=SEL,...    the ways of being on (default `:checked`)
    icon=SELECTOR      a descendant that takes the icon colour
    focus=SEL,...      (a field) the ways of holding the focus (default
                       `:focus-within`)
    error=SEL,...      (a field) the ways of being wrong (default `.error`)

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

MARK = re.compile(r"^/\* (control|field|text|surface) (\S+) (\S+)((?: \S+=\S+)*) \*/$")
END = {k: f"/* end {k} */" for k in ("control", "field", "text", "surface")}
END["control"] = "/* end control */"

TEXT = [
    ("color", "color"),
    ("font-family", "font-family"),
    ("font-size", "font-size"),
    ("font-weight", "font-weight"),
    ("font-style", "font-style"),
    ("letter-spacing", "letter-spacing"),
    ("font-feature-settings", "font-features"),
    ("font-variation-settings", "font-variations"),
    ("text-shadow", "text-shadow"),
    ("margin", "margin"),
]

SURFACE = [
    ("background-color", "background-color"),
    ("background-image", "background-image"),
    ("box-shadow", "shadow"),
    ("border-radius", "radius"),
    ("padding", "padding"),
    ("margin", "margin"),
]

# A field's resting properties: a control's, but no margin or least width.
FIELD_REST = [p for p in REST if p[1] not in ("margin", "min-width")]


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
    base = scope + selector.replace("+", " ")

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
    covered = scope + ".wc-inert " + selector.replace("+", " ")
    inert = [f"{covered}:disabled:not(.wc-off)"]
    out += block(inert, "", STATE)
    inert = [f"{covered}{way}:disabled:not(.wc-off)" for way in checked]
    out += block(inert, "checked-", STATE)
    return out


def field(path, selector, options):
    scope = options.get("scope", "").replace("+", " ")
    scope = scope + " " if scope else ""
    focus = [s for s in options.get("focus", ":focus-within").split(",") if s]
    error = [s for s in options.get("error", ".error").split(",") if s]
    base = scope + selector.replace("+", " ")

    def combos(*parts):
        sels = [base]
        for ways in parts:
            sels = [s + w for s in sels for w in ways]
        return sels

    out = rule([base], [(p, var(path, "", n)) for p, n in FIELD_REST]
               + [("caret-color", var(path, "", "caret"))])
    # (The toolkit dims a placeholder as well: the colour is all of it.)
    out += rule([f"{base} placeholder"],
                [("color", var(path, "", "placeholder")), ("opacity", "1"), ("filter", "none")])
    out += rule([f"{base} text > selection"],
                [("background-color", var(path, "", "selection"))])
    out += rule(combos(focus), [(p, var(path, "focus-", n)) for p, n in STATE])
    out += rule(combos(error), [(p, var(path, "error-", n)) for p, n in STATE])
    out += rule(combos(error, focus), [(p, var(path, "error-focus-", n)) for p, n in STATE])
    return out


def plain(props):
    def write(path, selector, options):
        scope = options.get("scope", "").replace("+", " ")
        scope = scope + " " if scope else ""
        return rule([scope + selector.replace("+", " ")], [(p, var(path, "", n)) for p, n in props])
    return write


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
        write = {
            "control": control,
            "field": field,
            "text": plain(TEXT),
            "surface": plain(SURFACE),
        }[kind]
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
