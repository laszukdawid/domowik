#!/usr/bin/env bash
# CHROME_EXECUTABLE for `flutter test --platform chrome` (issue 0088): runs the
# pinned Chrome for Testing with every argument Flutter passes EXCEPT
# --disable-gpu, which Flutter adds to headless runs and which removes the
# WebGPU adapter the browser 3D tests need, and adding the host's WebGPU test
# switches (chrome-for-testing.sh --webgpu-flags; SwiftShader on Linux).
set -euo pipefail
here=$(cd -P "$(dirname "$0")" && pwd -P)
chrome=${DOMOWIK_TEST_CHROME:-$("$here/chrome-for-testing.sh")}
# shellcheck disable=SC2207 # the flags are space-separated switches
args=($("$here/chrome-for-testing.sh" --webgpu-flags))
for arg in "$@"; do
  [ "$arg" = "--disable-gpu" ] || args+=("$arg")
done
exec "$chrome" "${args[@]}"
