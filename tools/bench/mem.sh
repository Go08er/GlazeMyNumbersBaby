#!/usr/bin/env bash
# usage: [SETTLE=6] [CPU=10] mem.sh LABEL PROCNAME CMD [ARGS...]
#
# Launch CMD on the bench compositor and measure that process. CMD must exec
# the app (an `env ...` prefix does), so the launched PID is the app's;
# PROCNAME confirms it. It runs in its own process group, so a wrapper that
# forks instead is cleaned up with everything it started. Let it settle,
# then print RSS, PSS, anonymous and shared-memory PSS (MiB) and idle CPU
# over CPU seconds, and stop it. Other running copies of the app are never
# touched. This is the method behind the README's table.
label=$1; name=$2; shift 2
# setsid execs (this shell's background jobs aren't group leaders), so $! is
# the app and also its process group.
setsid env WAYLAND_DISPLAY="${BENCH_SOCKET:-bench}" "$@" >/dev/null 2>&1 &
P=$!
stop() { kill -- -"$P" 2>/dev/null; wait "$P" 2>/dev/null || true; }
alive() { [ "$(cat "/proc/$P/comm" 2>/dev/null)" = "$name" ]; }
for _ in $(seq 80); do alive && break; sleep 0.25; done
alive || { echo "$label: PID $P never became $name (does CMD exec it?)"; stop; exit 1; }
sleep "${SETTLE:-6}"
alive || { echo "$label: $name (PID $P) exited while settling"; stop; exit 1; }
kb() { awk -v k="$1" '$1==k":" {print $2}' "/proc/$P/smaps_rollup"; }
mib() { echo "scale=1; $1/1024" | bc; }
rss=$(awk '/^VmRSS/ {print $2}' "/proc/$P/status")
t0=$(awk '{print $14+$15}' "/proc/$P/stat"); sleep "${CPU:-10}"; t1=$(awk '{print $14+$15}' "/proc/$P/stat")
alive || { echo "$label: $name (PID $P) exited while measuring"; stop; exit 1; }
cpu=$(awk -v a="$t0" -v b="$t1" -v hz="$(getconf CLK_TCK)" -v s="${CPU:-10}" 'BEGIN{printf "%.2f", (b-a)/hz/s*100}')
printf "%-28s RSS %6s  PSS %6s  anon %5s  shmem %5s  idleCPU %s%%  threads %s\n" "$label" \
  "$(mib "$rss")" "$(mib "$(kb Pss)")" "$(mib "$(kb Pss_Anon)")" "$(mib "$(kb Pss_Shmem)")" "$cpu" "$(ls /proc/$P/task | wc -l)"
stop
exit 0
