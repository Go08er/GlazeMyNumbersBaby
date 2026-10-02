#!/usr/bin/env bash
# Regenerates the golden files in crates/calcmanager/tests/data from the C++
# oracle. Deterministic: same driver + same arguments => same output.
#
# Usage: tools/oracle/calcmanager/gen.sh   (run inside the nix dev profile)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
OUT="${ORACLE_OUT:-$ROOT/target/agent-calcmanager/oracle}"
DATA="$ROOT/crates/calcmanager/tests/data"
mkdir -p "$DATA"

"$HERE/build.sh"

# suite  count  base-seed
SUITES=(
    "std  ${STD_COUNT:-700}  1"
    "sci  ${SCI_COUNT:-1100} 2"
    "prog ${PROG_COUNT:-800} 3"
    "mix  ${MIX_COUNT:-600}  4"
    "loc  ${LOC_COUNT:-300}  5"
)
CHUNKS="${CHUNKS:-8}"

for entry in "${SUITES[@]}"; do
    read -r suite count seed <<<"$entry"
    per=$(( (count + CHUNKS - 1) / CHUNKS ))
    pids=()
    for ((c = 0; c < CHUNKS; c++)); do
        first=$(( c * per ))
        n=$(( first + per > count ? count - first : per ))
        (( n > 0 )) || continue
        "$OUT/driver" "$suite" "$first" "$n" "$seed" >"$OUT/$suite.$c.part" 2>"$OUT/$suite.$c.log" &
        pids+=($!)
    done
    for p in "${pids[@]}"; do wait "$p"; done
    cat "$OUT/$suite".*.part >"$DATA/golden_$suite.txt"
    rm -f "$OUT/$suite".*.part
    cat "$OUT/$suite".*.log | grep -v '^$' | tail -n +1 >"$OUT/$suite.log"
    rm -f "$OUT/$suite".*.log
    echo "$suite: $(grep -c '^#S' "$DATA/golden_$suite.txt") sequences, $(du -h "$DATA/golden_$suite.txt" | cut -f1); $(grep -c '^discarded' "$OUT/$suite.log" || true) discarded" >&2
done
