#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
#
# Regenerates crates/ratpack/tests/data/ratpack_golden.txt from the ORIGINAL
# C++ ratpack:
#
#   1. builds the oracle (build.sh, needs g++),
#   2. generates the deterministic command list (gen_cases.py, needs python3),
#   3. evaluates every command with the C++ code and records the results.
#
# Usage (from anywhere; g++ must be on PATH, e.g. inside `nix develop`):
#   tools/oracle/ratpack/regen.sh
#
# Cases slower than SLOW_MS milliseconds are reported on stderr.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
golden="$root/crates/ratpack/tests/data/ratpack_golden.txt"
slow_ms="${SLOW_MS:-250}"

oracle="$("$here/build.sh" | tail -n 1)"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

python3 "$here/gen_cases.py" > "$tmp"
mkdir -p "$(dirname "$golden")"
"$oracle" "$slow_ms" < "$tmp" > "$golden"

echo "wrote $golden: $(wc -l < "$golden") cases" >&2
