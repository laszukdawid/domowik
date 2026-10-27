#!/usr/bin/env bash
# Packet regressions, plus an offscreen GL pixel regression when Xvfb is present.
# Run after the Linux Flutter build has supplied its engine headers/library.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
source_dir="$root/frontend/linux/runner"
engine="$root/frontend/linux/flutter/ephemeral"
test_dir=$(mktemp -d "${TMPDIR:-/tmp}/domowik-scene-packets.XXXXXX")
trap 'rm -f "$test_dir/scene_texture_test"; rmdir "$test_dir"' EXIT
c++ -std=c++17 -pthread "$source_dir/scene_texture_test.cc" \
  -I"$engine" -L"$engine" -Wl,-rpath,"$engine" -lflutter_linux_gtk \
  $(pkg-config --cflags --libs gtk+-3.0 epoxy) -o "$test_dir/scene_texture_test"
timeout 30 "$test_dir/scene_texture_test" -s /scene/linear-light-beige
if command -v xvfb-run >/dev/null 2>&1; then
  GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 DOMOWIK_TEST_GL=1 xvfb-run -a timeout 30 "$test_dir/scene_texture_test" -p /scene/linear-light-beige
else
  echo "Linux scene lighting: SKIPPED - Xvfb not on PATH"
fi
