#!/usr/bin/env bash
# CPU packet regressions against the actual presenter, without a GL context.
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
timeout 30 "$test_dir/scene_texture_test"
