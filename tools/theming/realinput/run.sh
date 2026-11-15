#!/usr/bin/env bash
# Runs a scenario over themes, dark values and opacities, under lim.
#   tools/theming/realinput/run.sh WxH SCENARIO [OUTDIR]
# Environment: THEMES (default "base aurora sprinkles swirls"), DARKS ("0 1"),
# OPACITIES (default "default", the app's own; e.g. "1 0.6"; GMNB only), APP
# (gmnb, or dgmnb: no themes, so THEMES is ignored), BIN, LIM_JOBS (default 2), LIM (the
# lim script). Each combination runs in its own compositor and writes to
# OUTDIR/<theme>-d<D>[-o<O>]/. The app's env for the combination reaches the
# scenario's `app` calls as THEME_ENV, so `app` alone is enough.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
RES=${1:?WxH}; SCN=${2:?scenario}; OUTBASE=${3:-realinput-out}
REPO=$(cd "$HERE/../../.." && pwd)
# The project's CPU and memory cap (theming-work/tools/lim); without one, runs uncapped.
LIM=${LIM:-$REPO/theming-work/tools/lim}
[ -x "$LIM" ] || LIM=env
export CARGO_INCREMENTAL=0 LIM_JOBS=${LIM_JOBS:-2}
APP=${APP:-gmnb}; export APP
if [ "$APP" = dgmnb ]; then themes=dgmnb; else themes=${THEMES:-base aurora sprinkles swirls}; fi
fail=0
for t in $themes; do
  for d in ${DARKS:-0 1}; do
    for o in ${OPACITIES:-default}; do
      if [ "$APP" = dgmnb ]; then
        env_="DGMNB_DARK=$d"  # no opacity setting in DGMNB
      else
        env_="GMNB_THEME=$t GMNB_DARK=$d"; [ "$o" != default ] && env_="$env_ GMNB_OPACITY=$o"
      fi
      name=$t-d$d; [ "$o" != default ] && name=$name-o$o
      mkdir -p "$OUTBASE/$name"
      THEME_ENV="$env_" OUT="$OUTBASE/$name" "$LIM" "$HERE/sw.sh" "$RES" "$SCN" \
        || { echo "FAILED: $name" >&2; fail=1; }
    done
  done
done
exit $fail
