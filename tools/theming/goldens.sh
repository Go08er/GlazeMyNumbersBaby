#!/usr/bin/env bash
# Reference screenshots for the theming work (DGMNB also writes the boxes of its text and curves next to each picture, dgmnb-NAME.json), taken fully isolated: the
# private headless weston (socket wincalc-shot), temporary XDG dirs, and a
# private D-Bus with no services (no portal: the default accent, no desktop
# settings). Usage, from the repo root, inside the nix dev shell:
#   tools/theming/goldens.sh OUTDIR BINDIR [gmnb|dgmnb|all]
# BINDIR holds the gmnb and dgmnb binaries to capture (theming-work/v02-bin
# for the v0.2.0 references).
set -u
SP=/tmp/claude-1000/-home-goober-Documents-wincalc/a4c4533e-9373-47f7-85c3-9ead96a053d7/scratchpad
OUT=${1:?outdir}; BIN=${2:?bindir}; WHICH=${3:-all}
mkdir -p "$OUT"
X=$(mktemp -d "$SP/xdg.XXXX"); mkdir -p "$X"/{config,data,cache,state}
base=(env -u DISPLAY -u DBUS_SESSION_BUS_ADDRESS WAYLAND_DISPLAY=wincalc-shot XDG_CONFIG_HOME="$X/config"
      XDG_DATA_HOME="$X/data" XDG_CACHE_HOME="$X/cache" XDG_STATE_HOME="$X/state" GTK_A11Y=none NO_AT_BRIDGE=1
      dbus-run-session --config-file="$SP/privbus.conf" --)
BOXES=${BOXES-1}  # BOXES= (empty) leaves the hook unset
WSIZE=${WSIZE:-760x640}  # not SIZE: the nix shell sets SIZE=size (binutils)
shot_g() { # NAME ENV...
  local n=$1; shift
  "${base[@]}" env GMNB_SCREENSHOT="$OUT/gmnb-$n.png" GMNB_SIZE=$WSIZE GMNB_STILL=1 "$@" \
    timeout 30 "$BIN/gmnb" >>"$OUT/log.txt" 2>&1 || echo "FAIL gmnb $n" >>"$OUT/log.txt"
}
shot_d() {
  local n=$1; shift
  "${base[@]}" env DGMNB_SCREENSHOT="$OUT/dgmnb-$n.png" ${BOXES:+DGMNB_TEXT_BOXES="$OUT/dgmnb-$n.json"} DGMNB_SIZE=$WSIZE "$@" \
    timeout 30 "$BIN/dgmnb" >>"$OUT/log.txt" 2>&1 || echo "FAIL dgmnb $n" >>"$OUT/log.txt"
}
modes=(standard scientific programmer date graphing currency length)
EQS='y=sin(x)*3;x^2/4-3;y<cos(x)-4'
if [ "$WHICH" != dgmnb ]; then
  for p in aurora ember abyss orchard graphite system custom; do
    for d in 0 1; do
      for m in "${modes[@]}"; do shot_g "$m-$p-d$d" GMNB_MODE=$m GMNB_PALETTE=$p GMNB_DARK=$d; done
      shot_g "standard-$p-d$d-op10" GMNB_MODE=standard GMNB_PALETTE=$p GMNB_DARK=$d GMNB_OPACITY=0.1
      shot_g "standard-$p-d$d-op50" GMNB_MODE=standard GMNB_PALETTE=$p GMNB_DARK=$d GMNB_OPACITY=0.5
    done
  done
  for d in 0 1; do
    p=aurora
    shot_g "standard-$p-d$d-keys" GMNB_MODE=standard GMNB_PALETTE=$p GMNB_DARK=$d GMNB_KEYS='1234*56=789/3='
    shot_g "nav-$p-d$d" GMNB_MODE=standard GMNB_PALETTE=$p GMNB_DARK=$d GMNB_NAV=1
    shot_g "bits-$p-d$d" GMNB_MODE=programmer GMNB_PALETTE=$p GMNB_DARK=$d GMNB_BITS=1
    shot_g "graphing-eqs-$p-d$d" GMNB_MODE=graphing GMNB_PALETTE=$p GMNB_DARK=$d GMNB_EQUATIONS="$EQS"
    WSIZE=360x600 shot_g "small-$p-d$d" GMNB_MODE=standard GMNB_PALETTE=$p GMNB_DARK=$d
  done
fi
if [ "$WHICH" != gmnb ]; then
  for d in 0 1; do
    for m in "${modes[@]}"; do shot_d "$m-d$d" DGMNB_MODE=$m DGMNB_DARK=$d; done
    shot_d "standard-d$d-keys" DGMNB_MODE=standard DGMNB_DARK=$d DGMNB_KEYS='1234*56=789/3='
    shot_d "nav-d$d" DGMNB_MODE=standard DGMNB_DARK=$d DGMNB_NAV=1
    shot_d "graphing-eqs-d$d" DGMNB_MODE=graphing DGMNB_DARK=$d DGMNB_EQUATIONS="$EQS"
    WSIZE=360x600 shot_d "small-d$d" DGMNB_MODE=standard DGMNB_DARK=$d
    shot_d "standard-d$d-green" DGMNB_MODE=standard DGMNB_DARK=$d DGMNB_ACCENT=25d08d
  done
fi
echo "$(ls "$OUT"/*.png | wc -l) pictures, $(grep -c FAIL "$OUT/log.txt") failures"
