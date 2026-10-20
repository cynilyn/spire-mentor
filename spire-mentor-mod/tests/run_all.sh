#!/usr/bin/env bash
# The mod's local gate on a box without the game installed: for EVERY
# supported game build, compile SpireMentor against that build's sts2.dll,
# check its binary binding (tests/compile/build_mod.sh) and run the
# BridgeServer wire test on the result (tests/wire); then the game-free unit
# tests (tests/unit). Exit 0 only if all of it passes.
#
# Usage: spire-mentor-mod/tests/run_all.sh [<game-version>...]
#   default: v0.107.1 v0.111.0
# Needs the .NET 9 SDK, sts2_binaries/<ver>/ (see build_mod.sh) and the
# oracle libs in sts2-pycli/lib/<ver>/ (git lfs pull) for the wire test.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
versions=("$@")
[[ ${#versions[@]} -gt 0 ]] || versions=(v0.107.1 v0.111.0)
for v in "${versions[@]}"; do
  echo "=== SpireMentor for $v: build + binding + wire test"
  "$HERE/compile/build_mod.sh" "$v" --wire
done
echo "=== SpireMentor unit tests"
"$HERE/unit/run_unit_tests.sh"
echo "run_all.sh: OK (${versions[*]})"
