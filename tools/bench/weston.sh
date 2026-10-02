#!/usr/bin/env bash
# Start a headless weston on socket "bench" (1× scale) unless it's running.
sock=${BENCH_SOCKET:-bench}
if [ ! -S "$XDG_RUNTIME_DIR/$sock" ]; then
  weston --backend=headless --renderer=gl --width=2000 --height=1400 \
    --socket="$sock" --idle-time=0 >/dev/null 2>&1 </dev/null &
  for _ in $(seq 50); do [ -S "$XDG_RUNTIME_DIR/$sock" ] && break; sleep 0.1; done
fi
echo "WAYLAND_DISPLAY=$sock"
