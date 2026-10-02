# GMNB — GlazeMyNumbers,Baby

The open-source **Windows Calculator**, ported to Rust and given a GTK 4 /
libadwaita interface that is considerably more beautiful than a calculator
has any reason to be.

Every mode of the original is here: Standard, Scientific, Programmer,
Graphing, Date calculation, and thirteen converters (including live
currency), plus history, memory, keep-on-top, copy/paste and the full
keyboard map. The arithmetic is not a re-imagining: the original
arbitrary-precision engine (`Ratpack` + `CalcManager`) was ported
function-for-function and is checked against the real C++ engine.

> Not affiliated with or endorsed by Microsoft. Based on
> [microsoft/calculator](https://github.com/microsoft/calculator) (MIT).

<p align="center">
  <img src="docs/screenshots/standard.png" width="49%" alt="Standard mode with history">
  <img src="docs/screenshots/graphing.png" width="49%" alt="Graphing mode">
</p>
<p align="center">
  <img src="docs/screenshots/scientific.png" width="24%" alt="Scientific, light">
  <img src="docs/screenshots/programmer.png" width="24%" alt="Programmer, Ember palette">
  <img src="docs/screenshots/currency.png" width="24%" alt="Currency, Abyss palette">
  <img src="docs/screenshots/palettes.png" width="24%" alt="Palette settings">
</p>

## Install

Every release attaches all of these to its
[GitHub release](https://github.com/Go08er/GlazeMyNumbersBaby/releases).

| Platform | How |
| --- | --- |
| **Flatpak** (any distro) | download `GMNB.flatpak`, then `flatpak install --user GMNB.flatpak` (pulls the GNOME 51 runtime from Flathub) |
| **Arch Linux** | `sudo pacman -U gmnb-*.pkg.tar.zst`, or build `packaging/arch/PKGBUILD` with `makepkg -si` |
| **Debian 13+ / Ubuntu** | `sudo apt install ./gmnb_*_amd64.deb` |
| **Fedora** | `sudo dnf install ./gmnb-*.rpm` |
| **NixOS / Nix** | `nix run github:Go08er/GlazeMyNumbersBaby`, or the module below |

### NixOS module

```nix
{
  inputs.gmnb.url = "github:Go08er/GlazeMyNumbersBaby";

  outputs = { nixpkgs, gmnb, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        gmnb.nixosModules.default
        { programs.gmnb.enable = true; }
      ];
    };
  };
}
```

There's also `gmnb.overlays.default` (adds `pkgs.gmnb`) and
`gmnb.packages.<system>.gmnb` for Home Manager or `environment.systemPackages`.

## Pointlessly beautiful, specifically

- A living **aurora** background: drifting colour fields, film grain, a
  vignette, and soft blooms that ripple out from every key you press.
  It drifts only while the window is focused and settles after 45 s idle.
- **Glass keys** with spring physics, a Fluent-style **reveal light** that
  follows the pointer along nearby key borders, and press ripples that start
  under your finger.
- A display drawn glyph by glyph: typed digits pop in, results rise in a
  staggered wave followed by a sweep of light, errors shake.
- Keys **cascade** in whenever a calculator mode appears; navigation icons
  draw themselves; graph curves trace themselves in and glow.
- **Seven palettes**, each in light and dark:
  - Aurora, Ember, Abyss, Orchard, Graphite.
  - **System** — built from your desktop's accent colour and its complement,
    following it live. This reads the freedesktop portal setting
    `org.freedesktop.appearance accent-color` (GNOME 47+, KDE Plasma 6,
    shells like Noctalia…), falling back to libadwaita's accent.
  - **Freestyle** — pick a primary and secondary colour (or let the
    secondary be the primary's complement) and the whole scheme is
    generated from them.

## Building from source

Everything goes through the flake; you don't need Rust installed.

| What | Command |
| --- | --- |
| Dev shell (cargo, rustc, clippy, rust-analyzer, GTK) | `nix develop` |
| Run from source | `nix develop -c cargo run -p gmnb` |
| Native Nix package | `nix build` (→ `./result/bin/gmnb`), or `nix run` |
| **Flatpak bundle** | `nix run .#flatpak` → `dist/GMNB.flatpak` |
| Flatpak bundle + install | `nix run .#flatpak -- --install` |
| Refresh `cargo-sources.json` after changing dependencies | `nix run .#update-cargo-sources` |

Without Nix: Rust ≥ 1.92, GTK ≥ 4.18, libadwaita ≥ 1.7 and Pango ≥ 1.56,
then `cargo build --release -p gmnb`.

> Flakes only see files git knows about. After adding files, `git add -A`
> (staging is enough); the Flatpak script refuses to build if it finds
> untracked files.

### Packaging layout

```
packaging/
  io.github.Go08er.GlazeMyNumbersBaby.{desktop,metainfo.xml}, icons/
  flatpak/   canonical manifest (Flathub-style: git tag + cargo-sources.json)
  arch/      PKGBUILD
  debian/    copy to ./debian, then dpkg-buildpackage -b (needs rustup's cargo)
  fedora/    gmnb.spec
nix/         package.nix, NixOS module, Flatpak tooling
```

`nix run .#flatpak` reuses the canonical Flatpak manifest verbatim, only
swapping its source for an offline tarball with every crate vendored by Nix.
The `Packages` workflow builds the Flatpak, Arch, Debian and Fedora packages
in their real distro containers and attaches them to the release.

## Layout

```
crates/ratpack      Ratpack + Number/Rational/RationalMath (arbitrary precision)
crates/calcmanager  CEngine + CalculatorManager + history + expression commands
crates/calcvm       StandardCalculatorViewModel & friends (UI-agnostic)
crates/unitconv     UnitConverter engine, unit tables, currency, view model
crates/datecalc     DateCalculator + its view model
crates/copypaste    CopyPasteManager (paste validation → key sequences)
crates/graphing     Numeric graphing engine (parser, sampler, analysis)
app/                The GTK 4 / libadwaita application
tools/oracle/       C++ drivers that generate the golden test data
```

## Verification

`nix develop -c cargo test --workspace` runs **532 tests** (counts include
doctests; one more, a live currency fetch, is `#[ignore]`d). The heart of it
is differential testing against the *real* C++ engine, compiled from the
upstream sources with g++:

| Crate | What's checked |
| --- | --- |
| ratpack (11) | 13,628 golden cases from the C++ Ratpack (every op and function, all angle types, radixes 2–36, formats, precisions, error codes), byte-for-byte; port of `RationalTest.cpp` |
| calcmanager (77) | 3,500 golden command sequences replayed against the C++ `CalculatorManager` (every display callback, expression token, history and memory state); ports of `CalcEngineTests`, `CalcInputTest`, `CalculatorManagerTest` |
| calcvm (121) | Ports of `StandardCalculatorViewModelTests`, `HistoryTests`, the snapshot tests, plus programmer/paste/event coverage |
| unitconv (134 + 1 ignored) | Ports of `UnitConverterTest.cpp`, `UnitConverterViewModelTests`, currency tests, a known value for every unit |
| datecalc (40), copypaste (40) | Ports of `DateCalculatorTests` and `CopyPasteManagerTests`, plus paste key-sequence tests |
| graphing (107) | Parser, sampling and asymptotes, implicit/inequality plots, function analysis, frame-time budgets, and regressions for hostile input (deep nesting, huge nCr/nPr, extreme ranges) |

The oracles live in `tools/oracle/` and need the upstream repository checked
out at `reference/calculator` to regenerate the golden files.

## Deliberate differences from the original

- **Currency rates are real.** Microsoft's endpoints are dead, so the
  open-source app ships fictional planet currencies. GMNB fetches
  central-bank reference rates (158 currencies) via the keyless
  [Frankfurter](https://frankfurter.dev) API, caches them, and falls back to
  a bundled snapshot when offline.
- **Graphing uses a new numeric engine.** The original graphing engine is
  proprietary (open-source builds contain only a mock). GMNB's engine was
  written against the original's interfaces and reproduces its features
  numerically: explicit, implicit and inequality plots, variables with
  sliders, tracing, and key-graph-feature analysis.
- **Keep on top** switches to the original's compact overlay, but Wayland
  has no client-side "always on top": pin it with your compositor (e.g. a
  niri window rule for `io.github.Go08er.GlazeMyNumbersBaby`).
- Dates use the Gregorian calendar; strings are en-US.
- The port fixes a handful of upstream bugs and undefined behaviour (e.g.
  deleting a history item removed the wrong entry; C left the engine in
  E-notation); each is commented at the fix.

## Memory footprint

Technically, GMNB beats KCalc's memory footprint once Vulkan acceleration is
off. With GTK's software renderer it has the smaller resident set and the
same proportional footprint, although KCalc's private heap is a little
smaller. That's despite GMNB also carrying graphing, 13 converters, date
calculation and an embedded font.

| App (Standard mode, idle) | RSS | PSS | Private |
| --- | --- | --- | --- |
| KCalc 26.08.1 (Qt 6, software-drawn) | 79 MB | 36 MB | 13 MB |
| **GMNB, software renderer** (`GSK_RENDERER=cairo`) | **67 MB** | **36 MB** | 18 MB |
| GMNB, default (Vulkan) | 201 MB | 113 MB | 43 MB |

The default build renders on the GPU so the aurora, blur and glow stay
cheap on the CPU. Nearly all of the extra memory is the GPU driver (here
NVIDIA's Vulkan stack) being loaded into the process. To trade animation
smoothness for memory:

```sh
flatpak override --user --env=GSK_RENDERER=cairo io.github.Go08er.GlazeMyNumbersBaby
# or, for one run / native installs:
GSK_RENDERER=cairo gmnb
```

Measured on NixOS with an RTX 3070 (driver 595), both apps in the same
headless Wayland session at 760×700, idle in Standard mode. RSS counts
shared libraries in full; PSS splits them between the processes using them;
"Private" is anonymous memory (heap) only. Numbers will differ with other
GPUs, drivers and fonts.

## Notes

- **Flatpak on NixOS:** Flatpak can't translate NixOS's `/etc/localtime`
  and runs sandboxes in UTC. GMNB asks systemd-timedated for the real zone
  (read-only `org.freedesktop.timedate1` access), so "Updated …" times and
  Date's "today" are local.
- NVIDIA's driver busy-waits on GPU fences by default, which costs ~20% of a
  core even for gentle animation; GMNB sets `__GL_YIELD=USLEEP` for its own
  process unless you've set it yourself. Measured idle cost is then ~0.25%
  of a core.

## Licences

The code is MIT; see [LICENSE](LICENSE), which carries Microsoft's original
notice. The *Outfit* typeface is under the SIL Open Font License
(`app/assets/fonts/OFL-Outfit.txt`). Exchange-rate data comes from
Frankfurter (central bank reference rates).
