#!/usr/bin/env bash
# Builds the C++ CalcManager oracle driver from the unmodified reference
# sources into target/agent-calcmanager/oracle.
#
# Usage: tools/oracle/calcmanager/build.sh   (run inside the nix dev profile)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
SRC="$ROOT/reference/calculator/src/CalcManager"
OUT="${ORACLE_OUT:-$ROOT/target/agent-calcmanager/oracle}"
OBJ="$OUT/obj"
mkdir -p "$OBJ"

# Engine strings: generated from the Rust provider's table so both sides
# use exactly the same resources.
python3 - "$ROOT/crates/calcmanager/src/resource.rs" "$OUT/strings.inc" <<'PY'
import re, sys
src = open(sys.argv[1], encoding="utf-8").read()
table = src[src.index("pub const EN_US_ENGINE_STRINGS"):]
table = table[:table.index("];")]
pairs = re.findall(r'\("((?:[^"\\]|\\.)*)",\s*"((?:[^"\\]|\\.)*)"\)', table)
with open(sys.argv[2], "w", encoding="utf-8") as f:
    for k, v in pairs:
        f.write('    { L"%s", L"%s" },\n' % (k, v))
print(f"strings.inc: {len(pairs)} strings", file=sys.stderr)
PY

# _GLIBCXX_DEBUG turns out-of-range container accesses (UB in the engine)
# into aborts, which the driver treats as "discard this sequence".
CXXFLAGS=(-std=c++20 -w -O2 -include cmath -D_GLIBCXX_DEBUG -I"$SRC" -I"$SRC/Header Files")

sources=()
for f in "$SRC"/Ratpack/*.cpp "$SRC"/CEngine/*.cpp "$SRC/CalculatorManager.cpp" "$SRC/CalculatorHistory.cpp" "$SRC/ExpressionCommand.cpp"; do
    sources+=("$f")
done

pids=()
objs=()
for f in "${sources[@]}"; do
    rel="${f#$SRC/}"
    o="$OBJ/$(echo "$rel" | tr '/' '_').o"
    objs+=("$o")
    if [[ ! -f "$o" || "$f" -nt "$o" || "$0" -nt "$o" ]]; then
        g++ "${CXXFLAGS[@]}" -c "$f" -o "$o" &
        pids+=($!)
    fi
done
for p in "${pids[@]}"; do wait "$p"; done

g++ "${CXXFLAGS[@]}" -I"$OUT" -c "$HERE/driver.cpp" -o "$OBJ/driver.o"
g++ -o "$OUT/driver" "$OBJ/driver.o" "${objs[@]}"
echo "built $OUT/driver" >&2
