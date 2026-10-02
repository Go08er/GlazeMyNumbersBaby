# Toolkit bench

The throwaway prototypes behind DGMNB's toolkit choice, kept so the
README's comparison can be reproduced. Each opens a 760×700 window with a
display, 24 calculator keys and (where the toolkit has one) a text field:

| Crate | Stack |
| --- | --- |
| `p-raw` | winit + softbuffer + tiny-skia + fontdue, drawn by hand |
| `p-raw2` | the same plus optional `a11y` (AccessKit), `clip` (smithay-clipboard) and `portal` (zbus) features |
| `p-iced` | iced 0.14 with its tiny-skia renderer |
| `p-slint` | Slint 1.18 with its software renderer |
| `p-gtk` | GTK 4 without libadwaita (run with `GSK_RENDERER=cairo`) |
| `p-fltk` | fltk-rs on Wayland (crashed on the headless compositor here) |

These are bare windows, not finished apps: DGMNB itself (every mode,
accessibility, clipboard, desktop colours) measures a few MB more than
`p-raw2` with all features. They're measurement fixtures, not examples to
copy: `p-raw2`'s `clip` feature hands winit's Wayland display to
smithay-clipboard without the orderly shutdown DGMNB does before the event
loop closes that display.

`mem.sh` measures the process it launches (and checks it's still the named
app throughout), so an already running copy of an app is never measured or
stopped.

```sh
nix develop -f tools/bench/shell.nix
cd tools/bench && cargo build --release
export BENCH_SOCKET=bench && ./weston.sh
./mem.sh raw    p-raw   ./target/release/p-raw
./mem.sh iced   p-iced  ./target/release/p-iced
./mem.sh slint  p-slint env SLINT_BACKEND=winit-software ./target/release/p-slint
./mem.sh gtk    p-gtk   env GSK_RENDERER=cairo GDK_BACKEND=wayland ./target/release/p-gtk
# the apps themselves (from the repository root's builds):
./mem.sh dgmnb  dgmnb   env DGMNB_SIZE=760x700 ../../target/lean/dgmnb
./mem.sh gmnb   gmnb    env GMNB_SIZE=760x700 GDK_BACKEND=wayland ../../target/release/gmnb
```

Numbers depend on the GPU, driver, fonts and compositor; the README's were
taken on NixOS with an RTX 3070 (driver 595) under headless weston at 1×
scale. GMNB's default renderer loads the GPU driver into the process, which
is most of its footprint.
