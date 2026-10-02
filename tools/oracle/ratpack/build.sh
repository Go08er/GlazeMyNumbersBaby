#!/usr/bin/env bash
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
#
# Builds the C++ ratpack oracle from the ORIGINAL Windows Calculator sources
# (reference/calculator/src/CalcManager) into target/agent-ratpack/oracle.
#
# Must be run with a g++ on PATH (e.g. inside `nix develop`).
#
# -fwrapv: the C++ relies on two's-complement wrap-around in a few places
# (e.g. `digit *= 2` in _divnumx, numtoi32, exponent arithmetic); MSVC wraps,
# and the Rust port uses wrapping arithmetic, so make GCC wrap too instead of
# treating it as undefined behaviour.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
src="$root/reference/calculator/src/CalcManager"
out="${ORACLE_OUT:-$root/target/agent-ratpack/oracle}"

mkdir -p "$out/obj"

CXX="${CXX:-g++}"
CXXFLAGS=(-std=c++20 -O2 -fwrapv -w -include cmath -I"$src" -I"$src/Header Files")

sources=(
    "$src"/Ratpack/basex.cpp
    "$src"/Ratpack/conv.cpp
    "$src"/Ratpack/exp.cpp
    "$src"/Ratpack/fact.cpp
    "$src"/Ratpack/itrans.cpp
    "$src"/Ratpack/itransh.cpp
    "$src"/Ratpack/logic.cpp
    "$src"/Ratpack/num.cpp
    "$src"/Ratpack/rat.cpp
    "$src"/Ratpack/support.cpp
    "$src"/Ratpack/trans.cpp
    "$src"/Ratpack/transh.cpp
    "$src"/CEngine/Number.cpp
    "$src"/CEngine/Rational.cpp
    "$src"/CEngine/RationalMath.cpp
    "$here"/driver.cpp
)

objs=()
pids=()
for f in "${sources[@]}"; do
    o="$out/obj/$(basename "${f%.cpp}").o"
    if [[ ! -f "$o" || "$f" -nt "$o" || "$0" -nt "$o" ]]; then
        echo "CXX $(basename "$f")" >&2
        "$CXX" "${CXXFLAGS[@]}" -c "$f" -o "$o" &
        pids+=($!)
    fi
    objs+=("$o")
done
for pid in "${pids[@]}"; do
    wait "$pid"
done

"$CXX" -o "$out/ratpack_oracle" "${objs[@]}"
echo "$out/ratpack_oracle"
