#!/usr/bin/env bash
# Regenerates THIRD-PARTY-LICENSES.txt (at the repository root) from
# Cargo.lock: cargo-about lists the licence files of every crate GMNB, DGMNB
# and gmnb-launcher are built from (about.toml), and render.py writes them
# out with the code kept in this repository. Needs Nix; the tools come from
# the flake's nixpkgs, so the output only changes with the lock files.
set -euo pipefail
cd "$(dirname "$0")/../.."
json=$(mktemp)
trap 'rm -f "$json"' EXIT
nix shell --inputs-from . nixpkgs#cargo nixpkgs#cargo-about nixpkgs#python3 -c sh -euc '
  # cargo-about reads every platform'"'"'s crates; fetch them, then stay offline.
  cargo fetch --locked --quiet
  cargo-about generate --frozen -c packaging/licences/about.toml --format json -o "$1"
  python3 packaging/licences/render.py "$1" > THIRD-PARTY-LICENSES.txt
' sh "$json"
