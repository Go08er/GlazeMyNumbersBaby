#!/usr/bin/env bash
# Reference screenshots for the theming work (DGMNB also writes the boxes of its text and curves next to each picture, dgmnb-NAME.json), taken fully isolated: the
# private headless weston (socket wincalc-shot), temporary XDG dirs, and a
# private D-Bus with no services (no portal: the default accent, no desktop
# settings). Usage, from the repo root, inside the nix dev shell:
#   tools/theming/goldens.sh OUTDIR BINDIR [gmnb|dgmnb|all|pages]
# BINDIR holds the gmnb and dgmnb binaries to capture (theming-work/v02-bin
# for the v0.2.0 references).
set -u
SP=/tmp/claude-1000/-home-goober-Documents-wincalc/a4c4533e-9373-47f7-85c3-9ead96a053d7/scratchpad
OUT=${1:?outdir}; BIN=${2:?bindir}; WHICH=${3:-all}
mkdir -p "$OUT"
fresh() { # a new profile (the pages' shots: a graph saved by one isn't restored by the next)
  X=$(mktemp -d "$SP/xdg.XXXX"); mkdir -p "$X"/{config,data,cache,state}
  base=(env -u DISPLAY -u DBUS_SESSION_BUS_ADDRESS WAYLAND_DISPLAY=wincalc-shot XDG_CONFIG_HOME="$X/config"
        XDG_DATA_HOME="$X/data" XDG_CACHE_HOME="$X/cache" XDG_STATE_HOME="$X/state" GTK_A11Y=none NO_AT_BRIDGE=1
        dbus-run-session --config-file="$SP/privbus.conf" --)
}
fresh
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
if [ "$WHICH" = base ]; then
  # The whole comparison of the base look with DGMNB (docs/theming.md,
  # "Acceptance: the base look is DGMNB's"): every pair of base.map, both
  # apps, each in a fresh profile, GMNB in its base look with DGMNB's
  # accent. Then the comparison itself, with base.map's exclusions only,
  # and every failing run of text. Exit status: 0 when every pair passes.
  # - Drawn by the toolkit's software renderer (GSK_RENDERER=cairo unless
  #   set): the same pixels on any machine, and no GPU.
  # - Offline, in a namespace of its own with no network (unshare -rn):
  #   the converter shows its bundled rates and their time in both apps,
  #   with no fetch racing the picture. NET=online captures with the
  #   network (the currency's rates and time are then live data).
  # - ONLY=REGEX captures and compares only the cases it matches
  #   (ONLY='date|currency'); the other pictures in OUT are kept.
  # - least.map's pairs (the least window) are compared after base.map's
  #   and reported; they are not in the exit status.
  export GSK_RENDERER=${GSK_RENDERER:-cairo}
  HERE=$(cd "$(dirname "$0")" && pwd)
  ONLY=${ONLY:-}
  if [ "${NET:-offline}" = offline ]; then
    unshare -rn true || { echo "goldens.sh: no network namespace (unshare -rn); NET=online to capture with the network" >&2; exit 2; }
    offline=(unshare -rn)
  else
    offline=()
  fi
  base_g=(GMNB_THEME=base GMNB_ACCENT=3b6fd8); base_d=(DGMNB_ACCENT=3b6fd8)
  pair() { # NAME "GMNB ENV" "DGMNB ENV": one case of both apps
    local n=$1 g d
    [ -z "$ONLY" ] || [[ $n =~ $ONLY ]] || return 0
    read -ra g <<<"$2"; read -ra d <<<"$3"
    rm -f "$OUT/gmnb-$n.png" "$OUT/dgmnb-$n.png" "$OUT/dgmnb-$n.json"
    fresh; base=("${offline[@]}" "${base[@]}"); shot_g "$n" "${base_g[@]}" "${g[@]}"; rm -rf "$X"
    fresh; base=("${offline[@]}" "${base[@]}"); shot_d "$n" "${base_d[@]}" "${d[@]}"; rm -rf "$X"
  }
  # Date with a result: the calendar's cell at row 2, column 3 (Sunday
  # first) of this month, DGMNB by two clicks (the To button, then the
  # cell); adding days: DGMNB by a click on the switch's second half, then
  # one off it (the pointer would light it).
  first=$(date +%Y-%m-01); wd=$(date -d "$first" +%w)
  PICK=$(date -d "$first -$wd days +17 days" +%Y-%m-%d)
  rm -f "$OUT/log.txt"
  for d in 0 1; do
    for m in standard scientific programmer date currency length graphing; do
      pair "$m-d$d" "GMNB_MODE=$m GMNB_DARK=$d" "DGMNB_MODE=$m DGMNB_DARK=$d"
    done
    pair "standard-d$d-keys" "GMNB_MODE=standard GMNB_DARK=$d GMNB_KEYS=1234*56=789/3=" \
      "DGMNB_MODE=standard DGMNB_DARK=$d DGMNB_KEYS=1234*56=789/3="
    pair "nav-d$d" "GMNB_MODE=standard GMNB_DARK=$d GMNB_NAV=1" "DGMNB_MODE=standard DGMNB_DARK=$d DGMNB_NAV=1"
    pair "bits-d$d" "GMNB_MODE=programmer GMNB_DARK=$d GMNB_BITS=1" \
      "DGMNB_MODE=programmer DGMNB_DARK=$d DGMNB_CLICKS=62,260;600,500"
    pair "compact-d$d" "GMNB_MODE=standard GMNB_DARK=$d GMNB_COMPACT=1 GMNB_KEYS=123" \
      "DGMNB_MODE=standard DGMNB_DARK=$d DGMNB_KEYS={alt+up}123"
    pair "date-result-d$d" "GMNB_MODE=date GMNB_DARK=$d GMNB_DATE_TO=$PICK" \
      "DGMNB_MODE=date DGMNB_DARK=$d DGMNB_CLICKS=176,221;166,413"
    pair "date-add-d$d" "GMNB_MODE=date GMNB_DARK=$d GMNB_DATE_ADD=1" \
      "DGMNB_MODE=date DGMNB_DARK=$d DGMNB_CLICKS=377,72;600,600"
    pair "graphing-eqs-d$d" "GMNB_MODE=graphing GMNB_DARK=$d GMNB_EQUATIONS=$EQS" \
      "DGMNB_MODE=graphing DGMNB_DARK=$d DGMNB_EQUATIONS=$EQS"
    WSIZE=360x600 pair "small-d$d" "GMNB_MODE=standard GMNB_DARK=$d" "DGMNB_MODE=standard DGMNB_DARK=$d"
    for m in scientific programmer; do
      WSIZE=300x400 pair "$m-least-d$d" "GMNB_MODE=$m GMNB_DARK=$d" "DGMNB_MODE=$m DGMNB_DARK=$d"
    done
  done
  echo "$(grep -c '^screenshot:' "$OUT/log.txt") pictures, $(grep -c FAIL "$OUT/log.txt") failures"
  CMP=${COMPARE:-$HERE/target/release/compare}
  [ -x "$CMP" ] || cargo build --quiet --release --manifest-path "$HERE/Cargo.toml" || exit 2
  compare() { # MAP: the table, then every failing run of text
    local map=$1
    if [ -n "$ONLY" ]; then
      map=$OUT/only-$(basename "$1")
      grep -v '^\s*#' "$1" | while read -r dn rest; do
        [[ ${dn#dgmnb-} =~ $ONLY ]] && echo "$dn $rest"
      done >"$map"
      [ -s "$map" ] || return 0
    fi
    "$CMP" --batch "$OUT" "$OUT" --map "$map" --report-dir "$OUT/report"
    local status=$?
    echo "== every failing run of text"
    grep -v '^\s*#' "$map" | while read -r dn gn rest; do
      [ -n "$dn" ] || continue
      ex=(); for e in $rest; do ex+=(--exclude "${e#exclude=}"); done
      "$CMP" "$OUT/$dn.png" "$OUT/$gn.png" --text "$OUT/$dn.json" "${ex[@]}" 2>&1 | grep '^  run ' | sed "s/^/$dn:/"
    done
    return $status
  }
  compare "$HERE/base.map"
  status=$?
  echo "== beyond the contract's set (least.map; not in the exit status)"
  compare "$HERE/least.map"
  exit $status
fi
if [ "$WHICH" = pages ]; then
  # The base look against DGMNB on the date page, the converter and
  # graphing, both apps, the accent pinned (compare with --map
  # tools/theming/pages.map). "Date with a result" picks the calendar's
  # cell at row 2, column 3 (Sunday first) of this month: DGMNB by two
  # clicks (the To button, then the cell of its calendar card), GMNB by
  # GMNB_DATE_TO.
  first=$(date +%Y-%m-01); wd=$(date -d "$first" +%w)
  PICK=$(date -d "$first -$wd days +17 days" +%Y-%m-%d)
  CLICKS='176,221;166,413'
  base_g=(GMNB_THEME=base GMNB_ACCENT=3b6fd8); base_d=(DGMNB_ACCENT=3b6fd8)
  for d in 0 1; do
    for m in date currency length graphing; do
      fresh; shot_g "$m-d$d" "${base_g[@]}" GMNB_MODE=$m GMNB_DARK=$d
      fresh; shot_d "$m-d$d" "${base_d[@]}" DGMNB_MODE=$m DGMNB_DARK=$d
    done
    fresh; shot_g "date-result-d$d" "${base_g[@]}" GMNB_MODE=date GMNB_DARK=$d GMNB_DATE_TO=$PICK
    fresh; shot_d "date-result-d$d" "${base_d[@]}" DGMNB_MODE=date DGMNB_DARK=$d DGMNB_CLICKS="$CLICKS"
    fresh; shot_g "graphing-eqs-d$d" "${base_g[@]}" GMNB_MODE=graphing GMNB_DARK=$d GMNB_EQUATIONS="$EQS"
    fresh; shot_d "graphing-eqs-d$d" "${base_d[@]}" DGMNB_MODE=graphing DGMNB_DARK=$d DGMNB_EQUATIONS="$EQS"
  done
  echo "$(ls "$OUT"/*.png | wc -l) pictures, $(grep -c FAIL "$OUT/log.txt") failures"
  exit 0
fi
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
