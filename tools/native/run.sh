#!/usr/bin/env bash
# Native check of GMNB or DGMNB on Wayland: a headless private session, real
# keystrokes (with modifiers) and the display read back over AT-SPI.
# FOR DEVELOPMENT CHECKS ONLY: not part of the build, the tests or CI.
#
#     tools/native/run.sh gmnb  target/debug/gmnb
#     tools/native/run.sh dgmnb target/debug/dgmnb
#     tools/native/run.sh gmnb  target/debug/gmnb edit
#
# It never touches your desktop: WAYLAND_DISPLAY and DISPLAY are unset, and
# everything runs in a private XDG_RUNTIME_DIR (keep its path short: Unix
# sockets are limited to 108 bytes), temporary XDG config/data/cache/state
# directories, a private D-Bus session, and its own accessibility bus. It
# starts, and in the end kills by PID:
#   sway (WLR_BACKENDS=headless, no input devices)  the compositor
#   at-spi-bus-launcher                             the accessibility bus
#   wtype, kept running to hold a keyboard on the seat
#   the app, and short-lived wtype (typing) and atspi.py (reading) runs.
#
# Needs: bash, dbus-run-session, sway, wtype, at-spi-bus-launcher and
# at-spi2-registryd (from at-spi2-core), and python3 with PyGObject and the
# Atspi-2.0 typelib. From the Nix binary cache, for example:
#     nix build nixpkgs#sway nixpkgs#wtype nixpkgs#at-spi2-core
# then point SWAY, WTYPE, ATSPI_LAUNCHER, ATSPI_REGISTRYD, PYTHON and
# GI_TYPELIB_PATH at them (each defaults to what is on PATH). Optional:
# PRIVBUS_CONF, a dbus-daemon config without service directories, to use
# instead of the built-in one; NATIVE_TMP, where the private directory goes
# (default /tmp); NATIVE_DUMP=1 to print the accessibility tree on failure;
# WL_COPY, wl-copy (wl-clipboard), for the edit steps to paste a text of
# several lines from another program (on the private compositor), instead of
# one copied in the app.
#
# Steps: type "2+3=" and expect the display to say 5; clear with Escape; type
# "2+3", Tab to the 7 key and press Shift+Enter there (with Shift held it is
# not "=", as in the original: the display stays 3), then plain Enter (5).
#
# Steps with "edit": the graphing page's equation field, its undo and its
# slider (R20-M-04, R20-M-05, R20-L-03). y=a*x+1 set over AT-SPI, a set to
# 0.3; all of it selected and x+a*x+2 typed a key at a time, a pause after
# each: a is 0.3 whenever its slider shows, and one Ctrl+Z gives
# y=a*x+1 back, with a 0.3; a second the empty field; Ctrl+Shift+Z y=a*x+1,
# a 0.3, and Ctrl+Y x+a*x+2 again. Each of AT-SPI's SetTextContents,
# InsertText and DeleteText (in GMNB: DGMNB's AccessKit offers only
# SetTextContents) is taken back by one Ctrl+Z (and redone), and
# "\n\n y=a*x+2\ny=3" set is " y=a*x+2". A keypad key over a selection and
# a paste over the whole text are one Ctrl+Z each. In GMNB, the field's
# context menu (Shift+F10) has an Undo (U), which takes back an edit too.
# Pointer input (wlrctl, swaymsg) has not been tried.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
app=${1:?usage: run.sh gmnb|dgmnb BINARY [edit]}
bin=$(realpath "${2:?usage: run.sh gmnb|dgmnb BINARY [edit]}")
steps=${3:-basic}
case $steps in basic | edit) ;; *) echo "usage: run.sh gmnb|dgmnb BINARY [edit]" >&2; exit 2 ;; esac
SWAY=${SWAY:-sway}
WTYPE=${WTYPE:-wtype}
ATSPI_LAUNCHER=${ATSPI_LAUNCHER:-at-spi-bus-launcher}
ATSPI_REGISTRYD=${ATSPI_REGISTRYD:-at-spi2-registryd}
PYTHON=${PYTHON:-python3}

if [ -z "${NATIVE_INNER:-}" ]; then
  unset WAYLAND_DISPLAY DISPLAY WAYLAND_SOCKET DBUS_SESSION_BUS_ADDRESS AT_SPI_BUS_ADDRESS
  rt=$(mktemp -d "${NATIVE_TMP:-/tmp}/nat.XXXXXX")
  chmod 700 "$rt"
  conf=${PRIVBUS_CONF:-$rt/bus.conf}
  if [ ! -f "$conf" ]; then
    cat > "$conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:tmpdir=$rt</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
  fi
  export XDG_RUNTIME_DIR=$rt NATIVE_RT=$rt NATIVE_INNER=1
  export XDG_CONFIG_HOME=$rt/config XDG_DATA_HOME=$rt/data
  export XDG_CACHE_HOME=$rt/cache XDG_STATE_HOME=$rt/state
  mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_STATE_HOME"
  rc=0
  dbus-run-session --config-file="$conf" -- bash "$0" "$@" || rc=$?
  rm -rf "$rt"
  exit "$rc"
fi

# ---- inside the private session bus ---------------------------------------
rt=$NATIVE_RT
pids=()
cleanup() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  for p in "${pids[@]}"; do wait "$p" 2>/dev/null || true; done
}
trap cleanup EXIT
fail() {
  echo "FAIL: $*" >&2
  echo "--- applications on the accessibility bus:" >&2
  atspi apps >&2 || true
  if [ -n "${NATIVE_DUMP:-}" ]; then atspi dump "$app" >&2 || true; fi
  [ -n "${apppid:-}" ] && { kill -0 "$apppid" 2>/dev/null && echo "(the app is running)" >&2 || echo "(the app has exited)" >&2; }
  for f in sway app atspi registryd; do
    [ -s "$rt/$f.log" ] && { echo "--- $f.log" >&2; tail -n 15 "$rt/$f.log" >&2; }
  done
  exit 1
}
atspi() { "$PYTHON" "$here/atspi.py" "$@"; }

# The accessibility bus (its registry daemon is started by the launcher).
# The launcher keeps its enabled flags in GSettings: keep them in memory.
GSETTINGS_BACKEND=memory "$ATSPI_LAUNCHER" --launch-immediately >"$rt/atspi.log" 2>&1 &
pids+=($!)
atspi enable
# Nothing here activates services on demand, so the registry is started too.
"$ATSPI_REGISTRYD" >"$rt/registryd.log" 2>&1 &
pids+=($!)
sleep 0.5

# The compositor: wlroots' headless backend, no libinput, software rendering.
cat > "$rt/sway.conf" <<EOF
output HEADLESS-1 resolution 1280x1024
default_border none
focus_follows_mouse no
EOF
WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
  "$SWAY" --unsupported-gpu -c "$rt/sway.conf" >"$rt/sway.log" 2>&1 &
pids+=($!)
sock=
for _ in $(seq 100); do
  sock=$(cd "$rt" && ls -d wayland-[0-9]* 2>/dev/null | grep -v '\.lock$' | head -n1 || true)
  [ -n "$sock" ] && [ -S "$rt/$sock" ] && break
  sleep 0.1
done
[ -S "$rt/${sock:-none}" ] || { cat "$rt/sway.log" >&2; fail "the compositor did not start"; }
export WAYLAND_DISPLAY=$sock

# The seat has no keyboard until wtype makes one, and a window is focused (and
# says so over AT-SPI) only while the seat has one. Each wtype makes a keyboard
# of its own and drops it on exit, so one is kept alive for the whole run by a
# wtype that only sleeps; the keys come from short-lived ones.
"$WTYPE" -s 3600000 &
pids+=($!)
sleep 1.5

# The app. GMNB is told not to look for a GPU.
GSK_RENDERER=cairo GMNB_STILL=1 "$bin" >"$rt/app.log" 2>&1 &
apppid=$!
pids+=($apppid)
atspi display "$app" >/dev/null || fail "$app never showed a display"
keys() { "$WTYPE" -s 150 "$@"; }
settle() { sleep 0.6; }

basic() {
  echo "start: display = $(atspi display "$app")"

  got=
  for _ in 1 2 3 4 5; do
    keys -k Escape
    keys "2+3="
    settle
    got=$(atspi display "$app")
    [ "$got" = 5 ] && break
  done
  echo "typed 2+3=: display = $got"
  [ "$got" = 5 ] || fail "expected 5"

  keys -k Escape
  settle
  cleared=$(atspi display "$app")
  echo "Escape: display = $cleared"
  [ "$cleared" = 0 ] || fail "expected the display to clear to 0"

  # Shift+Enter on a focused button: Tab to the 7 key, with "2+3" typed. Enter
  # on a calculator key is "=" (here 5); with Shift held it is nothing, as in the
  # original (R18-L-02): the display stays 3. The modifier gets through.
  keys -k Escape
  keys "2+3"
  settle
  echo "typed 2+3: display = $(atspi display "$app")"
  button=
  for _ in $(seq 40); do
    keys -k Tab
    button=$(atspi focused "$app" 2>/dev/null || true)
    case $button in 7 | Seven*) break ;; esac
  done
  echo "Tab: focused = $button"
  case $button in 7 | Seven*) ;; *) fail "never reached the 7 key by Tab" ;; esac
  keys -M shift -k Return -m shift
  settle
  held=$(atspi display "$app")
  echo "Shift+Enter on '$button': display = $held"
  [ "$held" = 3 ] || fail "expected Shift+Enter to leave 3"
  keys -k Return
  settle
  plain=$(atspi display "$app")
  echo "Enter on '$button': display = $plain"
  [ "$plain" = 5 ] || fail "expected Enter to give 5"
}

# The edit steps ("edit", above). The equation's text and a's value are read
# over AT-SPI after each step; a is "none" while no slider is shown.
equation() { atspi text "$app" Equation; }
slider_a() { atspi value "$app" "Variable a"; }
expect() {
  local text a
  text=$(equation)
  a=$(slider_a)
  echo "$1: equation = '$text', a = $a"
  [ "$text" = "$2" ] || fail "$1: expected the equation '$2'"
  [ "$a" = "$3" ] || fail "$1: expected a = $3"
}
undo() { keys -M ctrl -k z -m ctrl; settle; }
redo() { keys -M ctrl -M shift -k z -m shift -m ctrl; settle; }
focus_equation() {
  for _ in $(seq 60); do
    [ "$(atspi focused "$app" 2>/dev/null || true)" = Equation ] && return
    keys -k Tab
  done
  fail "never reached the equation by Tab"
}
settext() { atspi settext "$app" Equation "$1" || fail "SetTextContents refused"; settle; }

edit() {
  keys -M alt -k 3 -m alt
  settle
  for _ in $(seq 20); do atspi text "$app" Equation >/dev/null 2>&1 && break; sleep 0.5; done
  focus_equation
  settext "y=a*x+1"
  atspi setvalue "$app" "Variable a" 0.3 || fail "a refused 0.3"
  settle
  expect "start" "y=a*x+1" 0.3

  # R20-M-04: typed over a key at a time. A valid x on the way uses no a.
  focus_equation
  keys -M ctrl -k a -m ctrl
  local c text a
  for c in x + a '*' x + 2; do
    keys "$c"
    sleep 0.4
    text=$(equation)
    a=$(slider_a)
    echo "typed '$c': equation = '$text', a = $a"
    # Not shown while the text doesn't parse ("x+a*"); shown, it is 0.3.
    [ "$a" = none ] || [ "$a" = 0.3 ] || fail "a is $a at '$text'"
  done
  expect "typed" "x+a*x+2" 0.3
  undo
  expect "Ctrl+Z" "y=a*x+1" 0.3
  undo
  expect "Ctrl+Z again" "" none
  redo
  expect "Ctrl+Shift+Z" "y=a*x+1" 0.3
  keys -M ctrl -k y -m ctrl
  settle
  expect "Ctrl+Y" "x+a*x+2" 0.3
  undo
  expect "Ctrl+Z" "y=a*x+1" 0.3

  # R20-M-05: an assistive technology's edits are the user's, one each.
  settext "y=a*x+3"
  expect "SetTextContents" "y=a*x+3" 0.3
  undo
  expect "Ctrl+Z" "y=a*x+1" 0.3
  redo
  expect "Ctrl+Shift+Z" "y=a*x+3" 0.3
  undo
  # R20-L-03: one line of it, the first that isn't blank.
  settext $'\n\n y=a*x+2\ny=3'
  expect "SetTextContents of lines" " y=a*x+2" 0.3
  undo
  expect "Ctrl+Z" "y=a*x+1" 0.3
  # AccessKit's AT-SPI adapter (DGMNB's) offers SetTextContents alone.
  if [ "$app" = gmnb ]; then
    atspi inserttext "$app" Equation 6 0 || fail "InsertText refused"
    settle
    expect "InsertText" "y=a*x+01" 0.3
    undo
    expect "Ctrl+Z" "y=a*x+1" 0.3
    atspi deletetext "$app" Equation 0 2 || fail "DeleteText refused"
    settle
    expect "DeleteText" "a*x+1" 0.3
    undo
    expect "Ctrl+Z" "y=a*x+1" 0.3
  fi

  # R19-M-03's controls: a keypad key over a selection, a paste over the
  # whole text: one Ctrl+Z each.
  focus_equation
  keys -k End
  keys -M shift -k Left -m shift
  atspi press "$app" 2 || fail "the keypad's 2 refused"
  settle
  expect "keypad 2 over the 1" "y=a*x+2" 0.3
  undo
  expect "Ctrl+Z" "y=a*x+1" 0.3
  redo
  expect "Ctrl+Shift+Z" "y=a*x+2" 0.3
  undo
  local pasted
  if [ -n "${WL_COPY:-}" ]; then
    printf '\n\ny=a*x+5\ny=6' | "$WL_COPY" --foreground --type 'text/plain;charset=utf-8' &
    pids+=($!)
    pasted="y=a*x+5"
  else
    settext "y=a*x+5"
    focus_equation
    keys -M ctrl -k a -m ctrl
    keys -M ctrl -k c -m ctrl
    settle
    undo
    pasted="y=a*x+5"
  fi
  settle
  focus_equation
  keys -M ctrl -k a -m ctrl
  keys -M ctrl -k v -m ctrl
  settle
  expect "paste over all of it" "$pasted" 0.3
  undo
  expect "Ctrl+Z" "y=a*x+1" 0.3
  redo
  expect "Ctrl+Shift+Z" "$pasted" 0.3
  undo
  expect "Ctrl+Z" "y=a*x+1" 0.3

  # GMNB: the context menu's Undo, as upstream's equation box has one.
  # GTK 4.22 leaves a GtkText's menu out of the accessibility tree (the
  # GtkText isn't presented, nor anything in it), so it is opened with
  # Shift+F10 and its Undo chosen by its mnemonic, U: typed into the field
  # instead, the u would show in the equation.
  if [ "$app" = gmnb ]; then
    settext "y=a*x+4"
    focus_equation
    keys -M shift -k F10 -m shift
    settle
    local focused
    focused=$(atspi focused "$app" 2>/dev/null || true)
    echo "Shift+F10: focused = '$focused', menu items in the tree: $(atspi items "$app" | wc -l)"
    [ "$focused" != Equation ] || fail "the context menu didn't take the focus"
    keys u
    settle
    expect "context menu Undo" "y=a*x+1" 0.3
    [ "$(atspi focused "$app" 2>/dev/null || true)" = Equation ] || fail "the menu stayed open"
  fi
}

$steps

kill -0 "$apppid" 2>/dev/null || fail "the app exited"
echo "OK: $app"
