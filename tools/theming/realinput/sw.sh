#!/usr/bin/env bash
# "Real input" harness: a private headless sway with a real keyboard (wtype)
# and pointer (sway's own IPC), to drive GMNB and DGMNB as a person would.
#   tools/theming/realinput/sw.sh WxH SCENARIO
# Environment: BIN (the folder with gmnb and dgmnb, default target/release),
# OUT (where `shot` writes, default ./realinput-out), APP (gmnb or dgmnb).
# The scenario is a bash file sourced inside the compositor; see
# tools/theming/README.md. Run it under theming-work/tools/lim (run.sh does).
# Everything is isolated: a user+net namespace (no network), a private D-Bus,
# a fresh XDG_RUNTIME_DIR and XDG dirs, no sound; every process it starts is
# killed by PID on exit.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)

if [ -z "${SW_INNER:-}" ]; then
  [ $# -ge 2 ] || { echo "usage: sw.sh WxH SCENARIO" >&2; exit 2; }
  SCN=$(readlink -f "$2")
  [ -f "$SCN" ] || { echo "sw.sh: no scenario $2" >&2; exit 2; }
  rt=$(mktemp -d "${TMPDIR:-/tmp}/realinput.XXXXXX"); chmod 700 "$rt"
  mkdir -p "$rt"/{config,data,cache,state,run}; chmod 700 "$rt/run"
  mkdir -p "${OUT:-realinput-out}"; OUT=$(readlink -f "${OUT:-realinput-out}")
  BIN=$(readlink -f "${BIN:-$REPO/target/release}")
  trap 'rm -rf "$rt"' EXIT
  # DGMNB (winit) loads libxkbcommon at run time: find it in the same pinned nixpkgs.
  XKBLIB=$(nix build --inputs-from "$REPO" nixpkgs#libxkbcommon --print-out-paths --no-link)/lib
  # nix shell puts sway, wtype and grim on PATH, from the repo's pinned nixpkgs.
  env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS -u PULSE_SERVER -u PULSE_RUNTIME_PATH \
    -u SWAYSOCK SW_INNER=1 XKBLIB="$XKBLIB" SW_RT="$rt" OUT="$OUT" BIN="$BIN" \
    nix shell --inputs-from "$REPO" nixpkgs#sway nixpkgs#wtype nixpkgs#grim nixpkgs#python3 -c \
    unshare -rn env XDG_RUNTIME_DIR="$rt/run" XDG_CONFIG_HOME="$rt/config" XDG_DATA_HOME="$rt/data" \
      XDG_CACHE_HOME="$rt/cache" XDG_STATE_HOME="$rt/state" \
      dbus-run-session --config-file="$HERE/../privbus.conf" -- bash "$0" "$1" "$SCN"
  exit $?
fi

# ---- inside the namespace, on the private bus ----
unset DISPLAY WAYLAND_DISPLAY PULSE_SERVER PULSE_RUNTIME_PATH
res=${1:?WxH}; SCN=${2:?scenario}
PIDS=()
cleanup() {
  trap '' EXIT INT TERM HUP
  local p
  for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done
  for p in "${PIDS[@]}"; do
    for _ in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$p" 2>/dev/null || break; sleep 0.2; done
    kill -9 "$p" 2>/dev/null
    wait "$p" 2>/dev/null
  done
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP
ip link set lo up 2>/dev/null

cfg="$SW_RT/sway.conf"
cat > "$cfg" <<CFG
output HEADLESS-1 resolution $res
default_border none
focus_follows_mouse no
for_window [app_id=".*"] floating enable, move position 0 0
CFG
WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
  sway --unsupported-gpu -c "$cfg" >"$SW_RT/sway.log" 2>&1 &
PIDS+=($!)
s=
for _ in $(seq 100); do
  s=$(ls "$XDG_RUNTIME_DIR" 2>/dev/null | grep -m1 '^wayland-[0-9]*$') && [ -n "$s" ] && break
  sleep 0.1
done
[ -n "$s" ] || { echo "sw.sh: sway did not start" >&2; cat "$SW_RT/sway.log" >&2; exit 1; }
export WAYLAND_DISPLAY=$s
SWAYSOCK=
for _ in $(seq 100); do
  SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock 2>/dev/null | head -1)
  [ -n "$SWAYSOCK" ] && break
  sleep 0.1
done
export SWAYSOCK
wtype -s 3600000 &
PIDS+=($!)
# A headless sway has no pointer device, so its seat has no pointer: hold a
# virtual one open (vptr.py), then drive it with sway's own IPC.
python3 "$HERE/vptr.py" >"$SW_RT/vptr.log" 2>&1 &
PIDS+=($!)
sleep 0.5

mkdir -p "$OUT"
APPPID=
# app [ENV=VALUE...]: start $BIN/$APP (gmnb, or dgmnb) with the given env.
# THEME_ENV (set by run.sh) is added first. Its log is $SW_RT/app.log, copied
# to $OUT/app.log on exit.
app() {
  # shellcheck disable=SC2086
  env GSK_RENDERER=cairo GMNB_STILL=1 GMNB_SOUND=null GTK_A11Y=none NO_AT_BRIDGE=1 \
    LD_LIBRARY_PATH="$XKBLIB${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ${THEME_ENV:-} "$@" "$BIN/${APP:-gmnb}" >"$SW_RT/app.log" 2>&1 &
  APPPID=$!; PIDS+=($APPPID)
  sleep "${APP_WAIT:-3}"
}
keys() { wtype -s 80 "$@"; sleep "${KEY_WAIT:-0.4}"; }
# isokey: Shift+Tab as a real keyboard sends it (ISO_Left_Tab); wtype's
# `-M shift -k Tab` does not move GTK's focus backwards.
isokey() { wtype -M shift -k ISO_Left_Tab -m shift; sleep "${KEY_WAIT:-0.4}"; }
_ptr() { swaymsg -q seat seat0 cursor "$@"; }
point() { _ptr set "$1" "$2"; }
press() { _ptr press "${1:-button1}"; }
release() { _ptr release "${1:-button1}"; }
click() { point "$1" "$2"; sleep 0.1; press; sleep 0.05; release; sleep "${CLICK_WAIT:-0.3}"; }
# scroll X Y AMOUNT: the wheel (positive down), as button5/button4 presses.
scroll() {
  point "$1" "$2"; local n=${3:-1} b=button5
  if [ "$n" -lt 0 ]; then b=button4; n=$((-n)); fi
  while [ "$n" -gt 0 ]; do press "$b"; release "$b"; n=$((n-1)); sleep 0.1; done
}
wait_() { sleep "$1"; }
shot() { grim "$OUT/$1.png"; }
size() { swaymsg -q "[app_id=.*] resize set width $1 px height $2 px"; sleep 0.5; }

# shellcheck disable=SC1090
. "$SCN"
cp "$SW_RT/app.log" "$OUT/app.log" 2>/dev/null
