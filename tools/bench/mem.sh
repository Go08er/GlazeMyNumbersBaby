#!/usr/bin/env bash
# usage: [SETTLE=6] [CPU=10] mem.sh LABEL PROCNAME CMD [ARGS...]
#
# Launch CMD on the bench compositor, wait for a process named exactly
# PROCNAME (match with pgrep -x, never -f), let it settle, then print RSS,
# PSS, anonymous and shared-memory PSS (MiB) and idle CPU over CPU seconds.
# This is the method behind the README's memory table.
label=$1; name=$2; shift 2
( env WAYLAND_DISPLAY="${BENCH_SOCKET:-bench}" "$@" >/dev/null 2>&1 & )
P=
for _ in $(seq 80); do P=$(pgrep -x "$name" | head -1); [ -n "$P" ] && break; sleep 0.25; done
[ -z "$P" ] && { echo "$label: process $name never appeared"; exit 1; }
sleep "${SETTLE:-6}"
kb() { awk -v k="$1" '$1==k":" {print $2}' "/proc/$P/smaps_rollup"; }
mib() { echo "scale=1; $1/1024" | bc; }
rss=$(awk '/^VmRSS/ {print $2}' "/proc/$P/status")
t0=$(awk '{print $14+$15}' "/proc/$P/stat"); sleep "${CPU:-10}"; t1=$(awk '{print $14+$15}' "/proc/$P/stat")
cpu=$(awk -v a="$t0" -v b="$t1" -v hz="$(getconf CLK_TCK)" -v s="${CPU:-10}" 'BEGIN{printf "%.2f", (b-a)/hz/s*100}')
printf "%-28s RSS %6s  PSS %6s  anon %5s  shmem %5s  idleCPU %s%%  threads %s\n" "$label" \
  "$(mib "$rss")" "$(mib "$(kb Pss)")" "$(mib "$(kb Pss_Anon)")" "$(mib "$(kb Pss_Shmem)")" "$cpu" "$(ls /proc/$P/task | wc -l)"
kill "$P"; sleep 1
